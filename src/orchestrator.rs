#![allow(dead_code)]

mod conversations;
mod event_senders;
mod queue;

use crate::galaxy_setup::{PlanetMap, galaxy_loader};
use crate::orchestrator::conversations::{PossibleMessage, SendersToExplorer, SendersToPlanet};
use crate::orchestrator::queue::ConvoScheduler;
use crate::{get_id_manager, payload};

use crate::logging_utils::{log_internal, log_msg_to};
use crate::orchestrator::conversations::ToExplorerStruct;
use crate::orchestrator::conversations::ToPlanetStruct;
use crate::orchestrator::conversations::orch_explorer::kill_explorer::KillExplorerConversation;
use crate::orchestrator::conversations::orch_explorer::kill_explorer::SendingKillExplorer;
use common_explorer::ExplorerBagContent;
use common_game::components::forge::Forge;
use common_game::logging::{ActorType, Channel, EventType};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::protocols::planet_explorer::{ExplorerToPlanet, PlanetToExplorer};
use common_game::utils::ID;
use crossbeam_channel::unbounded;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

type ExplorersLocationRef = Arc<Mutex<HashMap<ID, ID>>>;

type ExplorerBag = ExplorerBagContent;

pub(crate) struct PlanetExplorerChannels {
    planet_to_explorer_senders: Arc<Mutex<HashMap<ID, Sender<PlanetToExplorer>>>>,
    explorer_to_planet_senders: Arc<Mutex<HashMap<ID, Sender<ExplorerToPlanet>>>>,
}

pub(crate) struct Orchestrator {
    planets_senders: SendersToPlanet,
    explorer_senders: SendersToExplorer,
    planets_receiver: Receiver<PlanetToOrchestrator>,
    explorers_receiver: Receiver<OrchestratorToExplorer>,
    forge: Arc<Forge>,
    convo_scheduler: ConvoScheduler<ExplorerBag>,
    galaxy: PlanetMap,
    planet_explorer_channels: PlanetExplorerChannels,
    explorers_location: ExplorersLocationRef,
    planet_threads: Vec<std::thread::JoinHandle<()>>,
}

impl Orchestrator {
    pub fn new(file_path: &std::path::Path) -> Self {
        //TODO: fix receivers and senders initialization
        let (galaxy, planets_receiver, orch_to_plan_senders, expl_to_plan_senders) =
            galaxy_loader(file_path);
        let (explorers_receiver, explorer_senders) =
            (unbounded::<OrchestratorToExplorer>().1, HashMap::new());
        let forge = Arc::new(Forge::new().expect("Couldn't create forge!"));

        let mut planet_explorer_channels = PlanetExplorerChannels::new();
        planet_explorer_channels.explorer_to_planet_senders =
            Arc::new(Mutex::new(expl_to_plan_senders));

        // Spawn planet threads immediately
        let planet_threads = {
            let mut handles = Vec::new();
            let map = galaxy.lock().unwrap();
            for node in map.values() {
                let inner = Arc::clone(&node.inner);
                let node_id = node.id;
                let handle = std::thread::spawn(move || {
                    let mut inner_guard = inner.lock().unwrap();
                    let planet = &mut inner_guard.planet;
                    let res = planet.run();

                    if let Err(e) = res {
                        log_internal(
                            Channel::Error,
                            payload!(
                                action : "Planet encountered an error during its main loop",
                                planet_id : node_id,
                                error : e,
                            ),
                        );
                    }
                });
                handles.push(handle);
            }
            handles
        };

        Self {
            planets_senders: Arc::new(Mutex::new(orch_to_plan_senders)),
            explorer_senders: Arc::new(Mutex::new(explorer_senders)),
            planets_receiver,
            explorers_receiver,
            forge,
            galaxy,
            convo_scheduler: ConvoScheduler::new(),
            planet_explorer_channels,
            explorers_location: Arc::new(Mutex::new(HashMap::new())),
            planet_threads,
        }
    }

    /// Sends an `OrchestratorToPlanet` to the correspondent `planet_id`. Returns nothing if successful, a String error otherwise
    fn to_planet(&self, planet_id: ID, msg: OrchestratorToPlanet) -> Result<(), String> {
        log_msg_to(
            Channel::Trace,
            EventType::MessageOrchestratorToPlanet,
            (ActorType::Planet, planet_id),
            payload!(
                message : format!("{:?}", msg)
            ),
        );

        self.planets_senders
            .lock()
            .unwrap()
            .get(&planet_id)
            .ok_or(format!("Planet {planet_id} not found"))?
            .send(msg)
            .map_err(|err| format!("Failed to send to Planet {planet_id}: {err}"))
    }

