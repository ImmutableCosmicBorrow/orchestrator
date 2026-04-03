#![allow(dead_code)]

mod background_events;
pub(crate) mod conversations;

use crate::galaxy_setup::galaxy_loader;
use crate::orchestrator::conversations::PossibleMessage;
use crate::planet::{self, PlanetMap};
use crate::{get_id_manager, payload};

use crate::channels_manager::ChannelsManager;
use crate::convo_manager::ConvoManager;
use crate::globals::{get_game_step, set_game_step};
use crate::logging::{LogTarget, log_internal, log_msg_from};
use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
use common_explorer::ExplorerAI;
pub(crate) use common_explorer::ExplorerBagContent;
use common_game::components::forge::Forge;
use common_game::logging::{ActorType, Channel, EventType};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::utils::ID;
use crossbeam_channel::{Receiver, Sender, select};
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

//DashMap is already thread safe and handles multiple write and read accesses
pub(crate) type ExplorersLocationRef = DashMap<ID, ID>;
pub(crate) type ChannelsManagerRef = Arc<ChannelsManager>;
pub(crate) type OrchContextRef = Arc<OrchContext>; //Pass the Arc and inside modifiable fields will have the Arc<RWLock>

#[derive(Clone, Copy, Debug)]
pub enum ExplorerType {
    Vojager,  //Roberto
    Explorer, //Nicola
    Nomad,    //Jacopo
}

pub(crate) struct OrchContext {
    //Read only
    forge: Forge,
    //Modifiable
    channels_manager: ChannelsManagerRef,
    galaxy: PlanetMap,
    explorers_location: ExplorersLocationRef,
}

impl OrchContext {
    pub(crate) fn new(
        channels_manager: ChannelsManagerRef,
        forge: Forge,
        galaxy: PlanetMap,
        explorers_location: ExplorersLocationRef,
    ) -> Self {
        Self {
            forge,
            channels_manager,
            galaxy,
            explorers_location,
        }
    }

    pub(crate) fn get_channels_manager(&self) -> ChannelsManagerRef {
        self.channels_manager.clone()
    }
    pub(crate) fn get_forge(&self) -> &Forge {
        &self.forge
    }
    pub(crate) fn get_galaxy(&self) -> PlanetMap {
        self.galaxy.clone()
    }

    pub(crate) fn get_explorers_location(&self) -> ExplorersLocationRef {
        self.explorers_location.clone()
    }
}

pub struct Orchestrator {
    //TODO: MIGHT DELETE THIS
    orch_context_ref: OrchContextRef,
    convo_manager: Arc<ConvoManager>,
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

        let channels_manager_ref = Arc::new(ChannelsManager::new(ui_sender, ui_receiver));
        // galaxy_loader now returns 2 values (galaxy and planet_threads), all channels are distributed inside using
        // channels manager APIs
        let (galaxy, planet_threads) = galaxy_loader(file_path, &channels_manager_ref);
        let explorers_location = DashMap::new();
        let forge = Forge::new().expect("Couldn't create forge!");

        let orch_context = Arc::new(OrchContext::new(
            channels_manager_ref.clone(),
            forge,
            galaxy.clone(),
            explorers_location,
        ));
        let convo_manager = ConvoManager::new(orch_context.clone());

