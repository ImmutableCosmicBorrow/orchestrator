use crate::galaxy_setup::OrchPlanSenderMap;
use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
use common_explorer::ExplorerBagContent;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::protocols::planet_explorer::{ExplorerToPlanet, PlanetToExplorer};
use common_game::utils::ID;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

//TODO: ADD DOCUMENTATION
//TODO: MAYBE DELETE SMART POINTERS INSIDE AS THEY'RE NOT NEEDED (ALREADY PROTECTED BY THE RWLOCK OUTSIDE)

pub(crate) type OrchToPlanetSenders = Arc<Mutex<OrchPlanSenderMap>>;
pub(crate) type OrchToExplorerSenders = Arc<Mutex<HashMap<ID, Sender<OrchestratorToExplorer>>>>;
//HashMap PlanetId, Receiver<OrchToPlanet>, saves receivers that are given to planets to communicate with orchestrator
type OrchToPlanReceivers = Arc<Mutex<HashMap<ID, Receiver<OrchestratorToPlanet>>>>;
//HashMap ExplorerId, Receiver<OrchToExp>, saves receivers that are given to explorers to communicate with orchestrator
type OrchToExplorerReceivers = Arc<Mutex<HashMap<ID, Receiver<OrchestratorToExplorer>>>>;
//HasMap ExplorerID, Sender<PlanetToExplorer>, saves Senders associated to the receiver given to the explorer
//Used to give the right sender to the planet to the explorer living on the planet
type PlanetToExplorerSenders = Arc<Mutex<HashMap<ID, Sender<PlanetToExplorer>>>>;
//HashMap ExplorerID, Receiver<PlanetToExplorer>, given to the Explorer at creation time to receive messages from planets
type PlanetToExplorerReceivers = Arc<Mutex<HashMap<ID, Receiver<PlanetToExplorer>>>>;
//HasMap PlanetID, Sender<ExplorerToPlanet>, saves Senders associated to the receiver given to the planet
//Used to give the right sender to the explorer to the planet that is currently hosting him
type ExplorerToPlanetSenders = Arc<Mutex<HashMap<ID, Sender<ExplorerToPlanet>>>>;
//HashMap PlanetID, Receiver<ExplorerID>, given to the Planet at creation time to receive messages from Explorers
type ExplorerToPlanetReceivers = Arc<Mutex<HashMap<ID, Receiver<ExplorerToPlanet>>>>;

#[derive(Clone)]
pub(crate) struct UIChannels {
    pub(crate) ui_sender: Sender<OrchestratorToUiUpdate>,
    pub(crate) ui_receiver: Receiver<UiToOrchestratorCommand>,
}

impl UIChannels {
    fn new(
        ui_sender: Sender<OrchestratorToUiUpdate>,
        ui_receiver: Receiver<UiToOrchestratorCommand>,
    ) -> Self {
        Self {
            ui_sender,
            ui_receiver,
        }
    }
}

#[derive(Clone)]
pub(crate) struct PlanetsChannels {
    //Channels to send messages from Orchestrator to the planets and respective receivers
    to_planet_senders: OrchToPlanetSenders,
    to_planet_receivers: OrchToPlanReceivers,
    from_planets_sender: Sender<PlanetToOrchestrator>,
    from_planets_receiver: Receiver<PlanetToOrchestrator>,
}

#[allow(dead_code)]
impl PlanetsChannels {
    fn new() -> Self {
        let (from_planets_sender, from_planets_receiver) =
            crossbeam_channel::unbounded::<PlanetToOrchestrator>();
        Self {
            to_planet_senders: Arc::new(Mutex::new(HashMap::new())),
            to_planet_receivers: Arc::new(Mutex::new(HashMap::new())),
            from_planets_sender,
            from_planets_receiver,
        }
    }

    fn set_orch_to_planet_sender(&self, planet_id: ID, sender: Sender<OrchestratorToPlanet>) {
        self.to_planet_senders
            .lock()
            .unwrap()
            .insert(planet_id, sender);
    }