    /// Sends an `OrchestratorToExplorer` to the correspondent `explorer_id`. Returns nothing if successful, a String error otherwise
    fn to_explorer(&self, explorer_id: ID, msg: OrchestratorToExplorer) -> Result<(), String> {
        log_msg_to(
            Channel::Trace,
            EventType::MessageOrchestratorToExplorer,
            (ActorType::Explorer, explorer_id),
            payload!(
                message : format!("{:?}", msg)
            ),
        );

        self.explorer_senders
            .lock()
            .unwrap()
            .get(&explorer_id)
            .ok_or(format!("Explorer {explorer_id} not found"))?
            .send(msg)
            .map_err(|err| format!("Failed to send to Explorer {explorer_id}: {err}"))
    }

    fn handle_message(&mut self, message: PossibleMessage<ExplorerBag>) {
        let message_kind = message.to_kind_type();
        let entities_ids = message.get_entity_ids();

        let matching_conversation = self
            .convo_scheduler
            .find_matching_conversation(&message_kind, entities_ids);

        match matching_conversation {
            // If the message matches the expected kind, we let the message wait for the transition
            Some(conversation) => {
                self.convo_scheduler
                    .add_waiting_message(conversation.get_id(), message);
            }
            None => {
                match message {
                    PossibleMessage::ExplorerToOrch(msg) => {
                        match msg {
                            #[allow(unused_variables)]
                            ExplorerToOrchestrator::NeighborsRequest {
                                explorer_id,
                                current_planet_id,
                            } => {
                                let to_explorer_struct =
                                    crate::orchestrator::conversations::ToExplorerStruct {
                                        explorer_id,
                                        explorers_senders: self.explorer_senders.clone(),
                                    };
                                let state = conversations::orch_explorer::neighbors_discovery::WaitingExplorerNeighborsRequest::new(
                                    to_explorer_struct,
                                    self.galaxy.clone(),
                                );
                                let new_conv = conversations::orch_explorer::neighbors_discovery::NeighborsDiscoveryConversation::<conversations::orch_explorer::neighbors_discovery::WaitingExplorerNeighborsRequest>::new(
                                    explorer_id,
                                    state,
                                );
                                self.convo_scheduler.add_conversation(Box::new(new_conv)
                                    as Box<
                                        dyn conversations::Conversation<ExplorerBag> + Send + Sync,
                                    >);
                            }
                            #[allow(unused_variables)]
                            ExplorerToOrchestrator::TravelToPlanetRequest {
                                explorer_id,
                                current_planet_id,
                                dst_planet_id,
                            } => todo!(),
                            // The other messages are responses that do not start a conversation
                            _ => {
                                log_internal(
                                    Channel::Debug,
                                    payload!(
                                        action : "Received ExplorerToOrchestrator message that does not start a conversation. Ignoring.",
                                        message_kind : format!{"{:?}", message_kind},
                                        from_explorer : entities_ids.1.unwrap(),
                                    ),
                                );
                            }
                        }
                    }
                    // Since the planet never starts a conversation, we just ignore these messages
                    PossibleMessage::PlanetToOrch(_) => {
                        log_internal(
                            Channel::Debug,
                            payload!(
                                action : "Received PlanetToOrchestrator message that does not start a conversation. Ignoring.",
                                message_kind : format!{"{message_kind:?}"},
                                from_planet : entities_ids.0.unwrap(),
                            ),
                        );
                    }
                }
            }
        }
    }

    /// Starts a background thread that periodically sends asteroids to random planets.
    pub fn start_asteroid_sender(&self) {
        event_senders::start_asteroid_sender(
            self.planets_senders.clone(),
            self.forge.clone(),
            self.explorers_location.clone(),
            self.explorer_senders.clone(),
            self.convo_scheduler.clone(),
            self.galaxy.clone(),
        );
    }

    /// Starts a background thread that periodically sends sunrays to random planets.
    pub fn start_sunray_sender(&self) {
        event_senders::start_sunray_sender(
            self.planets_senders.clone(),
            self.forge.clone(),
            self.explorers_location.clone(),
            self.convo_scheduler.clone(),
            self.galaxy.clone(),
        );
    }

