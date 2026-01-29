#![allow(dead_code)]

mod conversations;
mod convo_factory;
mod event_senders;
mod queue;

use crate::galaxy_setup::galaxy_loader;
use crate::orchestrator::conversations::{PossibleMessage, SendersToExplorer, SendersToPlanet};
use crate::orchestrator::queue::ConvoScheduler;
use crate::planet::{self, PlanetMap};
use crate::{get_id_manager, payload};

use crate::globals::{get_game_step, set_game_step};
use crate::logging_utils::log_internal;
use crate::orchestrator::conversations::ToExplorerStruct;
use crate::orchestrator::conversations::ToPlanetStruct;
use crate::orchestrator::conversations::orch_explorer::kill_explorer::{
    KillExplorerConversation, SendingKillExplorer,
};
use crate::orchestrator::conversations::orch_explorer::start_explorer::SendingExplorerStart;
use crate::orchestrator::conversations::orch_explorer::start_explorer::StartExplorerConversation;
use crate::orchestrator::conversations::orch_explorer::stop_explorer::{
    SendingExplorerStop, StopExplorerConversation,
};
use common_explorer::ExplorerAI;
pub(crate) use common_explorer::ExplorerBagContent;
use common_game::components::forge::Forge;
use common_game::logging::Channel;
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
    pub(crate) ui_sender: Sender<OrchestratorToUiUpdate>,
    pub(crate) ui_receiver: Receiver<UiToOrchestratorCommand>,
    pub(crate) planets_senders: SendersToPlanet,
    pub(crate) explorer_senders: SendersToExplorer,
    planets_receiver: Receiver<PlanetToOrchestrator>,
    explorers_receiver: Receiver<ExplorerToOrchestrator<ExplorerBag>>,
    explorer_to_orchestrator_sender: Sender<ExplorerToOrchestrator<ExplorerBagContent>>,
    forge: Arc<Forge>,
    pub(crate) convo_scheduler: ConvoScheduler<ExplorerBag>,
    pub(crate) galaxy: PlanetMap,
    planet_explorer_channels: PlanetExplorerChannels,
    pub(crate) explorers_location: ExplorersLocationRef,
    planet_threads: std::sync::Arc<std::sync::Mutex<HashMap<ID, JoinHandle<()>>>>,
    explorer_threads: HashMap<ID, JoinHandle<()>>,
    manual_mode: bool,
}

impl Orchestrator {
    /// Creates a new Orchestrator instance from a galaxy configuration file.
    ///
    /// # Panics
    ///
    /// Panics if the forge cannot be created.
    #[must_use]
    pub fn new(
        file_path: &std::path::Path,
        game_step: u64,
        ui_sender: Sender<OrchestratorToUiUpdate>,
        ui_receiver: Receiver<UiToOrchestratorCommand>,
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

        Self {
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
            convo_factory::create_start_planet_conversation(
                &self.convo_scheduler,
                &self.planets_senders,
                *id,
            );
        }

        // Send ExplorerStart to all Explorers
        for (id, _) in self.explorer_senders.lock().unwrap().iter() {
            convo_factory::create_start_explorer_conversation(
                &self.convo_scheduler,
                &self.explorer_senders,
                *id,
            );
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
                let to_explorer =
                    ToExplorerStruct::new(self.explorer_senders.clone(), *explorer_id);
                let state = SendingExplorerStop::new(to_explorer);
                let stop_ai_convo = StopExplorerConversation::new(
                    get_id_manager().get_next_conversation_id(),
                    state,
                );
                self.convo_scheduler
                    .add_conversation(Box::new(stop_ai_convo));
            }
        } else {
            log_internal(
                Channel::Info,
                payload!(
                    action : "Orchestrator switched to AUTOMATIC mode",
                ),
            );
            for explorer_id in self.explorer_senders.lock().unwrap().keys() {
                let to_explorer =
                    ToExplorerStruct::new(self.explorer_senders.clone(), *explorer_id);
                let state = SendingExplorerStart::new(to_explorer);
                let start_ai_convo = StartExplorerConversation::new(
                    get_id_manager().get_next_conversation_id(),
                    state,
                );
                self.convo_scheduler
                    .add_conversation(Box::new(start_ai_convo));
            }
        }
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

        let convo_id = self
            .convo_scheduler
            .find_matching_conversation(&message_kind, entities_ids)
            .map(|convo| convo.get_id())
            .or_else(|| self.try_create_conversation(&message, &message_kind, entities_ids));