    fn set_orch_to_planet_rcv(&self, planet_id: ID, receiver: Receiver<OrchestratorToPlanet>) {
        self.to_planet_receivers
            .lock()
            .unwrap()
            .insert(planet_id, receiver);
    }

    fn get_orch_to_planet_sender(&self, planet_id: ID) -> Option<Sender<OrchestratorToPlanet>> {
        self.to_planet_senders
            .lock()
            .unwrap()
            .get(&planet_id)
            .cloned()
    }

    fn get_orch_to_planet_rcv(&self, planet_id: ID) -> Option<Receiver<OrchestratorToPlanet>> {
        self.to_planet_receivers
            .lock()
            .unwrap()
            .get(&planet_id)
            .cloned()
    }

    fn get_planet_to_orch_sender(&self) -> Sender<PlanetToOrchestrator> {
        self.from_planets_sender.clone()
    }

    fn get_from_planets_rcv(&self) -> Receiver<PlanetToOrchestrator> {
        self.from_planets_receiver.clone()
    }

    fn to_planet_senders_contains(&self, planet_id: ID) -> bool {
        self.to_planet_senders
            .lock()
            .unwrap()
            .contains_key(&planet_id)
    }

    fn to_planet_senders_next_id(&self) -> Option<ID> {
        self.to_planet_senders
            .lock()
            .unwrap()
            .keys()
            .next()
            .copied()
    }
}

#[derive(Clone)]
pub(crate) struct ExplorersChannels {
    //Channels to send messages to the explorers and respective receivers
    to_explorer_senders: OrchToExplorerSenders,
    to_explorer_receivers: OrchToExplorerReceivers,
    from_explorers_sender: Sender<ExplorerToOrchestrator<ExplorerBagContent>>,
    from_explorers_receiver: Receiver<ExplorerToOrchestrator<ExplorerBagContent>>,
}

#[allow(dead_code)]
impl ExplorersChannels {
    fn new() -> Self {
        let (from_explorers_sender, from_explorers_receiver) =
            crossbeam_channel::unbounded::<ExplorerToOrchestrator<ExplorerBagContent>>();
        Self {
            to_explorer_senders: Arc::new(Mutex::new(HashMap::new())),
            to_explorer_receivers: Arc::new(Mutex::new(HashMap::new())),
            from_explorers_sender,
            from_explorers_receiver,
        }
    }

    fn set_orch_to_explorer_sender(&self, explorer_id: ID, sender: Sender<OrchestratorToExplorer>) {
        self.to_explorer_senders
            .lock()
            .unwrap()
            .insert(explorer_id, sender);
    }

    fn set_orch_to_explorer_rcv(
        &self,
        explorer_id: ID,
        receiver: Receiver<OrchestratorToExplorer>,
    ) {
        self.to_explorer_receivers
            .lock()
            .unwrap()
            .insert(explorer_id, receiver);
    }

    fn get_orch_to_explorer_sender(
        &self,
        explorer_id: ID,
    ) -> Option<Sender<OrchestratorToExplorer>> {
        self.to_explorer_senders
            .lock()
            .unwrap()
            .get(&explorer_id)
            .cloned()
    }

    fn get_orch_to_explorer_rcv(
        &self,
        explorer_id: ID,
    ) -> Option<Receiver<OrchestratorToExplorer>> {
        self.to_explorer_receivers
            .lock()
            .unwrap()
            .get(&explorer_id)
            .cloned()
    }

    fn get_from_explorers_rcv(&self) -> Receiver<ExplorerToOrchestrator<ExplorerBagContent>> {
        self.from_explorers_receiver.clone()
    }
    fn get_exp_to_orch_sender(&self) -> Sender<ExplorerToOrchestrator<ExplorerBagContent>> {
        self.from_explorers_sender.clone()
    }
}

#[derive(Clone)]
pub(crate) struct PlanetExplorerChannels {
    pub(crate) planet_to_explorer_senders: PlanetToExplorerSenders,
    planet_to_explorer_receivers: PlanetToExplorerReceivers,
    pub(crate) explorer_to_planet_senders: ExplorerToPlanetSenders,
    explorer_to_planet_receivers: ExplorerToPlanetReceivers,
}

