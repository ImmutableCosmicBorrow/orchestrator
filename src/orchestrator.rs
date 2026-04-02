#![allow(dead_code)]

mod background_events;
pub(crate) mod conversations;
pub mod convo_factory;
mod convo_router;
mod queue;
use crate::galaxy_setup::galaxy_loader;
use crate::orchestrator::conversations::PossibleMessage;
use crate::orchestrator::queue::ConvoScheduler;
use crate::planet::{self, PlanetMap};
use crate::{get_id_manager, payload};

use crate::channels_manager::ChannelsManager;
use crate::globals::{get_game_step, set_game_step};
use crate::logging_utils::{LogTarget, log_internal, log_msg_from};
use crate::orchestrator::conversations::ToExplorerStruct;
use crate::orchestrator::conversations::ToPlanetStruct;
use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
use common_explorer::ExplorerAI;
pub(crate) use common_explorer::ExplorerBagContent;
use common_game::components::forge::Forge;
use common_game::logging::{ActorType, Channel, EventType};
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestrator;
use common_game::protocols::orchestrator_planet::PlanetToOrchestrator;
use common_game::utils::ID;
use conversations::orch_explorer::lifecycle::kill_explorer::{
    KillExplorerConversation, SendingKillExplorer,
};
use crossbeam_channel::{Receiver, Sender, select};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

pub type ExplorersLocationRef = Arc<Mutex<HashMap<ID, ID>>>;

#[derive(Clone, Copy, Debug)]
pub enum ExplorerType {
    Vojager,  //Roberto
    Explorer, //Nicola
    Nomad,    //Jacopo
}

pub struct Orchestrator {
    channels_manager: Arc<ChannelsManager>,
    forge: Arc<Forge>,
    pub(crate) convo_scheduler: ConvoScheduler<ExplorerBagContent>,
    pub(crate) galaxy: PlanetMap,
    pub(crate) explorers_location: ExplorersLocationRef,
    planet_threads: Arc<Mutex<HashMap<ID, JoinHandle<()>>>>,
    explorer_threads: HashMap<ID, JoinHandle<()>>,
    message_processor_thread: Option<JoinHandle<()>>,
    message_processor_stop: Arc<AtomicBool>,
    shutdown_requested: bool,
    manual_mode: bool,
}

impl Orchestrator {
    // ---------------- PUBLIC API ---------------------

    /// Creates a new Orchestrator instance from a galaxy configuration file.
    /// - `file_path`: The path of the galaxy configuration file.
    /// - `explorer1`: The `ExplorerType` of the first Explorer.
    /// - `explorer2`: An optional `ExplorerType` for the optional second Explorer
    /// - `spawn_planet`: An `Option<ID>`. If provided, the Explorer will be spawned in this Planet, otherwise a random one will be chosen.
    /// - `game_step`: A parameter that regulates the speed of the Explorer's actions.
    ///
    /// # Panics
    ///
    /// Panics if the forge cannot be created, or if there are no Planets.
    #[must_use]
    pub fn new(
        file_path: &std::path::Path,
        game_step: u64,
        ui_sender: Sender<OrchestratorToUiUpdate>,
        ui_receiver: Receiver<UiToOrchestratorCommand>,
        explorer1: ExplorerType,
        explorer2: Option<ExplorerType>,
        spawn_planet: Option<ID>,
    ) -> Self {
        // Set static variable GAME_STEP
        set_game_step(game_step);

        let channels_manager = Arc::new(ChannelsManager::new(ui_sender, ui_receiver));

        // galaxy_loader now returns 2 values (galaxy and planet_threads), all channels are distributed inside using
        // channels manager APIs
        let (galaxy, planet_threads) = galaxy_loader(file_path, channels_manager.as_ref());

        let mut orchestrator = Self {
            forge: Arc::new(Forge::new().expect("Couldn't create forge!")),
            galaxy,
            channels_manager,
            convo_scheduler: ConvoScheduler::new(),
            explorers_location: Arc::new(Mutex::new(HashMap::new())),
            planet_threads: Arc::new(Mutex::new(planet_threads)), // threads were spawned in galaxy_loader/create_planet_with_channels
            explorer_threads: HashMap::new(),
            message_processor_thread: None,
            message_processor_stop: Arc::new(AtomicBool::new(false)),
            shutdown_requested: false,
            manual_mode: false,
        };

        orchestrator.orch_init(explorer1, explorer2, spawn_planet);

        // Return the Orchestrator
        orchestrator
    }