        let mut orchestrator = Self {
            convo_manager: Arc::new(convo_manager),
            orch_context_ref: orch_context,
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
            .filter(|id| self.orch_context_ref.channels_manager.to_planet_senders_contains(*id))
            .unwrap_or_else(|| {
                let fallback_id = self.orch_context_ref.channels_manager.to_planet_senders_next_id().expect("Planet senders hashmap is empty");
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
        let planet_ids = self.orch_context_ref.channels_manager.get_planet_list();

        for id in planet_ids {
            self.convo_manager.create_start_planet_conversation(id);
        }

        // Send ExplorerStart to all Explorers
        let explorer_ids = self.orch_context_ref.channels_manager.get_explorer_list();

        for id in explorer_ids {
            self.convo_manager.create_start_explorer_conversation(id);
        }

        // Start message processing thread
        self.process_messages();

        // Start background event senders
        self.start_background_event_senders();

        //Get receiving channels
        let from_planets_rcv = self.orch_context_ref.channels_manager.get_from_planet_rcv();
        let from_explorers_rcv = self
            .orch_context_ref
            .channels_manager
            .get_from_explorers_rcv();
        let from_ui_rcv = self.orch_context_ref.channels_manager.get_ui_receiver();
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
                    if self.orch_context_ref.explorers_location.is_empty() {
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
                self.convo_manager
                    .handle_message(PossibleMessage::PlanetToOrch(msg));
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
                self.convo_manager
                    .handle_message(PossibleMessage::ExplorerToOrch(msg));
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
        &self.orch_context_ref.galaxy
    }

    /// Toggles between manual and automatic mode for the orchestrator.
    ///
    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    pub fn change_mode(&mut self) {
        self.manual_mode = !self.manual_mode;
        let explorers = self.orch_context_ref.channels_manager.get_explorer_list();
        if self.manual_mode {
            log_internal(
                LogTarget::General,
                Channel::Info,
                payload!(
                    action : "Orchestrator switched to MANUAL mode",
                ),
            );

            //Stop all explorers
            for explorer_id in explorers {
                self.convo_manager
                    .create_stop_explorer_conversation(explorer_id);
            }
            //stop events
            background_events::disable_asteroids();
        } else {
            log_internal(
                LogTarget::General,
                Channel::Info,
                payload!(
                    action : "Orchestrator switched to AUTOMATIC mode",
                ),
            );
            //Start all Explorers
            for explorer_id in explorers {
                self.convo_manager
                    .create_start_explorer_conversation(explorer_id);
            }
            //Start Events
            background_events::enable_asteroids();
        }
    }

    fn start_background_event_senders(&self) {
        background_events::init_background_event_scheduler(self.convo_manager.clone());

        background_events::enable_sunrays();
        background_events::enable_asteroids();
    }

    fn stop_background_event_senders() {
        background_events::disable_sunrays();
        background_events::disable_asteroids();
    }

    /// Handles UI commands from the UI layer and creates appropriate conversations or performs direct actions.
    ///
    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    // TODO: remove the clippy allow once the function is refactored into smaller functions
    // TODO: Move this in convo_manager?
    #[allow(clippy::too_many_lines)]
    pub fn handle_ui_message(&mut self, command: UiToOrchestratorCommand) {
        #[allow(clippy::enum_glob_use)]
        use UiToOrchestratorCommand::*;

        match command {
            // Rendering/Query Commands - Direct responses without conversations
            GetGalaxy => {
                let _ = self
                    .orch_context_ref
                    .channels_manager
                    .get_ui_sender_ref()
                    .send(OrchestratorToUiUpdate::Galaxy(
                        self.orch_context_ref.galaxy.clone(),
                    ));
            }
            GetExplorersPosition => {
                let _ = self
                    .orch_context_ref
                    .channels_manager
                    .get_ui_sender_ref()
                    .send(OrchestratorToUiUpdate::ExplorersPosition(
                        self.orch_context_ref.explorers_location.clone(),
                    ));
            }
            GetPlanetSnapshot(planet_id) => {
                self.convo_manager
                    .create_internal_state_conversation(planet_id); //the conversation will send the update to UI
            }

            GetExplorerSnapshot(explorer_id) => {
                self.convo_manager
                    .create_bag_content_conversation(explorer_id); //the conversation will send the update to UI
            }

            AddPlanet(planet_id, connected_planets) => {
                planet::add_planet_with_neighbors(
                    &self.orch_context_ref.galaxy,
                    planet_id,
                    connected_planets,
                );
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
                Orchestrator::stop_background_event_senders();

                for explorer_id in self.orch_context_ref.channels_manager.get_explorer_list() {
                    self.convo_manager
                        .create_stop_explorer_conversation(explorer_id);
                }

                for planet_id in self.orch_context_ref.channels_manager.get_planet_list() {
                    self.convo_manager
                        .create_stop_planet_conversation(planet_id);
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

                for explorer_id in self.orch_context_ref.channels_manager.get_explorer_list() {
                    self.convo_manager
                        .create_start_explorer_conversation(explorer_id);
                }

                for planet_id in self.orch_context_ref.channels_manager.get_planet_list() {
                    self.convo_manager
                        .create_start_planet_conversation(planet_id);
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
                self.convo_manager.create_send_manual_move_conversation(
                    explorer_id,
                    current_planet,
                    dst_planet,
                );
            }

            // Explorer Resource Commands
            ExplorerGenerateResource(explorer_id, resource_type) => {
                self.convo_manager
                    .create_generate_resource_conversation(explorer_id, resource_type);
            }
            ExplorerCombineResource(explorer_id, resource_type) => {
                self.convo_manager
                    .create_combine_resource_conversation(explorer_id, resource_type);
            }

            SupportedCombinations(explorer_id) => {
                //it automatically sends the update to UI
                self.convo_manager
                    .create_supported_combinations_conversation(explorer_id);
            }

            SupportedResources(explorer_id) => {
                //it automatically sends the update to UI
                self.convo_manager
                    .create_supported_resources_conversation(explorer_id);
            }

            // Asteroid/Sunray Commands
            SendManualAsteroid(planet_id) => {
                self.convo_manager.create_asteroid_conversation(planet_id);
            }

            SendManualSunray(planet_id) => {
                self.convo_manager.create_sunray_conversation(planet_id);
            }

            // Planet AI Control Commands
            StartPlanetAI(planet_id) => {
                self.convo_manager
                    .create_start_planet_conversation(planet_id);
            }
            StopPlanetAI(planet_id) => {
                self.convo_manager
                    .create_stop_planet_conversation(planet_id);
            }
            ResetPlanetAI(planet_id) => {
                // morally a stop + start
                self.convo_manager
                    .create_stop_planet_conversation(planet_id);
                self.convo_manager
                    .create_start_planet_conversation(planet_id);
            }
            KillPlanet(planet_id) => {
                self.convo_manager
                    .create_kill_planet_conversation(planet_id);
            }

            // Explorer AI Control Commands
            StartExplorerAI(explorer_id) => {
                self.convo_manager
                    .create_start_explorer_conversation(explorer_id);
            }
            StopExplorerAI(explorer_id) => {
                self.convo_manager
                    .create_stop_explorer_conversation(explorer_id);
            }
            ResetExplorerAI(explorer_id) => {
                self.convo_manager
                    .create_reset_explorer_conversation(explorer_id);
            }
            KillExplorer(explorer_id) => {
                self.convo_manager.create_kill_explorer_conversation(
                    explorer_id,
                    *self
                        .orch_context_ref
                        .explorers_location
                        .get(&explorer_id)
                        .expect("Explorer not found in explorers_location when trying to kill it"),
                    true,
                );
            }
        }
    }

    //TODO: EXPLORERS_SENDERS AND PLANETS_SENDERS ARE NEEDED TO BE OWNED?
    fn process_messages(&mut self) {
        let convo_manager = self.convo_manager.clone();
        let orch_context_ref = self.orch_context_ref.clone();
        let planet_threads = self.planet_threads.clone();
        let stop = self.message_processor_stop.clone();

        self.message_processor_thread = Some(thread::spawn(move || {
            loop {
                if stop.load(Ordering::Acquire) {
                    break;
                }

                if convo_manager.convo_scheduler.is_empty() {
                    // Wait for new messages to arrive
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }

                let current_convo = convo_manager.convo_scheduler.get_next_conversation();
                if let Some(convo) = current_convo {
                    let kill_expl_vec = convo.get_kill_explorers_vec();
                    if let Some((vec, handle_outgoing)) = kill_expl_vec {
                        for el in vec {
                            convo_manager.create_kill_explorer_conversation(
                                el.0,
                                el.1,
                                handle_outgoing,
                            );
                        }

                        //TODO: ASK to the others, planet is already killed by the convos
                        //TODO: MAYBE ADD THIS TO THE CONVO
                        // Remove the planet from the galaxy and notify the planet thread to stop.
                        if let (Some(planet_id), _) = convo.get_entities_ids() {
                            let planets_senders_clone = orch_context_ref
                                .channels_manager
                                .get_to_planet_senders_struct()
                                .clone();
                            let galaxy_clone = orch_context_ref.galaxy.clone();
                            let planet_threads_clone = planet_threads.clone();
                            // remove_node_with_stop will remove the node from the PlanetMap and then
                            // call the provided closure to kill the planet (send KillPlanet and remove sender).
                            planet::remove_node_with_stop(&galaxy_clone, planet_id, |dead_id| {
                                // remove and notify sender
                                if let Some((_, sender)) = planets_senders_clone.remove(&dead_id) {
                                    let _ = sender.send(
                                        OrchestratorToPlanet::KillPlanet,
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
                    let msg = convo_manager.convo_scheduler.get_waiting_message(id);
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
                            convo_manager.convo_scheduler.add_conversation(convo);
                        }
                    } else {
                        convo_manager.convo_scheduler.add_conversation(convo);
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

        //Kill all explorers, first store senders in vec to prevent holding lock in dashmap
        let explorer_senders: Vec<Sender<OrchestratorToExplorer>> = self
            .orch_context_ref
            .channels_manager
            .get_orch_to_exp_senders_struct() // This returns the DashMap
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        for sender in explorer_senders {
            let _ = sender.send(OrchestratorToExplorer::KillExplorer);
        }

        for (_, handle) in self.explorer_threads.drain() {
            let _ = handle.join();
        }

        // Retrieve senders to all planets and kill them
        let explorer_senders: Vec<Sender<OrchestratorToPlanet>> = self
            .orch_context_ref
            .channels_manager
            .get_to_planet_senders_struct() // This returns the DashMap
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        for sender in explorer_senders {
            let _ = sender.send(OrchestratorToPlanet::KillPlanet);
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
        let exp_sender = self
            .orch_context_ref
            .channels_manager
            .get_from_explorers_sender();
        // Create channels
        let (_tx_orchestrator_to_explorer, rx_orchestrator_to_explorer) = self
            .orch_context_ref
            .channels_manager
            .create_orch_to_explorer_channel(id);
        let (_tx_planet_to_explorer, rx_planet_to_explorer) = self
            .orch_context_ref
            .channels_manager
            .create_planet_to_exp_channel(id);

        let mut explorer: Box<dyn ExplorerAI + Send> = match explorer_type {
            ExplorerType::Explorer => Box::new(explorer_nico::Explorer::new(
                id,
                exp_sender,
                rx_orchestrator_to_explorer,
                rx_planet_to_explorer,
                get_game_step(),
            )),
            ExplorerType::Vojager => Box::new(explorer_rob::Voyager::new(
                id,
                exp_sender,
                rx_orchestrator_to_explorer,
                rx_planet_to_explorer,
                get_game_step(),
            )),
            ExplorerType::Nomad => {
                let nomad = explorer_jacopo::Nomad::new(
                    id,
                    exp_sender,
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

        self.orch_context_ref
            .explorers_location
            .insert(id, into_planet);

        // Add handle to the hashmap
        self.explorer_threads.insert(id, handle);

        // Move Manually the explorer to the planet
        self.convo_manager
            .create_send_manual_move_conversation(id, None, into_planet);

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
