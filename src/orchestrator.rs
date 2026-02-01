#![allow(dead_code)]

mod conversations;
pub mod convo_factory;
mod event_senders;
mod queue;

use crate::galaxy_setup::galaxy_loader;
use crate::orchestrator::conversations::{PossibleMessage, SendersToExplorer, SendersToPlanet};
use crate::orchestrator::queue::ConvoScheduler;
use crate::planet::{self, PlanetMap};
use crate::{get_id_manager, payload};

use crate::globals::{get_game_step, set_game_step};
use crate::logging_utils::{log_internal, log_msg_from};
use crate::orchestrator::conversations::ToExplorerStruct;
use crate::orchestrator::conversations::ToPlanetStruct;
use crate::orchestrator::conversations::orch_explorer::kill_explorer::{
    KillExplorerConversation, SendingKillExplorer,
};
use common_explorer::ExplorerAI;
pub(crate) use common_explorer::ExplorerBagContent;
use common_game::components::forge::Forge;
use common_game::logging::{ActorType, Channel, EventType};
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

use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};

pub type ExplorersLocationRef = Arc<Mutex<HashMap<ID, ID>>>;

#[derive(Clone, Copy)]
pub enum ExplorerType {
    Rob,
    Nico,
    Jaco,
}

#[derive(Clone)]
pub(crate) struct PlanetExplorerChannels {
    planet_to_explorer_senders: Arc<Mutex<HashMap<ID, Sender<PlanetToExplorer>>>>,
    explorer_to_planet_senders: Arc<Mutex<HashMap<ID, Sender<ExplorerToPlanet>>>>,
}

pub struct Orchestrator {
    pub(crate) ui_sender: Sender<OrchestratorToUiUpdate>,
    pub(crate) ui_receiver: Receiver<UiToOrchestratorCommand>,
    pub(crate) planets_senders: SendersToPlanet,
    pub(crate) explorer_senders: SendersToExplorer,
    planets_receiver: Receiver<PlanetToOrchestrator>,
    explorers_receiver: Receiver<ExplorerToOrchestrator<ExplorerBagContent>>,
    explorer_to_orchestrator_sender: Sender<ExplorerToOrchestrator<ExplorerBagContent>>,
    forge: Arc<Forge>,
    pub(crate) convo_scheduler: ConvoScheduler<ExplorerBagContent>,
    pub(crate) galaxy: PlanetMap,
    planet_explorer_channels: PlanetExplorerChannels,
    pub(crate) explorers_location: ExplorersLocationRef,
    planet_threads: std::sync::Arc<std::sync::Mutex<HashMap<ID, JoinHandle<()>>>>,
    explorer_threads: HashMap<ID, JoinHandle<()>>,
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

        let mut orchestrator = Self {
            ui_sender,
            ui_receiver,
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
            manual_mode: false,
        };

        // Check where to spawn Explorers
        let senders = orchestrator.planets_senders.lock().unwrap();
        let planet_id = if let Some(id) = spawn_planet {
            if senders.contains_key(&id) {
                id
            } else {
                let new_id = *senders
                    .keys()
                    .next()
                    .expect("Planet senders hashmap is empty");
                log_internal(
                    Channel::Warning,
                    payload!(
                        action : "Parameter spawn_planet was Some, but Planet was not found. Spawning Explorer(s) in a random Planet instead",
                        missing_planet_id : id,
                        random_planet_id_chosen : new_id
                    ),
                );
                new_id
            }
        } else {
            *senders
                .keys()
                .next()
                .expect("Planet senders hashmap is empty")
        };
        drop(senders);

        // Add first Explorer
        orchestrator.add_explorer(explorer1, planet_id);