    fn orch_init(
        &mut self,
        explorer1: ExplorerType,
        explorer2: Option<ExplorerType>,
        spawn_planet: Option<ID>,
    ) {
        self.spawn_first_explorers(explorer1, explorer2, spawn_planet);
    }

    fn spawn_first_explorers(
        &mut self,
        explorer1: ExplorerType,
        explorer2: Option<ExplorerType>,
        spawn_planet: Option<ID>,
    ) {
        // Check where to spawn Explorers
        let planet_id = spawn_planet
            .filter(|id| self.channels_manager.to_planet_senders_contains(*id))
            .unwrap_or_else(|| {
                let fallback_id = self.channels_manager.to_planet_senders_next_id().expect("Planet senders hashmap is empty");
                // Log only if a planet was actually requested but not found
                if let Some(requested_id) = spawn_planet {
                    log_internal(
                        LogTarget::General,
                        Channel::Warning,
                        payload!(
                    action: "Parameter spawn_planet was Some, but Planet was not found. Spawning Explorer(s) in a random Planet instead",
                    missing_planet_id: requested_id,
                    random_planet_id_chosen: fallback_id
                ),
                    );
                }
                fallback_id
            });

        // Add first Explorer
        self.add_explorer(explorer1, planet_id);

        // If the second Explorer is some, add it too
        if let Some(explorer) = explorer2 {
            self.add_explorer(explorer, planet_id);
        }
    }

    /// Runs the orchestrator, managing all planet and explorer conversations.
    ///
    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    pub fn run(&mut self) {
        // Send PlanetStart to all Planets
        let planet_senders = self.channels_manager.get_to_planet_senders_struct_ref();
        for (id, _) in planet_senders.lock().unwrap().iter() {
            convo_factory::create_start_planet_conversation(
                &self.convo_scheduler,
                planet_senders,
                *id,
            );
        }

        // Send ExplorerStart to all Explorers
        let explorer_senders = self.channels_manager.get_orch_to_exp_senders_struct_ref();
        for (id, _) in explorer_senders.lock().unwrap().iter() {
            convo_factory::create_start_explorer_conversation(
                &self.convo_scheduler,
                explorer_senders,
                *id,
            );
        }

        // Start message processing thread
        self.process_messages();

        // Start background event senders
        self.start_background_event_senders();

        //Get receiving channels
        let from_planets_rcv = self.channels_manager.get_from_planet_rcv();
        let from_explorers_rcv = self.channels_manager.get_from_explorers_rcv();
        let from_ui_rcv = self.channels_manager.get_ui_receiver();
        // Main loop
        loop {
            let timeout = crossbeam_channel::after(get_game_step() + Duration::from_millis(1000));
            select! {
                recv(from_planets_rcv) -> msg => {
                    self.handle_planets_message(msg);
                }
                recv(from_explorers_rcv) -> msg => {
                    self.handle_explorers_message(msg);
                }

                recv(from_ui_rcv) -> msg => {
                    self.handle_ui_receiver_message(msg);
                }

                // Periodic check to determine if there are any explorers left.
                // If none remain, shut the game down.
                recv(timeout) -> _ => { //  TODO: send message to UI
                    if self.explorers_location.lock().unwrap().is_empty() {
                        log_internal(
                            LogTarget::General,
                            Channel::Info,
                            payload!(
                                action : "No explorers left. Shutting down orchestrator",
                            )
                        );
                        self.shutdown_requested = true;
                    }
                }
            }

            if self.shutdown_requested {
                self.shutdown();
                return;
            }
        }
    }

    fn handle_planets_message(
        &mut self,
        msg: Result<PlanetToOrchestrator, crossbeam_channel::RecvError>,
    ) {
        match msg {
            Ok(msg) => {
                log_msg_from(
                    LogTarget::ChannelMessages,
                    Channel::Debug,
                    EventType::MessagePlanetToOrchestrator,
                    (ActorType::Planet, msg.planet_id()),
                    payload!(
                        msg : format!("{msg:?}"),
                    ),
                );
                self.handle_message(PossibleMessage::PlanetToOrch(msg));
            }
            Err(e) => {
                log_internal(
                    LogTarget::General,
                    Channel::Warning,
                    payload!(
                        action : "Error while receiving from Planets",
                        error : e
                    ),
                );
            }
        }
    }