    fn process_messages(&mut self) {
        let convo_scheduler = self.convo_scheduler.clone();
        let explorer_senders = self.explorer_senders.clone();
        let planets_senders = self.planets_senders.clone();
        thread::spawn(move || {
            loop {
                if convo_scheduler.is_empty() {
                    // Wait for new messages to arrive
                    thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }

                let current_convo = convo_scheduler.get_next_conversation();

                if let Some(convo) = current_convo {
                    let msg = convo_scheduler.get_waiting_message(convo.get_id());
                    let tmp = convo.get_kill_explorers_vec();

                    if let Some((vec, handle_outgoing)) = tmp {
                        for el in vec {
                            let conv_id = get_id_manager().get_next_conversation_id();
                            let to_explorer_struct = ToExplorerStruct {
                                explorer_id: el.0,
                                explorers_senders: explorer_senders.clone(),
                            };
                            let to_planet_struct =
                                ToPlanetStruct::new(planets_senders.clone(), el.1);

                            let state_struct = SendingKillExplorer::new(
                                to_explorer_struct,
                                to_planet_struct,
                                handle_outgoing,
                            );

                            let convo = KillExplorerConversation::<SendingKillExplorer>::new(
                                conv_id,
                                state_struct,
                            );

                            convo_scheduler.add_conversation(Box::new(convo)
                                as Box<dyn conversations::Conversation<ExplorerBag> + Send + Sync>);
                        }
                    }

                    let new_convo = convo.transition(msg);

                    if let Some(new_real_convo) = new_convo {
                        convo_scheduler.add_conversation(new_real_convo);
                    }
                }
            }
        });
    }
    fn add_explorer(&mut self, _explorer_id: ID, planet_id: ID) {
        //to add a new explorer for the first time inside the game
        let (tx_expl_out, _rx_expl_out) = unbounded::<PlanetToExplorer>();
        self.planet_explorer_channels
            .add_plan_to_expl_sender(planet_id, tx_expl_out.clone());

        // TODO: broken because i changed dummy_explorer, we need to adapt this to actual explorers
        /*
        let mut explorer: dummy_explorer::Explorer<ExplorerBag> = dummy_explorer::Explorer::new();
        //TODO: set explorer - orchestrator channels
        explorer.set_planet_channels(
            rx_expl_out,
            self.planet_explorer_channels
                .get_expl_to_plan_sender(planet_id)
                .expect("Failed to get explorer to planet sender")
                .clone(),
        );

        let msg = OrchestratorToPlanet::IncomingExplorerRequest {
            explorer_id,
            new_sender: tx_expl_out.clone(),
        };
        self.to_planet(planet_id, msg).unwrap_or_else(|_| {
            panic!("Failed to send IncomingExplorerRequest to planet {planet_id}")
        });

        // Emit log event
        log_internal(
            Channel::Info,
            payload!(
                action : "Created Explorer",
                explorer_id: explorer_id,
                into_planet_id : planet_id,
            ),
        );
         */
    }
}

impl PlanetExplorerChannels {
    pub fn new() -> Self {
        Self {
            planet_to_explorer_senders: Arc::new(Mutex::new(HashMap::new())),
            explorer_to_planet_senders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn add_plan_to_expl_sender(&mut self, explorer_id: ID, sender: Sender<PlanetToExplorer>) {
        self.planet_to_explorer_senders
            .lock()
            .unwrap()
            .insert(explorer_id, sender);
    }

    pub fn add_expl_to_plan_sender(&mut self, planet_id: ID, sender: Sender<ExplorerToPlanet>) {
        self.explorer_to_planet_senders
            .lock()
            .unwrap()
            .insert(planet_id, sender);
    }

    pub fn get_plan_to_expl_sender(&self, explorer_id: ID) -> Option<Sender<PlanetToExplorer>> {
        self.planet_to_explorer_senders
            .lock()
            .unwrap()
            .get(&explorer_id)
            .cloned()
    }

    pub fn get_expl_to_plan_sender(&self, planet_id: ID) -> Option<Sender<ExplorerToPlanet>> {
        self.explorer_to_planet_senders
            .lock()
            .unwrap()
            .get(&planet_id)
            .cloned()
    }
}
