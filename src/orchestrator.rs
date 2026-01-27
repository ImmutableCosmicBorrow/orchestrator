#![allow(dead_code)]

mod conversations;
mod event_senders;
mod queue;

use crate::galaxy_setup::galaxy_loader;
use crate::orchestrator::conversations::{PossibleMessage, SendersToExplorer, SendersToPlanet};
use crate::orchestrator::queue::ConvoScheduler;
use crate::planet::PlanetMap;
use crate::{get_id_manager, payload};

use crate::globals::{get_game_step, set_game_step};
use crate::logging_utils::log_internal;
use crate::orchestrator::conversations::ToExplorerStruct;
use crate::orchestrator::conversations::ToPlanetStruct;
use crate::orchestrator::conversations::orch_explorer::kill_explorer::KillExplorerConversation;
use crate::orchestrator::conversations::orch_explorer::kill_explorer::SendingKillExplorer;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, WaitingTravelRequest,
};
use crate::orchestrator::conversations::orch_explorer::start_explorer::{
    SendingExplorerStart, StartExplorerConversation,
};
use crate::orchestrator::conversations::orch_planet::{
    SendingPlanetStart, StartPlanetConversation,
};
use common_explorer::ExplorerAI;
pub(crate) use common_explorer::ExplorerBagContent;
use common_game::components::forge::Forge;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestrator::TravelToPlanetRequest;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::PlanetToOrchestrator;
use common_game::protocols::planet_explorer::{ExplorerToPlanet, PlanetToExplorer};
use common_game::utils::ID;
use crossbeam_channel::{Receiver, Sender, select, unbounded};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

type ExplorersLocationRef = Arc<Mutex<HashMap<ID, ID>>>;

pub enum ExplorerType {
    Rob,
    Nico,
    Jaco,
}
type ExplorerBag = ExplorerBagContent;

#[derive(Clone)]
pub(crate) struct PlanetExplorerChannels {
    planet_to_explorer_senders: Arc<Mutex<HashMap<ID, Sender<PlanetToExplorer>>>>,
    explorer_to_planet_senders: Arc<Mutex<HashMap<ID, Sender<ExplorerToPlanet>>>>,
}

pub struct Orchestrator {
    planets_senders: SendersToPlanet,
    explorer_senders: SendersToExplorer,
    planets_receiver: Receiver<PlanetToOrchestrator>,
    explorers_receiver: Receiver<ExplorerToOrchestrator<ExplorerBag>>,
    explorer_to_orchestrator_sender: Sender<ExplorerToOrchestrator<ExplorerBagContent>>,
    forge: Arc<Forge>,
    convo_scheduler: ConvoScheduler<ExplorerBag>,
    galaxy: PlanetMap,
    planet_explorer_channels: PlanetExplorerChannels,
    explorers_location: ExplorersLocationRef,
    planet_threads: std::sync::Arc<std::sync::Mutex<HashMap<ID, JoinHandle<()>>>>,
    explorer_threads: HashMap<ID, JoinHandle<()>>,
}

impl Orchestrator {
    /// Creates a new Orchestrator instance from a galaxy configuration file.
    ///
    /// # Panics
    ///
    /// Panics if the forge cannot be created.
    #[must_use]
    pub fn new(file_path: &std::path::Path, game_step: u64) -> Self {
        // Set static variable GAME_STEP
        set_game_step(game_step);

        // galaxy_loader now returns 5 values (the last one is planet thread handles)
        let (galaxy, planets_receiver, orch_to_plan_senders, expl_to_plan_senders, planet_threads) =
            galaxy_loader(file_path);

        let forge = Arc::new(Forge::new().expect("Couldn't create forge!"));

        // Channel for Explorers to Orchestrator communications
        let (tx_explorer_to_orchestrator, explorers_receiver) =
            unbounded::<ExplorerToOrchestrator<ExplorerBagContent>>();

        // Channels for Explorers - Planets communications
        let mut planet_explorer_channels = PlanetExplorerChannels::new();
        planet_explorer_channels.explorer_to_planet_senders =
            Arc::new(Mutex::new(expl_to_plan_senders));

        Self {
            planets_senders: Arc::new(Mutex::new(orch_to_plan_senders)),
            explorer_senders: Arc::new(Mutex::new(HashMap::new())),
            planets_receiver,
            explorers_receiver,
            explorer_to_orchestrator_sender: tx_explorer_to_orchestrator,
            forge,
            galaxy,
            convo_scheduler: ConvoScheduler::new(),
            planet_explorer_channels,
            explorers_location: Arc::new(Mutex::new(HashMap::new())),
            planet_threads: Arc::new(Mutex::new(planet_threads)), // threads were spawned in galaxy_loader/create_planet_with_channels
            explorer_threads: HashMap::new(),
        }
    }

