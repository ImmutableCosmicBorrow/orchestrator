#![allow(dead_code)]

mod conversations;
mod queue;

use crate::galaxy_setup::{PlanetMap, galaxy_loader};
use crate::orchestrator::conversations::{PossibleMessage, SendersToExplorer, SendersToPlanet};
use crate::orchestrator::queue::ConvoScheduler;
use crate::payload;

use crate::logging_utils::{log_internal, log_msg_to};
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
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::thread;

type ExplorersLocationRef = Arc<Mutex<HashMap<ID, ID>>>;

// Todo: Define what to store in the ExplorerBag
#[derive(Debug, Hash, Eq, PartialEq)]
pub(crate) struct ExplorerBag;

pub(crate) struct PlanetExplorerChannels {
    planet_to_explorer_senders: Arc<Mutex<HashMap<ID, Sender<PlanetToExplorer>>>>,
    explorer_to_planet_senders: Arc<Mutex<HashMap<ID, Sender<ExplorerToPlanet>>>>,
}

pub(crate) struct Orchestrator {
    planets_senders: SendersToPlanet,
    explorer_senders: SendersToExplorer,
    planets_receiver: Receiver<PlanetToOrchestrator>,
    explorers_receiver: Receiver<OrchestratorToExplorer>,
    forge: Forge,
    convo_scheduler: ConvoScheduler<ExplorerBag>,
    galaxy: PlanetMap,
    planet_explorer_channels: PlanetExplorerChannels,
    explorers_location: ExplorersLocationRef,
}

impl Orchestrator {
    pub fn new(file_path: &std::path::Path) -> Self {
        let (galaxy, planets_receiver, planets_senders) = galaxy_loader(file_path);
        let (explorers_receiver, explorer_senders) =
            (unbounded::<OrchestratorToExplorer>().1, HashMap::new());
        let forge = Forge::new().expect("Couldn't create forge!");

        let planet_explorer_channels = PlanetExplorerChannels {
            planet_to_explorer_senders: Arc::new(Mutex::new(HashMap::new())),
            explorer_to_planet_senders: Arc::new(Mutex::new(HashMap::new())),
        };

        Self {
            planets_senders: Arc::new(Mutex::new(planets_senders)),
            explorer_senders: Arc::new(Mutex::new(explorer_senders)),
            planets_receiver,
            explorers_receiver,
            forge,
            galaxy,
            convo_scheduler: ConvoScheduler::new(),
            planet_explorer_channels,
            explorers_location: Arc::new(Mutex::new(HashMap::new())),
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
            EventType::MessageOrchestratorToPlanet,
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
        let entity_id = message.get_entity_id();

        let matching_conversation = self
            .convo_scheduler
            .find_matching_conversation(&message_kind, entity_id);

        match matching_conversation {
            // If the message matches the expected kind, we let the message wait for the transition
            Some(_conversation) => {
                self.convo_scheduler.add_waiting_message(entity_id, message);
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
                                        from_explorer : entity_id,
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
                                from_planet : entity_id,
                            ),
                        );
                    }
                }
            }
        }
    }

    fn process_messages(&mut self) {
        let convo_scheduler = self.convo_scheduler.clone();
        thread::spawn(move || {
            loop {
                if convo_scheduler.is_empty() {
                    // Wait for new messages to arrive
                    thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }

                let current_convo = convo_scheduler.get_next_conversation();

                if current_convo.is_none() {
                    continue;
                }

                let msg =
                    convo_scheduler.get_waiting_message(current_convo.as_ref().unwrap().get_id());

                if msg.is_some()
                    && let Some(new_conv) = current_convo.unwrap().transition(msg)
                {
                    convo_scheduler.add_conversation(new_conv);
                }
            }
        });
    }
}