        // If the second Explorer is some, add it too
        if let Some(explorer) = explorer2 {
            orchestrator.add_explorer(explorer, planet_id);
        }
        // Return the Orchestrator
        orchestrator
    }

    /// Runs the orchestrator, managing all planet and explorer conversations.
    ///
    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    pub fn run(&mut self) {
        // Send PlanetStart to all Planets
        for (id, _) in self.planets_senders.lock().unwrap().iter() {
            convo_factory::create_start_planet_conversation(
                &self.convo_scheduler,
                &self.planets_senders,
                *id,
            );
        }
        /*
        // Send ExplorerStart to all Explorers
        for (id, _) in self.explorer_senders.lock().unwrap().iter() {
            convo_factory::create_start_explorer_conversation(
                &self.convo_scheduler,
                &self.explorer_senders,
                *id,
            );
        }
        */
        // Start message processing thread
        self.process_messages();

        // Start background event senders
        self.start_background_event_senders();

        // Main loop
        loop {
            let timeout = crossbeam_channel::after(get_game_step() + Duration::from_millis(1000));
            select! {
                recv(self.planets_receiver) -> msg => {
                    match msg {
                        Ok(msg) => {
                            log_msg_from(
                                Channel::Trace,
                                EventType::MessagePlanetToOrchestrator,
                                (ActorType::Planet, msg.planet_id()),
                                payload!(
                                    msg : format!("{msg:?}"),
                                )
                            );
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
                            log_msg_from(
                                Channel::Trace,
                                EventType::MessageExplorerToOrchestrator,
                                (ActorType::Planet, msg.explorer_id()),
                                payload!(
                                    msg : format!("{msg:?}"),
                                )
                            );
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

                recv(&self.ui_receiver) -> msg => {
                    match msg {
                        Ok(msg) => {
                            self.handle_ui_message(msg);
                        }
                        Err(e) => {
                            log_internal(
                                Channel::Warning,
                                payload!(
                                    action : "Error while receiving from UI",
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

    /// for creating orchestrator conversations and controlling entities.
    pub fn ask_neighbors(&self, explorer_id: ID) {
        convo_factory::create_neighbors_request_conversation(
            &self.galaxy,
            &self.convo_scheduler,
            &self.explorer_senders,
            explorer_id,
        );
    }

    /// Create travel-to-planet conversation.
    pub fn make_travel_to_planet_request(
        &self,
        explorer_id: ID,
        current_planet_id: ID,
        dst_planet_id: ID,
    ) {
        convo_factory::create_travel_to_planet_request_conversation(
            &self.convo_scheduler,
            &self.planet_explorer_channels,
            &self.explorer_senders,
            &self.planets_senders,
            &self.explorers_location,
            explorer_id,
            current_planet_id,
            dst_planet_id,
        );
    }

    /// Create internal state conversation for a planet.
    pub fn ask_internal_state(&self, planet_id: ID) {
        convo_factory::create_internal_state_conversation(
            &self.convo_scheduler,
            &self.planets_senders,
            self.ui_sender.clone(),
            planet_id,
        );
    }

    /// Create bag content conversation for an explorer.
    pub fn ask_bag_content(&self, explorer_id: ID) {
        convo_factory::create_bag_content_conversation(
            &self.convo_scheduler,
            &self.explorer_senders,
            self.ui_sender.clone(),
            explorer_id,
        );
    }

    /// Create generate resource conversation.
    pub fn generate_resource(
        &self,
        explorer_id: ID,
        resource_type: common_game::components::resource::BasicResourceType,
    ) {
        convo_factory::create_generate_resource_conversation(
            &self.convo_scheduler,
            &self.explorer_senders,
            explorer_id,
            resource_type,
        );
    }

    /// Create combine resource conversation.
    pub fn combine_resource(
        &self,
        explorer_id: ID,
        resource_type: common_game::components::resource::ComplexResourceType,
    ) {
        convo_factory::create_combine_resource_conversation(
            &self.convo_scheduler,
            &self.explorer_senders,
            explorer_id,
            resource_type,
        );
    }

    /// Start explorer AI conversation.
    pub fn start_explorer_ai(&self, explorer_id: ID) {
        convo_factory::create_start_explorer_conversation(
            &self.convo_scheduler,
            &self.explorer_senders,
            explorer_id,
        );
    }

    /// Stop explorer AI conversation.
    pub fn stop_explorer_ai(&self, explorer_id: ID) {
        convo_factory::create_stop_explorer_conversation(
            &self.convo_scheduler,
            &self.explorer_senders,
            explorer_id,
        );
    }

    /// Kill explorer conversation.
    pub fn kill_explorer(&self, explorer_id: ID, planet_id: ID, handle_outgoing: bool) {
        convo_factory::create_kill_explorer_conversation(
            &self.convo_scheduler,
            &self.explorer_senders,
            &self.planets_senders,
            &self.explorers_location,
            explorer_id,
            planet_id,
            handle_outgoing,
        );
    }

    /// Reset explorer conversation.
    pub fn reset_explorer(&self, explorer_id: ID) {
        convo_factory::create_reset_explorer_conversation(
            &self.convo_scheduler,
            &self.explorer_senders,
            explorer_id,
        );
    }

    /// Start planet AI conversation.
    pub fn start_planet_ai(&self, planet_id: ID) {
        convo_factory::create_start_planet_conversation(
            &self.convo_scheduler,
            &self.planets_senders,
            planet_id,
        );
    }

    /// Stop planet AI conversation.
    pub fn stop_planet_ai(&self, planet_id: ID) {
        convo_factory::create_stop_planet_conversation(
            &self.convo_scheduler,
            &self.planets_senders,
            planet_id,
        );
    }

    pub fn kill_planet(&self, planet_id: ID) {
        convo_factory::create_kill_planet_conversation(
            &self.convo_scheduler,
            &self.planets_senders,
            &self.explorer_senders,
            &self.explorers_location,
            planet_id,
        );
    }

    /// Supported resources conversation.
    pub fn ask_supported_resources(&self, explorer_id: ID) {
        convo_factory::create_supported_resources_conversation(
            &self.convo_scheduler,
            &self.explorer_senders,
            self.ui_sender.clone(),
            explorer_id,
        );
    }

    pub fn ask_supported_combinations(&self, explorer_id: ID) {
        convo_factory::create_supported_combinations_conversation(
            &self.convo_scheduler,
            &self.explorer_senders,
            self.ui_sender.clone(),
            explorer_id,
        );
    }

    pub fn send_asteroid(&self, planet_id: ID) {
        convo_factory::create_asteroid_conversation(
            &self.convo_scheduler,
            &self.planets_senders,
            &self.ui_sender,
            &self.forge,
            &self.explorers_location,
            &self.explorer_senders,
            planet_id,
        );
    }

    pub fn send_sunray(&self, planet_id: ID) {
        convo_factory::create_sunray_conversation(
            &self.convo_scheduler,
            &self.planets_senders,
            &self.ui_sender,
            &self.forge,
            planet_id,
        );
    }

    /// Toggles between manual and automatic mode for the orchestrator.
    ///
    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    pub fn change_mode(&mut self) {
        self.manual_mode = !self.manual_mode;
        if self.manual_mode {
            log_internal(
                Channel::Info,
                payload!(
                    action : "Orchestrator switched to MANUAL mode",
                ),
            );
            for explorer_id in self.explorer_senders.lock().unwrap().keys() {
                self.stop_explorer_ai(*explorer_id);
            }
            event_senders::disable_asteroids();
        } else {
            log_internal(
                Channel::Info,
                payload!(
                    action : "Orchestrator switched to AUTOMATIC mode",
                ),
            );
            for explorer_id in self.explorer_senders.lock().unwrap().keys() {
                self.start_explorer_ai(*explorer_id);
            }
            event_senders::enable_asteroids();
        }
    }

    fn start_background_event_senders(&self) {
        event_senders::init_background_event_scheduler(
            self.planets_senders.clone(),
            self.forge.clone(),
            self.ui_sender.clone(),
            self.explorers_location.clone(),
            self.explorer_senders.clone(),
            self.convo_scheduler.clone(),
            self.galaxy.clone(),
        );

        event_senders::enable_sunrays();
        event_senders::enable_asteroids();
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
                    &self.explorer_senders,
                    *explorer_id,
                )),
                ExplorerToOrchestrator::TravelToPlanetRequest {
                    explorer_id,
                    current_planet_id,
                    dst_planet_id,
                } => Some(convo_factory::create_travel_to_planet_request_conversation(
                    &self.convo_scheduler,
                    &self.planet_explorer_channels,
                    &self.explorer_senders,
                    &self.planets_senders,
                    &self.explorers_location,
                    *explorer_id,
                    *current_planet_id,
                    *dst_planet_id,
                )),
                _ => {
                    log_internal(
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
    #[allow(clippy::too_many_lines)]
    pub fn handle_ui_message(&mut self, command: UiToOrchestratorCommand) {
        #[allow(clippy::enum_glob_use)]
        use UiToOrchestratorCommand::*;

        match command {
            // Rendering/Query Commands - Direct responses without conversations
            GetGalaxy => {
                let _ = self
                    .ui_sender
                    .send(OrchestratorToUiUpdate::Galaxy(self.galaxy.clone()));
            }
            GetExplorersPosition => {
                let _ = self
                    .ui_sender
                    .send(OrchestratorToUiUpdate::ExplorersPosition(
                        self.explorers_location.clone(),
                    ));
            }
            GetPlanetSnapshot(planet_id) => {
                self.ask_internal_state(planet_id); //the conversation will send the update to UI
            }

            GetExplorerSnapshot(explorer_id) => {
                self.ask_bag_content(explorer_id); //the conversation will send the update to UI
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
                    Channel::Info,
                    payload!(
                        action : "Received EndGame command from UI. Shutting down orchestrator",
                    ),
                );
                std::process::exit(0);
            }
            PauseGame => {
                crate::orchestrator::event_senders::disable_asteroids();
                crate::orchestrator::event_senders::disable_sunrays();
                log_internal(
                    Channel::Info,
                    payload!(
                        action : "Received PauseGame command from UI. Pausing background events",
                    ),
                );
            }
            ResumeGame => {
                crate::orchestrator::event_senders::enable_asteroids();
                crate::orchestrator::event_senders::enable_sunrays();
                log_internal(
                    Channel::Info,
                    payload!(
                        action : "Received ResumeGame command from UI. Resuming background events",
                    ),
                );
            }

            // Explorer Movement Commands
            ManualMoveExplorer(explorer_id, current_planet, dst_planet) => {
                self.make_travel_to_planet_request(explorer_id, current_planet, dst_planet);
            }

            // Explorer Resource Commands
            ExplorerGenerateResource(explorer_id, resource_type) => {
                self.generate_resource(explorer_id, resource_type);
            }
            ExplorerCombineResource(explorer_id, resource_type) => {
                self.combine_resource(explorer_id, resource_type);
            }

            SupportedCombinations(explorer_id) => {
                //it automatically sends the update to UI
                self.ask_supported_combinations(explorer_id);
            }

            SupportedResources(explorer_id) => {
                //it automatically sends the update to UI
                self.ask_supported_resources(explorer_id);
            }

            // Asteroid/Sunray Commands
            SendManualAsteroid(planet_id) => {
                self.send_asteroid(planet_id);
            }

            SendManualSunray(planet_id) => {
                self.send_sunray(planet_id);
            }

            // Planet AI Control Commands
            StartPlanetAI(planet_id) => {
                self.start_planet_ai(planet_id);
            }
            StopPlanetAI(planet_id) => {
                self.stop_planet_ai(planet_id);
            }
            ResetPlanetAI(planet_id) => {
                // morally a stop + start
                self.stop_planet_ai(planet_id);
                self.start_planet_ai(planet_id);
            }
            KillPlanet(planet_id) => {
                self.kill_planet(planet_id);
            }

            // Explorer AI Control Commands
            StartExplorerAI(explorer_id) => {
                self.start_explorer_ai(explorer_id);
            }
            StopExplorerAI(explorer_id) => {
                self.stop_explorer_ai(explorer_id);
            }
            ResetExplorerAI(explorer_id) => {
                self.reset_explorer(explorer_id);
            }
            KillExplorer(explorer_id) => {
                self.kill_explorer(
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

    fn process_messages(&mut self) {
        let convo_scheduler = self.convo_scheduler.clone();
        let explorer_senders = self.explorer_senders.clone();
        let planets_senders = self.planets_senders.clone();
        let explorer_locations = self.explorers_location.clone();
        let galaxy = self.galaxy.clone();
        let planet_threads = self.planet_threads.clone();
        thread::spawn(move || {
            loop {
                if convo_scheduler.is_empty() {
                    // Wait for new messages to arrive
                    thread::sleep(std::time::Duration::from_millis(10));
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
                            crate::planet::remove_node_with_stop(
                                &galaxy_clone,
                                planet_id,
                                |dead_id| {
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
                                },
                            );
                        }
                    }
                    let id = convo.get_id();
                    let msg = convo_scheduler.get_waiting_message(id);
                    let should_transition = msg.is_some() || convo.get_expected_kind().is_none();
                    // Transition only if the waiting message is Some or if the expected kind is None
                    // Otherwise, add the conversation back in the convo_scheduler
                    if should_transition {
                        log_internal(
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
        });
    }
    /// Creates an Explorer and spawns its thread.
    ///
    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    ///
    pub fn add_explorer(&mut self, explorer_type: ExplorerType, into_planet: ID) {
        // Create channels
        let (tx_orchestrator_to_explorer, rx_orchestrator_to_explorer) =
            unbounded::<OrchestratorToExplorer>();
        let (tx_planet_to_explorer, rx_planet_to_explorer) = unbounded::<PlanetToExplorer>();
        let id = get_id_manager().get_next_explorer_id_by_type(explorer_type);

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

        let mut explorer: Box<dyn ExplorerAI + Send> = match explorer_type {
            ExplorerType::Nico => Box::new(explorer_nico::Explorer::new(
                id,
                self.explorer_to_orchestrator_sender.clone(),
                rx_orchestrator_to_explorer,
                rx_planet_to_explorer,
                get_game_step(),
            )),
            ExplorerType::Rob => {
                todo!()
            }
            ExplorerType::Jaco => {
                let nomad = explorer_jacopo::Nomad::new(
                    id,
                    self.explorer_to_orchestrator_sender.clone(),
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

        self.explorers_location
            .lock()
            .unwrap()
            .insert(id, into_planet);

        // Add handle to the hashmap
        self.explorer_threads.insert(id, handle);

        // Tell the Planet that an Explorer is coming
        // TODO! now I am using the same ToPlanetStruct for current planet and destination planet
        // because explorer does not have a current planet, but I don't know if it is the correct way to handle this

        convo_factory::create_travel_to_planet_request_conversation(
            &self.convo_scheduler,
            &self.planet_explorer_channels,
            &self.explorer_senders,
            &self.planets_senders,
            &self.explorers_location,
            id,
            into_planet,
            into_planet,
        );

        log_internal(
            Channel::Info,
            payload!(
                action: "Created Explorer",
                explorer_id : id,
            ),
        );
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