    /// Runs the orchestrator, managing all planet and explorer conversations.
    ///
    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    pub fn run(&mut self) {
        // Send PlanetStart to all Planets
        for (id, _) in self.planets_senders.lock().unwrap().iter() {
            let to_planet = ToPlanetStruct::new(self.planets_senders.clone(), *id);
            let state = SendingPlanetStart::new(to_planet);
            let convo =
                StartPlanetConversation::new(get_id_manager().get_next_conversation_id(), state);
            self.convo_scheduler.add_conversation(Box::new(convo));
        }

        // Send ExplorerStart to all Explorers
        for (id, _) in self.explorer_senders.lock().unwrap().iter() {
            let to_explorer = ToExplorerStruct::new(self.explorer_senders.clone(), *id);
            let state = SendingExplorerStart::new(to_explorer);
            let convo =
                StartExplorerConversation::new(get_id_manager().get_next_conversation_id(), state);
            self.convo_scheduler.add_conversation(Box::new(convo));
        }

        // Start message processing thread
        self.process_messages();

        // Start background event senders
        self.start_background_event_senders();

        // Main loop
        loop {
            let timeout = crossbeam_channel::after(Duration::from_millis(100));
            select! {
                recv(self.planets_receiver) -> msg => {
                    match msg {
                        Ok(msg) => {
                            self.handle_message(PossibleMessage::PlanetToOrch(msg));
                        }
                        Err(e) => {
                            log_internal(
                                Channel::Warning,
                                payload!(
                                    action : "Error while receiving from Planets",
                                    error : e
                                )
                            );
                        }
                    }
                }
                recv(self.explorers_receiver) -> msg => {
                    match msg {
                        Ok(msg) => {
                            self.handle_message(PossibleMessage::ExplorerToOrch(msg));
                        }
                        Err(e) => {
                            log_internal(
                                Channel::Warning,
                                payload!(
                                    action : "Error while receiving from Explorers",
                                    error : e
                                )
                            );
                        }
                    }
                }
                // Periodic check to determine if there are any explorers left.
                // If none remain, shut the game down.
                recv(timeout) -> _ => {
                    if self.explorers_location.lock().unwrap().is_empty() {
                        log_internal(
                            Channel::Info,
                            payload!(
                                action : "No explorers left. Shutting down orchestrator",
                            )
                        );
                        std::process::exit(0);
                    }
                }
            }
        }
    }

    #[must_use]
    pub fn get_galaxy(&self) -> &PlanetMap {
        &self.galaxy
    }

    fn start_background_event_senders(&self) {
        crate::orchestrator::event_senders::init_background_event_scheduler(
            self.planets_senders.clone(),
            self.forge.clone(),
            self.explorers_location.clone(),
            self.explorer_senders.clone(),
            self.convo_scheduler.clone(),
            self.galaxy.clone(),
        );

        crate::orchestrator::event_senders::enable_sunrays();
        crate::orchestrator::event_senders::enable_asteroids();
    }