        if let Some(id) = convo_id {
            log_internal(
                Channel::Trace,
                payload!(
                    event: "MessageMatchedConversation",
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
        message: &PossibleMessage<ExplorerBag>,
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
                    &self.galaxy,
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
                        Channel::Debug,
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
                    Channel::Debug,
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
                convo_factory::create_internal_state_conversation(
                    &self.convo_scheduler,
                    &self.planets_senders,
                    self.ui_sender.clone(),
                    planet_id,
                ); //the conversation will send the update to UI
            }

            GetExplorerSnapshot(explorer_id) => {
                convo_factory::create_bag_content_conversation(
                    &self.convo_scheduler,
                    &self.explorer_senders,
                    self.ui_sender.clone(),
                    explorer_id,
                );
            }
            AddPlanet(planet_id, connected_planets) => {
                planet::add_planet_with_neighbors(&self.galaxy, planet_id, connected_planets);
            }

            // Explorer Movement Commands
            ManualMoveExplorer(explorer_id, current_planet, dst_planet) => {
                convo_factory::create_travel_to_planet_request_conversation(
                    &self.convo_scheduler,
                    &self.galaxy,
                    &self.planet_explorer_channels,
                    &self.explorer_senders,
                    &self.planets_senders,
                    &self.explorers_location,
                    explorer_id,
                    current_planet,
                    dst_planet,
                );
            }

            // Explorer Resource Commands
            ManualExplorerCraftsRes(explorer_id, resource) => {
                convo_factory::create_craft_resource_conversation(
                    &self.convo_scheduler,
                    &self.explorer_senders,
                    explorer_id,
                    resource,
                );
            }
            ManualExplorerCombineRes(explorer_id, resource) => {
                convo_factory::create_combine_resource_conversation(
                    &self.convo_scheduler,
                    &self.explorer_senders,
                    explorer_id,
                    resource,
                );
            }

            SupportedCombinations(explorer_id) => {
                //it automatically sends the update to UI
                convo_factory::create_supported_combinations_conversation(
                    &self.convo_scheduler,
                    &self.explorer_senders,
                    self.ui_sender.clone(),
                    explorer_id,
                );
            }
            SupportedResources(explorer_id) => {
                //it automatically sends the update to UI
                convo_factory::create_supported_resources_conversation(
                    &self.convo_scheduler,
                    &self.explorer_senders,
                    self.ui_sender.clone(),
                    explorer_id,
                );
            }

            // Asteroid/Sunray Commands
            SendManualAsteroid(planet_id) => {
                convo_factory::create_asteroid_conversation(
                    &self.convo_scheduler,
                    &self.planets_senders,
                    &self.forge,
                    &self.explorers_location,
                    &self.explorer_senders,
                    planet_id,
                );
            }

            SendManualSunray(planet_id) => {
                convo_factory::create_sunray_conversation(
                    &self.convo_scheduler,
                    &self.planets_senders,
                    &self.forge,
                    planet_id,
                );
            }

            // Planet AI Control Commands
            StartPlanetAI(planet_id) => {
                convo_factory::create_start_planet_conversation(
                    &self.convo_scheduler,
                    &self.planets_senders,
                    planet_id,
                );
            }
            StopPlanetAI(planet_id) => {
                convo_factory::create_stop_planet_conversation(
                    &self.convo_scheduler,
                    &self.planets_senders,
                    planet_id,
                );
            }
            ResetPlanetAI(planet_id) => {
                // morally a stop + start
                convo_factory::create_stop_planet_conversation(
                    &self.convo_scheduler,
                    &self.planets_senders,
                    planet_id,
                );

                convo_factory::create_start_planet_conversation(
                    &self.convo_scheduler,
                    &self.planets_senders,
                    planet_id,
                );
            }
            KillPlanetAI(planet_id) => {
                convo_factory::create_kill_planet_conversation(
                    &self.convo_scheduler,
                    &self.planets_senders,
                    &self.explorer_senders,
                    &self.explorers_location,
                    planet_id,
                );
            }

            // Explorer AI Control Commands
            StartExplorerAI(explorer_id) => {
                convo_factory::create_start_explorer_conversation(
                    &self.convo_scheduler,
                    &self.explorer_senders,
                    explorer_id,
                );
            }
            StopExplorerAI(explorer_id) => {
                convo_factory::create_stop_explorer_conversation(
                    &self.convo_scheduler,
                    &self.explorer_senders,
                    explorer_id,
                );
            }
            ResetExplorerAI(explorer_id) => {
                convo_factory::create_reset_explorer_conversation(
                    &self.convo_scheduler,
                    &self.explorer_senders,
                    explorer_id,
                );
            }
            KillExplorerAI(explorer_id) => {
                convo_factory::create_kill_explorer_conversation(
                    &self.convo_scheduler,
                    &self.explorer_senders,
                    &self.planets_senders,
                    &self.explorers_location,
                    explorer_id,
                    *self
                        .explorers_location
                        .lock()
                        .unwrap()
                        .get(&explorer_id)
                        .unwrap(), //TODO: CHECK THAT
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
    ///
    pub fn add_explorer(&mut self, explorer_type: &ExplorerType, into_planet: ID) {
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

        if let Some(_planet_sender) = self
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