    fn handle_explorers_message(
        &mut self,
        msg: Result<ExplorerToOrchestrator<ExplorerBagContent>, crossbeam_channel::RecvError>,
    ) {
        match msg {
            Ok(msg) => {
                log_msg_from(
                    LogTarget::ChannelMessages,
                    Channel::Debug,
                    EventType::MessageExplorerToOrchestrator,
                    (ActorType::Explorer, msg.explorer_id()),
                    payload!(
                        msg : format!("{msg:?}"),
                    ),
                );
                self.handle_message(PossibleMessage::ExplorerToOrch(msg));
            }
            Err(e) => {
                log_internal(
                    LogTarget::General,
                    Channel::Warning,
                    payload!(
                        action : "Error while receiving from Explorers",
                        error : e
                    ),
                );
            }
        }
    }

    fn handle_ui_receiver_message(
        &mut self,
        msg: Result<UiToOrchestratorCommand, crossbeam_channel::RecvError>,
    ) {
        match msg {
            Ok(msg) => {
                log_internal(
                    LogTarget::ChannelMessages,
                    Channel::Debug,
                    payload!(
                        event : "UI->ORCH",
                        msg : format!("{msg:?}"),
                    ),
                );
                self.handle_ui_message(msg);
            }
            Err(e) => {
                log_internal(
                    LogTarget::General,
                    Channel::Warning,
                    payload!(
                        action : "Error while receiving from UI",
                        error : e
                    ),
                );
            }
        }
    }

    #[must_use]
    pub fn get_galaxy(&self) -> &PlanetMap {
        &self.galaxy
    }