    //TODO: FIX CLIPPY ERRORS TOO MANY LINES
    #[allow(clippy::too_many_lines)]
    fn handle_message(&mut self, message: PossibleMessage<ExplorerBag>) {
        let message_kind = message.to_kind_type();
        let entities_ids = message.get_entity_ids();

        // Log every incoming message with source and intended receiver (Orchestrator)
        log_internal(
            Channel::Trace,
            payload!(
                event: "MessageReceived",
                message_kind: format!("{:?}", message_kind),
                from_planet: format!("{:?}", entities_ids.0),
                from_explorer: format!("{:?}", entities_ids.1),
                to: "Orchestrator"
            ),
        );

        let matching_conversation = self
            .convo_scheduler
            .find_matching_conversation(&message_kind, entities_ids);

        match matching_conversation {
            // If the message matches the expected kind, we let the message wait for the transition
            Some(conversation) => {
                // Log match with the conversation id
                log_internal(
                    Channel::Trace,
                    payload!(
                        event: "MessageMatchedConversation",
                        conversation_id: conversation.get_id(),
                        message_kind: format!("{:?}", message_kind),
                        from_planet: format!("{:?}", entities_ids.0),
                        from_explorer: format!("{:?}", entities_ids.1)
                    ),
                );
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
                                let to_explorer_struct = ToExplorerStruct::new(
                                    self.explorer_senders.clone(),
                                    explorer_id,
                                );
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

                                self.handle_message(PossibleMessage::ExplorerToOrch(
                                    ExplorerToOrchestrator::NeighborsRequest {
                                        explorer_id,
                                        current_planet_id,
                                    },
                                ));
                            }
                            #[allow(unused_variables)]
                            ExplorerToOrchestrator::TravelToPlanetRequest {
                                explorer_id,
                                current_planet_id,
                                dst_planet_id,
                            } => {
                                let to_explorer_struct = ToExplorerStruct::new(
                                    self.explorer_senders.clone(),
                                    explorer_id,
                                );
                                let curr_planet_struct = ToPlanetStruct::new(
                                    self.planets_senders.clone(),
                                    current_planet_id,
                                );
                                let dst_planet_struct = ToPlanetStruct::new(
                                    self.planets_senders.clone(),
                                    dst_planet_id,
                                );
                                let state = WaitingTravelRequest::new(
                                    self.galaxy.clone(),
                                    self.planet_explorer_channels.clone(),
                                    curr_planet_struct,
                                    dst_planet_struct,
                                    to_explorer_struct,
                                    self.explorers_location.clone(),
                                );
                                //TODO: WHY is ID explorer_id? should be a new conversation ID
                                let new_conv =
                                    MoveToPlanetConversation::<WaitingTravelRequest>::new(
                                        explorer_id,
                                        state,
                                    );
                                self.convo_scheduler.add_conversation(Box::new(new_conv));
                                self.handle_message(PossibleMessage::ExplorerToOrch(
                                    TravelToPlanetRequest {
                                        explorer_id,
                                        current_planet_id,
                                        dst_planet_id,
                                    },
                                ));
                            }
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

    fn process_messages(&mut self) {
        let convo_scheduler = self.convo_scheduler.clone();
        let explorer_senders = self.explorer_senders.clone();
        let planets_senders = self.planets_senders.clone();
        let explorer_locations = self.explorers_location.clone();
        let galaxy = self.galaxy.clone();
        let planet_threads = self.planet_threads.clone();
        thread::spawn(move || {
            loop {
                // Check for timed-out conversations and handle them
                // (will panic for conversations that don't override on_timeout)
                convo_scheduler.handle_timeouts();

                if convo_scheduler.is_empty() {
                    // Wait for new messages to arrive
                    thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }

                let current_convo = convo_scheduler.get_next_conversation();

                if let Some(convo) = current_convo {
                    let msg = convo_scheduler.get_waiting_message(convo.get_id());
                    let kill_expl_vec = convo.get_kill_explorers_vec();

                    if let Some((vec, handle_outgoing)) = kill_expl_vec {
                        for el in vec {
                            let conv_id = get_id_manager().get_next_conversation_id();
                            let to_explorer_struct =
                                ToExplorerStruct::new(explorer_senders.clone(), el.0);

                            let to_planet_struct =
                                ToPlanetStruct::new(planets_senders.clone(), el.1);

                            let state_struct = SendingKillExplorer::new(
                                to_explorer_struct,
                                to_planet_struct,
                                handle_outgoing,
                                explorer_locations.clone(),
                            );

                            let convo = KillExplorerConversation::<SendingKillExplorer>::new(
                                conv_id,
                                state_struct,
                            );

                            convo_scheduler.add_conversation(Box::new(convo)
                                as Box<dyn conversations::Conversation<ExplorerBag> + Send + Sync>);
                        }
                        // Remove the planet from the galaxy and notify the planet thread to stop.
                        if let (Some(planet_id), _) = convo.get_entities_ids() {
                            let planets_senders_clone = planets_senders.clone();
                            let galaxy_clone = galaxy.clone();
                            let planet_threads_clone = planet_threads.clone();
                            // remove_node_with_stop will remove the node from the PlanetMap and then
                            // call the provided closure to kill the planet (send KillPlanet and remove sender).
                            crate::planet::remove_node_with_stop(&galaxy_clone, planet_id, |dead_id| {
                                // remove and notify sender
                                let mut lock = planets_senders_clone.lock().unwrap();
                                if let Some(sender) = lock.remove(&dead_id) {
                                    let _ = sender.send(
                                        common_game::protocols::orchestrator_planet::OrchestratorToPlanet::KillPlanet,
                                    );
                                }

                                // remove and join the planet thread handle if present
                                if let Ok(mut th_lock) = planet_threads_clone.lock()
                                    && let Some(handle) = th_lock.remove(&dead_id)
                                {
                                    let _ = handle.join();
                                }
                            });
                        }
                    }

                    // Log transition execution with convo id, expected kind, and incoming message kind
                    match &msg {
                        Some(m) => log_internal(
                            Channel::Trace,
                            payload!(
                                event: "ConversationTransition",
                                conversation_id: convo.get_id(),
                                expected_kind: format!("{:?}", convo.get_expected_kind()),
                                message_kind: format!("{:?}", m.to_kind_type())
                            ),
                        ),
                        None => log_internal(
                            Channel::Trace,
                            payload!(
                                event: "ConversationTransition",
                                conversation_id: convo.get_id(),
                                expected_kind: format!("{:?}", convo.get_expected_kind()),
                                message_kind: "None"
                            ),
                        ),
                    }

                    if convo.get_expected_kind().is_some() {
                        let new_convo = convo.transition(msg);

                        if let Some(new_real_convo) = new_convo {
                            convo_scheduler.add_conversation(new_real_convo);
                        }
                    }
                }
            }
        });
    }
    /// Creates an Explorer and spawns its thread.
    ///
    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    pub fn add_explorer(&mut self, explorer_type: &ExplorerType, into_planet: ID) {
        // Create channels
        let (tx_orchestrator_to_explorer, rx_orchestrator_to_explorer) =
            unbounded::<OrchestratorToExplorer>();
        let (tx_planet_to_explorer, rx_planet_to_explorer) = unbounded::<PlanetToExplorer>();
        let id = get_id_manager().get_next_explorer_id();

        // Save Explorer - Planet channels
        self.planet_explorer_channels
            .planet_to_explorer_senders
            .lock()
            .unwrap()
            .insert(id, tx_planet_to_explorer);
        self.explorer_senders
            .lock()
            .unwrap()
            .insert(id, tx_orchestrator_to_explorer);

        if let Some(planet_sender) = self
            .planet_explorer_channels
            .explorer_to_planet_senders
            .lock()
            .unwrap()
            .get(&into_planet)
        {
            // If the planet sender exists, create the Explorer
            let mut explorer: Box<dyn ExplorerAI + Send> = match explorer_type {
                ExplorerType::Nico => Box::new(explorer_nico::Explorer::new(
                    id,
                    into_planet,
                    planet_sender.clone(),
                    self.explorer_to_orchestrator_sender.clone(),
                    rx_orchestrator_to_explorer,
                    rx_planet_to_explorer,
                    Duration::from_millis(get_game_step()),
                )),
                ExplorerType::Rob => {
                    todo!()
                }
                ExplorerType::Jaco => {
                    todo!()
                }
            };

            // Spawn the Explorer
            let handle = thread::spawn(move || {
                let result = explorer.run();

                if let Err(e) = result {
                    log_internal(
                        Channel::Warning,
                        payload!(
                            action: "Explorer thread ended with an error",
                            explorer_id : id,
                            error : e
                        ),
                    );
                } else {
                    log_internal(
                        Channel::Debug,
                        payload!(
                            action : "Explorer thread ended correctly",
                            explorer_id : id,
                        ),
                    );
                }
            });
            // Add handle to the hashmap
            self.explorer_threads.insert(id, handle);

            // TODO: Tell the Planet that an Explorer is coming
        } else {
            // If the sender for that Planet does not exist, log the warning and return
            log_internal(
                Channel::Warning,
                payload!(
                    action: "The specified Planet sender does not exist, the Explorer has not been created",
                    planet_id : into_planet,
                ),
            );
        }
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