#[allow(dead_code)]
impl PlanetExplorerChannels {
    fn new() -> Self {
        Self {
            planet_to_explorer_senders: Arc::new(Mutex::new(HashMap::new())),
            planet_to_explorer_receivers: Arc::new(Mutex::new(HashMap::new())),
            explorer_to_planet_receivers: Arc::new(Mutex::new(HashMap::new())),
            explorer_to_planet_senders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn set_planet_to_exp_sender(&self, exp_id: ID, sender: Sender<PlanetToExplorer>) {
        self.planet_to_explorer_senders
            .lock()
            .unwrap()
            .insert(exp_id, sender);
    }
    fn set_planet_to_exp_rcv(&self, exp_id: ID, rcv: Receiver<PlanetToExplorer>) {
        self.planet_to_explorer_receivers
            .lock()
            .unwrap()
            .insert(exp_id, rcv);
    }
    fn set_exp_to_planet_sender(&self, planet_id: ID, sender: Sender<ExplorerToPlanet>) {
        self.explorer_to_planet_senders
            .lock()
            .unwrap()
            .insert(planet_id, sender);
    }
    fn set_exp_to_planet_rcv(&self, planet_id: ID, rcv: Receiver<ExplorerToPlanet>) {
        self.explorer_to_planet_receivers
            .lock()
            .unwrap()
            .insert(planet_id, rcv);
    }

    fn get_planet_to_exp_sender(&self, exp_id: ID) -> Option<Sender<PlanetToExplorer>> {
        self.planet_to_explorer_senders
            .lock()
            .unwrap()
            .get(&exp_id)
            .cloned()
    }

    fn get_planet_to_exp_rcv(&self, exp_id: ID) -> Option<Receiver<PlanetToExplorer>> {
        self.planet_to_explorer_receivers
            .lock()
            .unwrap()
            .get(&exp_id)
            .cloned()
    }

    fn get_exp_to_planet_sender(&self, planet_id: ID) -> Option<Sender<ExplorerToPlanet>> {
        self.explorer_to_planet_senders
            .lock()
            .unwrap()
            .get(&planet_id)
            .cloned()
    }

    fn get_exp_to_planet_rcv(&self, planet_id: ID) -> Option<Receiver<ExplorerToPlanet>> {
        self.explorer_to_planet_receivers
            .lock()
            .unwrap()
            .get(&planet_id)
            .cloned()
    }
}
#[derive(Clone)]
pub(crate) struct OrchestratorChannels {
    explorers_channels: ExplorersChannels,
    planets_channels: PlanetsChannels,
}

#[allow(dead_code)]
impl OrchestratorChannels {
    fn new() -> Self {
        Self {
            explorers_channels: ExplorersChannels::new(),
            planets_channels: PlanetsChannels::new(),
        }
    }

    fn get_planets_channels_struct(&self) -> &PlanetsChannels {
        &self.planets_channels
    }

    fn get_explorers_channels_struct(&self) -> &ExplorersChannels {
        &self.explorers_channels
    }
}

#[derive(Clone)]
pub(crate) struct ChannelsManager {
    ui: UIChannels,
    orchestrator: OrchestratorChannels,
    planet_explorer: PlanetExplorerChannels,
}

#[allow(dead_code)]
impl ChannelsManager {
    pub(crate) fn new(
        ui_sender: Sender<OrchestratorToUiUpdate>,
        ui_receiver: Receiver<UiToOrchestratorCommand>,
    ) -> Self {
        Self {
            ui: UIChannels::new(ui_sender, ui_receiver),
            orchestrator: OrchestratorChannels::new(),
            planet_explorer: PlanetExplorerChannels::new(),
        }
    }

    //
    // ──────────────────────────────────────────────────────────────────────────
    // ORCHESTRATOR - UI CHANNELS
    // ──────────────────────────────────────────────────────────────────────────
    //

    pub(crate) fn get_ui_sender(&self) -> Sender<OrchestratorToUiUpdate> {
        self.ui.ui_sender.clone()
    }
    pub(crate) fn get_ui_sender_ref(&self) -> &Sender<OrchestratorToUiUpdate> {
        &self.ui.ui_sender
    }
    pub(crate) fn get_ui_receiver(&self) -> Receiver<UiToOrchestratorCommand> {
        self.ui.ui_receiver.clone()
    }
    pub(crate) fn get_ui_receiver_ref(&self) -> &Receiver<UiToOrchestratorCommand> {
        &self.ui.ui_receiver
    }

    //
    // ──────────────────────────────────────────────────────────────────────────
    // ORCHESTRATOR - PLANET CHANNELS
    // ──────────────────────────────────────────────────────────────────────────
    //

    //crates tuple (tx,rx) for Orch to Planet comms for specific planet ID and stores them in the appropriate map
    pub(crate) fn create_orch_to_planet_channel(
        &self,
        planet_id: ID,
    ) -> (Sender<OrchestratorToPlanet>, Receiver<OrchestratorToPlanet>) {
        let (tx, rx) = crossbeam_channel::unbounded::<OrchestratorToPlanet>();
        //set new sender in senders Map
        self.orchestrator
            .planets_channels
            .set_orch_to_planet_sender(planet_id, tx.clone());
        //set new receiver in receivers Map
        self.orchestrator
            .planets_channels
            .set_orch_to_planet_rcv(planet_id, rx.clone());
        (tx, rx)
    }
    pub(crate) fn get_orch_to_planet_sender(
        &self,
        planet_id: ID,
    ) -> Option<Sender<OrchestratorToPlanet>> {
        self.orchestrator
            .planets_channels
            .get_orch_to_planet_sender(planet_id)
    }
    pub(crate) fn get_orch_to_planet_receiver(
        &self,
        planet_id: ID,
    ) -> Option<Receiver<OrchestratorToPlanet>> {
        self.orchestrator
            .planets_channels
            .get_orch_to_planet_rcv(planet_id)
    }

    pub(crate) fn get_from_planets_sender(&self) -> Sender<PlanetToOrchestrator> {
        self.orchestrator
            .planets_channels
            .get_planet_to_orch_sender()
    }

    pub(crate) fn get_from_planets_receiver(&self) -> Receiver<PlanetToOrchestrator> {
        self.orchestrator.planets_channels.get_from_planets_rcv()
    }

    pub(crate) fn to_planet_senders_contains(&self, planet_id: ID) -> bool {
        self.orchestrator
            .planets_channels
            .to_planet_senders_contains(planet_id)
    }

    pub(crate) fn to_planet_senders_next_id(&self) -> Option<ID> {
        self.orchestrator
            .planets_channels
            .to_planet_senders_next_id()
    }

    pub(crate) fn get_to_planet_senders_struct_ref(&self) -> &OrchToPlanetSenders {
        &self.orchestrator.planets_channels.to_planet_senders
    }

    pub(crate) fn get_to_planet_senders_struct(&self) -> OrchToPlanetSenders {
        self.orchestrator.planets_channels.to_planet_senders.clone()
    }

    pub(crate) fn get_from_planet_rcv_ref(&self) -> &Receiver<PlanetToOrchestrator> {
        &self.orchestrator.planets_channels.from_planets_receiver
    }
    pub(crate) fn get_from_planet_rcv(&self) -> Receiver<PlanetToOrchestrator> {
        self.orchestrator
            .planets_channels
            .from_planets_receiver
            .clone()
    }

    //
    // ──────────────────────────────────────────────────────────────────────────
    // ORCHESTRATOR - EXPLORER CHANNELS
    // ──────────────────────────────────────────────────────────────────────────
    //
    pub(crate) fn create_orch_to_explorer_channel(
        &self,
        explorer_id: ID,
    ) -> (
        Sender<OrchestratorToExplorer>,
        Receiver<OrchestratorToExplorer>,
    ) {
        let (tx, rx) = crossbeam_channel::unbounded::<OrchestratorToExplorer>();
        //set new sender in senders Map
        self.orchestrator
            .explorers_channels
            .set_orch_to_explorer_sender(explorer_id, tx.clone());
        //set new receiver in receivers Map
        self.orchestrator
            .explorers_channels
            .set_orch_to_explorer_rcv(explorer_id, rx.clone());
        (tx, rx)
    }

    pub(crate) fn get_orch_to_explorer_sender(
        &self,
        explorer_id: ID,
    ) -> Option<Sender<OrchestratorToExplorer>> {
        self.orchestrator
            .explorers_channels
            .get_orch_to_explorer_sender(explorer_id)
    }
    pub(crate) fn get_orch_to_explorer_rcv(
        &self,
        explorer_id: ID,
    ) -> Option<Receiver<OrchestratorToExplorer>> {
        self.orchestrator
            .explorers_channels
            .get_orch_to_explorer_rcv(explorer_id)
    }

    pub(crate) fn get_orch_to_exp_senders_struct_ref(&self) -> &OrchToExplorerSenders {
        &self.orchestrator.explorers_channels.to_explorer_senders
    }
    pub(crate) fn get_orch_to_exp_senders_struct(&self) -> OrchToExplorerSenders {
        self.orchestrator
            .explorers_channels
            .to_explorer_senders
            .clone()
    }
    pub(crate) fn get_from_explorers_sender(
        &self,
    ) -> Sender<ExplorerToOrchestrator<ExplorerBagContent>> {
        self.orchestrator
            .explorers_channels
            .get_exp_to_orch_sender()
    }

    pub(crate) fn get_from_explorers_rcv(
        &self,
    ) -> Receiver<ExplorerToOrchestrator<ExplorerBagContent>> {
        self.orchestrator
            .explorers_channels
            .get_from_explorers_rcv()
    }

    pub(crate) fn get_from_explorers_rcv_ref(
        &self,
    ) -> &Receiver<ExplorerToOrchestrator<ExplorerBagContent>> {
        &self.orchestrator.explorers_channels.from_explorers_receiver
    }

    //
    // ──────────────────────────────────────────────────────────────────────────
    // PLANET - EXPLORER CHANNELS
    // ──────────────────────────────────────────────────────────────────────────
    //
    pub(crate) fn create_planet_to_exp_channel(
        &self,
        exp_id: ID,
    ) -> (Sender<PlanetToExplorer>, Receiver<PlanetToExplorer>) {
        let (tx, rx) = crossbeam_channel::unbounded::<PlanetToExplorer>();
        self.planet_explorer
            .set_planet_to_exp_sender(exp_id, tx.clone());
        self.planet_explorer
            .set_planet_to_exp_rcv(exp_id, rx.clone());
        (tx, rx)
    }
    pub(crate) fn get_planet_to_exp_sender(&self, exp_id: ID) -> Option<Sender<PlanetToExplorer>> {
        self.planet_explorer.get_planet_to_exp_sender(exp_id)
    }
    pub(crate) fn get_planet_to_exp_rcv(&self, exp_id: ID) -> Option<Receiver<PlanetToExplorer>> {
        self.planet_explorer.get_planet_to_exp_rcv(exp_id)
    }

    pub(crate) fn create_exp_to_planet_channel(
        &self,
        planet_id: ID,
    ) -> (Sender<ExplorerToPlanet>, Receiver<ExplorerToPlanet>) {
        let (tx, rx) = crossbeam_channel::unbounded::<ExplorerToPlanet>();
        self.planet_explorer
            .set_exp_to_planet_sender(planet_id, tx.clone());
        self.planet_explorer
            .set_exp_to_planet_rcv(planet_id, rx.clone());
        (tx, rx)
    }
    pub(crate) fn get_exp_to_planet_sender(
        &self,
        planet_id: ID,
    ) -> Option<Sender<ExplorerToPlanet>> {
        self.planet_explorer.get_exp_to_planet_sender(planet_id)
    }
    pub(crate) fn get_exp_to_planet_rcv(
        &self,
        planet_id: ID,
    ) -> Option<Receiver<ExplorerToPlanet>> {
        self.planet_explorer.get_exp_to_planet_rcv(planet_id)
    }

    pub(crate) fn get_planet_explorer_struct(&self) -> &PlanetExplorerChannels {
        &self.planet_explorer
    }
}