    fn routing(&self) -> convo_router::ConvoRouter<'_> {
        convo_router::ConvoRouter::new(self)
    }

    /// Toggles between manual and automatic mode for the orchestrator.
    ///
    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    pub fn change_mode(&mut self) {
        self.manual_mode = !self.manual_mode;
        let explorer_senders = self.channels_manager.get_orch_to_exp_senders_struct();

        if self.manual_mode {
            log_internal(
                LogTarget::General,
                Channel::Info,
                payload!(
                    action : "Orchestrator switched to MANUAL mode",
                ),
            );

            for explorer_id in explorer_senders.lock().unwrap().keys() {
                self.routing().stop_explorer_ai(*explorer_id);
            }
            background_events::disable_asteroids();
        } else {
            log_internal(
                LogTarget::General,
                Channel::Info,
                payload!(
                    action : "Orchestrator switched to AUTOMATIC mode",
                ),
            );
            for explorer_id in explorer_senders.lock().unwrap().keys() {
                self.routing().start_explorer_ai(*explorer_id);
            }
            background_events::enable_asteroids();
        }
    }

    fn start_background_event_senders(&self) {
        background_events::init_background_event_scheduler(
            Arc::clone(&self.channels_manager),
            self.forge.clone(),
            self.explorers_location.clone(),
            self.convo_scheduler.clone(),
            self.galaxy.clone(),
        );

        background_events::enable_sunrays();
        background_events::enable_asteroids();
    }

    fn stop_background_event_senders(&self) {
        background_events::disable_sunrays();
        background_events::disable_asteroids();
    }

    fn handle_message(&mut self, message: PossibleMessage<ExplorerBagContent>) {
        let message_kind = message.to_kind_type();
        let entities_ids = message.get_entity_ids();
        let convo_id = self
            .convo_scheduler
            .find_matching_conversation(&message_kind, entities_ids)
            .or_else(|| self.try_create_conversation(&message, &message_kind, entities_ids));

        if let Some(id) = convo_id {
            log_internal(
                LogTarget::Conversations,
                Channel::Trace,
                payload!(
                    event: "Message matched conversation",
                    conversation_id: id,
                    message_kind: format!("{:?}", message_kind),
                    from_planet: format!("{:?}", entities_ids.0),
                    from_explorer: format!("{:?}", entities_ids.1)
                ),
            );
            self.convo_scheduler.add_waiting_message(id, message);
        }
    }

    fn try_create_conversation(
        &mut self,
        message: &PossibleMessage<ExplorerBagContent>,
        message_kind: &conversations::PossibleExpectedKinds,
        entities_ids: (Option<ID>, Option<ID>),
    ) -> Option<ID> {
        match message {
            PossibleMessage::ExplorerToOrch(msg) => match msg {
                ExplorerToOrchestrator::NeighborsRequest {
                    explorer_id,
                    current_planet_id: _,
                } => Some(convo_factory::create_neighbors_request_conversation(
                    &self.galaxy,
                    &self.convo_scheduler,
                    self.channels_manager.get_orch_to_exp_senders_struct_ref(),
                    *explorer_id,
                )),
                ExplorerToOrchestrator::TravelToPlanetRequest {
                    explorer_id,
                    current_planet_id,
                    dst_planet_id,
                } => Some(
                    convo_factory::create_waiting_travel_to_planet_request_conversation(
                        &self.convo_scheduler,
                        self.galaxy.clone(),
                        self.channels_manager.get_planet_explorer_struct(),
                        self.channels_manager.get_orch_to_exp_senders_struct_ref(),
                        self.channels_manager.get_to_planet_senders_struct_ref(),
                        &self.explorers_location,
                        *explorer_id,
                        *current_planet_id,
                        *dst_planet_id,
                    ),
                ),
                _ => {
                    log_internal(
                        LogTarget::General,
                        Channel::Warning,
                        payload!(
                            action: "Received ExplorerToOrchestrator message that does not start a conversation. Ignoring.",
                            message_kind: format!("{:?}", message_kind),
                            from_explorer: entities_ids.1.unwrap(),
                        ),
                    );
                    None
                }
            },
            PossibleMessage::PlanetToOrch(_) => {
                log_internal(
                    LogTarget::General,
                    Channel::Warning,
                    payload!(
                        action: "Received PlanetToOrchestrator message that does not start a conversation. Ignoring.",
                        message_kind: format!("{:?}", message_kind),
                        from_planet: entities_ids.0.unwrap(),
                    ),
                );
                None
            }
        }
    }

    /// Handles UI commands from the UI layer and creates appropriate conversations or performs direct actions.
    ///
    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    // TODO: remove the clippy allow once the function is refactored into smaller functions
    #[allow(clippy::too_many_lines)]
    pub fn handle_ui_message(&mut self, command: UiToOrchestratorCommand) {
        #[allow(clippy::enum_glob_use)]
        use UiToOrchestratorCommand::*;

        match command {
            // Rendering/Query Commands - Direct responses without conversations
            GetGalaxy => {
                let _ = self
                    .channels_manager
                    .get_ui_sender_ref()
                    .send(OrchestratorToUiUpdate::Galaxy(self.galaxy.clone()));
            }
            GetExplorersPosition => {
                let _ = self.channels_manager.get_ui_sender_ref().send(
                    OrchestratorToUiUpdate::ExplorersPosition(self.explorers_location.clone()),
                );
            }
            GetPlanetSnapshot(planet_id) => {
                self.routing().ask_internal_state(planet_id); //the conversation will send the update to UI
            }

            GetExplorerSnapshot(explorer_id) => {
                self.routing().ask_bag_content(explorer_id); //the conversation will send the update to UI
            }

            AddPlanet(planet_id, connected_planets) => {
                planet::add_planet_with_neighbors(&self.galaxy, planet_id, connected_planets);
            }

            AddExplorer(explorer_type, into_planet) => {
                self.add_explorer(explorer_type, into_planet);
            }

            SwitchGameMode => {
                self.change_mode();
            }
            EndGame => {
                log_internal(
                    LogTarget::General,
                    Channel::Info,
                    payload!(
                        action : "Received EndGame command from UI. Shutting down orchestrator",
                    ),
                );
                self.shutdown_requested = true;
            }
            PauseGame => {
                self.stop_background_event_senders();

                for explorer_id in self.channels_manager.get_orch_to_exp_senders_struct().lock().unwrap().keys() {
                    self.routing().stop_explorer_ai(*explorer_id);
                }
                
                for planet_id in self.channels_manager.get_to_planet_senders_struct().lock().unwrap().keys() {
                    self.routing().stop_planet_ai(*planet_id);
                }

                log_internal(
                    LogTarget::General,
                    Channel::Info,
                    payload!(
                        action : "Received PauseGame command from UI. Pausing background events and planet/explorer AIs",
                    ),
                );
            }
            ResumeGame => {
                self.start_background_event_senders();

                for explorer_id in self.channels_manager.get_orch_to_exp_senders_struct().lock().unwrap().keys() {
                    self.routing().start_explorer_ai(*explorer_id);
                }
                
                for planet_id in self.channels_manager.get_to_planet_senders_struct().lock().unwrap().keys() {
                    self.routing().start_planet_ai(*planet_id);
                }
                
                log_internal(
                    LogTarget::General,
                    Channel::Info,
                    payload!(
                        action : "Received ResumeGame command from UI. Resuming background events and planet/explorer AIs",
                    ),
                );
            }

            // Explorer Movement Commands
            ManualMoveExplorer(explorer_id, current_planet, dst_planet) => {
                self.routing().make_manual_travel_to_planet_request(
                    explorer_id,
                    current_planet,
                    dst_planet,
                );
            }

            // Explorer Resource Commands
            ExplorerGenerateResource(explorer_id, resource_type) => {
                self.routing().generate_resource(explorer_id, resource_type);
            }
            ExplorerCombineResource(explorer_id, resource_type) => {
                self.routing().combine_resource(explorer_id, resource_type);
            }

            SupportedCombinations(explorer_id) => {
                //it automatically sends the update to UI
                self.routing().ask_supported_combinations(explorer_id);
            }

            SupportedResources(explorer_id) => {
                //it automatically sends the update to UI
                self.routing().ask_supported_resources(explorer_id);
            }

            // Asteroid/Sunray Commands
            SendManualAsteroid(planet_id) => {
                self.routing().send_asteroid(planet_id);
            }

            SendManualSunray(planet_id) => {
                self.routing().send_sunray(planet_id);
            }

            // Planet AI Control Commands
            StartPlanetAI(planet_id) => {
                self.routing().start_planet_ai(planet_id);
            }
            StopPlanetAI(planet_id) => {
                self.routing().stop_planet_ai(planet_id);
            }
            ResetPlanetAI(planet_id) => {
                // morally a stop + start
                self.routing().stop_planet_ai(planet_id);
                self.routing().start_planet_ai(planet_id);
            }
            KillPlanet(planet_id) => {
                self.routing().kill_planet(planet_id);
            }

            // Explorer AI Control Commands
            StartExplorerAI(explorer_id) => {
                self.routing().start_explorer_ai(explorer_id);
            }
            StopExplorerAI(explorer_id) => {
                self.routing().stop_explorer_ai(explorer_id);
            }
            ResetExplorerAI(explorer_id) => {
                self.routing().reset_explorer(explorer_id);
            }
            KillExplorer(explorer_id) => {
                self.routing().kill_explorer(
                    explorer_id,
                    *self
                        .explorers_location
                        .lock()
                        .unwrap()
                        .get(&explorer_id)
                        .expect("Explorer not found in explorers_location when trying to kill it"),
                    true,
                );
            }
        }
    }

    //TODO: EXPLORERS_SENDERS AND PLANETS_SENDERS ARE NEEDED TO BE OWNED?
    fn process_messages(&mut self) {
        let convo_scheduler = self.convo_scheduler.clone();
        let explorer_senders = self.channels_manager.get_orch_to_exp_senders_struct();
        let planets_senders = self.channels_manager.get_to_planet_senders_struct();
        let explorer_locations = self.explorers_location.clone();
        let galaxy = self.galaxy.clone();
        let planet_threads = self.planet_threads.clone();
        let stop = self.message_processor_stop.clone();
        self.message_processor_thread = Some(thread::spawn(move || {
            loop {
                if stop.load(Ordering::Acquire) {
                    break;
                }

                if convo_scheduler.is_empty() {
                    // Wait for new messages to arrive
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }

                let current_convo = convo_scheduler.get_next_conversation();
                if let Some(convo) = current_convo {
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
                            convo_scheduler.add_conversation(Box::new(convo));
                        }
                        // Remove the planet from the galaxy and notify the planet thread to stop.
                        if let (Some(planet_id), _) = convo.get_entities_ids() {
                            let planets_senders_clone = planets_senders.clone();
                            let galaxy_clone = galaxy.clone();
                            let planet_threads_clone = planet_threads.clone();
                            // remove_node_with_stop will remove the node from the PlanetMap and then
                            // call the provided closure to kill the planet (send KillPlanet and remove sender).
                            planet::remove_node_with_stop(&galaxy_clone, planet_id, |dead_id| {
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
                    let id = convo.get_id();
                    let msg = convo_scheduler.get_waiting_message(id);
                    let should_transition = msg.is_some() || convo.get_expected_kind().is_none();
                    // Transition only if the waiting message is Some or if the expected kind is None
                    // Otherwise, add the conversation back in the convo_scheduler
                    if should_transition {
                        log_internal(
                            LogTarget::Conversations,
                            Channel::Trace,
                            payload!(
                                event: "Conversation Transition",
                                conversation_id: id,
                                old_expected_kind: format!("{:?}", convo.get_expected_kind()),
                            ),
                        );
                        if let Some(convo) = convo.transition(msg) {
                            convo_scheduler.add_conversation(convo);
                        }
                    } else {
                        convo_scheduler.add_conversation(convo);
                    }
                }
            }
        }));
    }

    fn shutdown(&mut self) {
        // Stop producers first so no new conversations/events are created while shutting down.
        self.message_processor_stop.store(true, Ordering::Release);
        background_events::shutdown_background_events();

        if let Some(handle) = self.message_processor_thread.take() {
            let _ = handle.join();
        }

        {
            let explorer_senders = self.channels_manager.get_orch_to_exp_senders_struct();
            for sender in explorer_senders.lock().unwrap().values() {
                let _ = sender
                    .send(common_game::protocols::orchestrator_explorer::OrchestratorToExplorer::KillExplorer);
            }
        }

        for (_, handle) in self.explorer_threads.drain() {
            let _ = handle.join();
        }

        {
            let planet_senders = self.channels_manager.get_to_planet_senders_struct();
            for sender in planet_senders.lock().unwrap().values() {
                let _ = sender.send(
                    common_game::protocols::orchestrator_planet::OrchestratorToPlanet::KillPlanet,
                );
            }
        }

        if let Ok(mut handles) = self.planet_threads.lock() {
            for (_, handle) in handles.drain() {
                let _ = handle.join();
            }
        }
    }

    /// Creates an Explorer and spawns its thread.
    ///
    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    ///
    pub fn add_explorer(&mut self, explorer_type: ExplorerType, into_planet: ID) {
        let id = get_id_manager().get_next_explorer_id_by_type(explorer_type);
        // Create channels
        let (_tx_orchestrator_to_explorer, rx_orchestrator_to_explorer) =
            self.channels_manager.create_orch_to_explorer_channel(id);
        let (_tx_planet_to_explorer, rx_planet_to_explorer) =
            self.channels_manager.create_planet_to_exp_channel(id);

        let mut explorer: Box<dyn ExplorerAI + Send> = match explorer_type {
            ExplorerType::Explorer => Box::new(explorer_nico::Explorer::new(
                id,
                self.channels_manager.get_from_explorers_sender(),
                rx_orchestrator_to_explorer,
                rx_planet_to_explorer,
                get_game_step(),
            )),
            ExplorerType::Vojager => Box::new(explorer_rob::Voyager::new(
                id,
                self.channels_manager.get_from_explorers_sender(),
                rx_orchestrator_to_explorer,
                rx_planet_to_explorer,
                get_game_step(),
            )),
            ExplorerType::Nomad => {
                let nomad = explorer_jacopo::Nomad::new(
                    id,
                    self.channels_manager.get_from_explorers_sender(),
                    rx_orchestrator_to_explorer,
                    rx_planet_to_explorer,
                    get_game_step(),
                );
                Box::new(nomad)
            }
        };

        // Spawn the Explorer
        let handle = thread::spawn(move || {
            let result = explorer.run();

            if let Err(e) = result {
                log_internal(
                    LogTarget::General,
                    Channel::Warning,
                    payload!(
                        action: "Explorer thread ended with an error",
                        explorer_id : id,
                        error : e
                    ),
                );
            } else {
                log_internal(
                    LogTarget::General,
                    Channel::Debug,
                    payload!(
                        action : "Explorer thread ended correctly",
                        explorer_id : id,
                    ),
                );
            }
        });

        self.explorers_location
            .lock()
            .unwrap()
            .insert(id, into_planet);

        // Add handle to the hashmap
        self.explorer_threads.insert(id, handle);

        // Move Manually the explorer to the planet
        self.routing()
            .make_manual_travel_to_planet_request(id, None, into_planet);

        log_internal(
            LogTarget::General,
            Channel::Info,
            payload!(
                action: "Created Explorer",
                explorer_id : id,
            ),
        );
    }
}
