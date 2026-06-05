#![allow(dead_code)]

mod background_events;
pub(crate) mod conversations;
use crate::id::PlanetKind;
mod ui_handler;
use crate::galaxy_setup::galaxy_loader;
use crate::orchestrator::conversations::PossibleMessage;
use crate::planet::{self, PlanetMap};
use crate::{explorer_factory, get_id_manager, payload};

use crate::channels_manager::ChannelsManager;
use crate::convo_manager::ConvoManager;
use crate::explorer_factory::ExplorerType;
use crate::galaxy_setup::spawn_planet_with_channels;
use crate::globals::{get_game_step, set_game_step};
use crate::logging::{LogTarget, log_internal, log_msg_from};
use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
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

pub(crate) struct OrchContext {
    //Read only
    forge: Arc<Forge>,
    //Modifiable
    channels_manager: ChannelsManagerRef,
    galaxy: PlanetMap,
    explorers_location: ExplorersLocationRef,
}

impl OrchContext {
    pub(crate) fn new(
        channels_manager: ChannelsManagerRef,
        forge: Arc<Forge>,
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
    pub(crate) fn get_forge(&self) -> Arc<Forge> {
        self.forge.clone()
    }
    pub(crate) fn get_galaxy(&self) -> PlanetMap {
        self.galaxy.clone()
    }

    pub(crate) fn get_explorers_location(&self) -> ExplorersLocationRef {
        self.explorers_location.clone()
    }

    pub(crate) fn insert_explorer_location(&self, explorer_id : ID, planet_id : ID) {
        self.explorers_location.insert(explorer_id, planet_id);
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
    background_events_enabled: bool,
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        file_path: &std::path::Path,
        game_step: u64,
        ui_sender: Sender<OrchestratorToUiUpdate>,
        ui_receiver: Receiver<UiToOrchestratorCommand>,
        explorer1: ExplorerType,
        explorer2: Option<ExplorerType>,
        spawn_planet: Option<ID>,
        background_events_enabled: bool,
    ) -> Self {
        // Set static variable GAME_STEP
        set_game_step(game_step);

        let channels_manager_ref = Arc::new(ChannelsManager::new(ui_sender, ui_receiver));
        // galaxy_loader now returns 2 values (galaxy and planet_threads), all channels are distributed inside using
        // channels manager APIs
        let (galaxy, planet_threads) = galaxy_loader(file_path, &channels_manager_ref);
        let explorers_location = DashMap::new();
        let forge = Arc::new(Forge::new().expect("Couldn't create forge!"));

        let orch_context = Arc::new(OrchContext::new(
            channels_manager_ref.clone(),
            forge,
            galaxy.clone(),
            explorers_location,
        ));
        let convo_manager = ConvoManager::new(orch_context.clone());

        // Sync planet ID generator with pre-loaded galaxy IDs to avoid runtime collisions
        // (e.g., first AddPlanet duplicating an existing ID from file).
        {
            let existing_planet_ids: Vec<ID> = galaxy
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .keys()
                .copied()
                .collect();
            get_id_manager().sync_planet_counter_with_existing(&existing_planet_ids);
        }

        // Sync planet ID generator with pre-loaded galaxy IDs to avoid runtime collisions
        // (e.g., first AddPlanet duplicating an existing ID from file).
        {
            let existing_planet_ids: Vec<ID> = galaxy
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .keys()
                .copied()
                .collect();
            get_id_manager().sync_planet_counter_with_existing(&existing_planet_ids);
        }

        let mut orchestrator = Self {
            convo_manager: Arc::new(convo_manager),
            orch_context_ref: orch_context,
            planet_threads: Arc::new(Mutex::new(planet_threads)), // threads were spawned in galaxy_loader/create_planet_with_channels
            explorer_threads: HashMap::new(),
            message_processor_thread: None,
            message_processor_stop: Arc::new(AtomicBool::new(false)),
            shutdown_requested: false,
            background_events_enabled,
            manual_mode: false,
        };

        // Add first explorers
        explorer_factory::spawn_first_explorers(
            &orchestrator.orch_context_ref,
            &orchestrator.convo_manager,
            &mut orchestrator.explorer_threads,
            explorer1,
            explorer2,
            spawn_planet,
        );

        // Return the Orchestrator
        orchestrator
    }

    #[must_use]
    pub fn get_galaxy(&self) -> &PlanetMap {
        &self.orch_context_ref.galaxy
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
        if self.background_events_enabled {
            self.start_background_event_senders();
        } else {
            background_events::disable_asteroids();
            background_events::disable_sunrays();
        }

        //Get receiving channels
        let mut from_planets_rcv = self.orch_context_ref.channels_manager.get_from_planet_rcv();
        let mut from_explorers_rcv = self
            .orch_context_ref
            .channels_manager
            .get_from_explorers_rcv();
        let mut from_ui_rcv = self.orch_context_ref.channels_manager.get_ui_receiver();
        // Main loop
        loop {
            let timeout = crossbeam_channel::after(get_game_step() + Duration::from_secs(1));
            select! {
                recv(from_planets_rcv) -> msg => {
                    if msg.is_err() {
                        from_planets_rcv = crossbeam_channel::never();
                    }
                    self.handle_planets_message(msg);
                }
                recv(from_explorers_rcv) -> msg => {
                    if msg.is_err() {
                        from_explorers_rcv = crossbeam_channel::never();
                    }
                    self.handle_explorers_message(msg);
                }

                recv(from_ui_rcv) -> msg => {
                    if msg.is_err() {
                        from_ui_rcv = crossbeam_channel::never();
                    }
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
            background_events::disable_sunrays();
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
            background_events::enable_sunrays();
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

    //TODO: EXPLORERS_SENDERS AND PLANETS_SENDERS ARE NEEDED TO BE OWNED?
    fn process_messages(&mut self) {
        let convo_manager = self.convo_manager.clone();
        let orch_context_ref = self.orch_context_ref.clone();
        let ui_sender = self
            .orch_context_ref
            .channels_manager
            .get_ui_sender()
            .clone();
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
                            explorer_factory::kill_explorer(
                                &orch_context_ref,
                                &convo_manager,
                                el.0,
                                Some(el.1),
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
                                    let _ = sender.send(OrchestratorToPlanet::KillPlanet);
                                }

                                // remove and join the planet thread handle if present
                                if let Ok(mut th_lock) = planet_threads_clone.lock()
                                    && let Some(handle) = th_lock.remove(&dead_id)
                                {
                                    let _ = handle.join();
                                }

                                let _ = ui_sender.send(OrchestratorToUiUpdate::DeadPlanet(dead_id));
                                let _ = ui_sender
                                    .send(OrchestratorToUiUpdate::Galaxy(galaxy_clone.clone()));

                                convo_manager.remove_convos_for_dead_entity(dead_id);
                                orch_context_ref.channels_manager.remove_planet_channels(dead_id);
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

    fn add_planet(&self, planet_kind: PlanetKind, connected_planets: Vec<u32>) {
        let planet_id = match planet_kind {
            PlanetKind::Trip => get_id_manager().get_next_trip_id(),
            PlanetKind::Rustrelli => get_id_manager().get_next_rustrelli_id(),
            PlanetKind::Luna4 => get_id_manager().get_next_luna4_id(),
            PlanetKind::RustyCrab => get_id_manager().get_next_rusty_crab_id(),
            PlanetKind::Enterprise => get_id_manager().get_next_enterprise_id(),
            PlanetKind::Orbitron => get_id_manager().get_next_orbitron_id(),
            PlanetKind::Houston => get_id_manager().get_next_houston_id(),
        };

        planet::add_planet_with_neighbors(
            &self.orch_context_ref.galaxy,
            planet_id,
            connected_planets,
        );
        let already_present = self
            .orch_context_ref
            .channels_manager
            .to_planet_senders_contains(planet_id);

        if already_present {
            log_internal(
                LogTarget::General,
                Channel::Warning,
                payload!(
                    action : "AddPlanet called for an existing planet: topology updated, skipping start",
                    planet_id : planet_id,
                ),
            );
        } else {
            self.planet_threads
                .lock()
                .unwrap()
                .entry(planet_id)
                .or_insert_with(|| {
                    spawn_planet_with_channels(
                        self.orch_context_ref.channels_manager.as_ref(),
                        planet_id,
                    )
                });

            // Send PlanetStart to initialize the newly spawned planet
            self.convo_manager
                .create_start_planet_conversation(planet_id);

            log_internal(
                LogTarget::General,
                Channel::Info,
                payload!(
                    action : "Added new planet",
                    planet_id : planet_id,
                    planet_kind : format!("{:?}", planet_kind),
                ),
            );
        }
    }
}
