use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use common_explorer::ExplorerBagContent;
use common_game::protocols::orchestrator_explorer::{ExplorerToOrchestrator, OrchestratorToExplorer};
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::protocols::planet_explorer::{ExplorerToPlanet, PlanetToExplorer};
use common_game::utils::ID;
use crossbeam_channel::{Receiver, Sender};
use crate::galaxy_setup::OrchPlanSenderMap;
use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};

pub(crate) type OrchToPlanetSenders = Arc<Mutex<OrchPlanSenderMap>>;
pub(crate) type OrchToExplorerSenders = Arc<Mutex<HashMap<ID, Sender<OrchestratorToExplorer>>>>;
//HashMap PlanetId, Receiver<OrchToPlanet>, saves receivers that are given to planets to communicate with orchestrator
type OrchToPlanReceivers = Arc<Mutex<HashMap<ID, Receiver<OrchestratorToPlanet>>>>;
//HashMap ExplorerId, Receiver<OrchToExp>, saves receivers that are given to explorers to communicate with orchestrator
type OrchToExplorerReceivers = Arc<Mutex<HashMap<ID,Receiver<OrchestratorToExplorer>>>>;
//HasMap ExplorerID, Sender<PlanetToExplorer>, saves Senders associated to the receiver given to the explorer
//Used to give the right sender to the planet to the explorer living on the planet
type PlanetToExplorerSenders = Arc<Mutex<HashMap<ID,Sender<PlanetToExplorer>>>>;
//HashMap ExplorerID, Receiver<PlanetToExplorer>, given to the Explorer at creation time to receive messages from planets
type PlanetToExplorerReceivers = Arc<Mutex<HashMap<ID,Receiver<PlanetToExplorer>>>>;
//HasMap PlanetID, Sender<ExplorerToPlanet>, saves Senders associated to the receiver given to the planet
//Used to give the right sender to the explorer to the planet that is currently hosting him
type ExplorerToPlanetSenders = Arc<Mutex<HashMap<ID,Sender<ExplorerToPlanet>>>>;
//HashMap PlanetID, Receiver<ExplorerID>, given to the Planet at creation time to receive messages from Explorers
type ExplorerToPlanetReceivers = Arc<Mutex<HashMap<ID,Receiver<ExplorerToPlanet>>>>;
pub (crate) struct UIChannels {
    pub(crate) ui_sender: Sender<OrchestratorToUiUpdate>,
    pub(crate) ui_receiver: Receiver<UiToOrchestratorCommand>,
}

impl UIChannels {
    fn new(ui_sender: Sender<OrchestratorToUiUpdate>, ui_receiver: Receiver<UiToOrchestratorCommand>) -> Self {
        Self {
            ui_sender,
            ui_receiver
        }
    }
}

pub (crate) struct PlanetsChannels {
    //Channels to send messages from Orchestrator to the planets and respective receivers
    to_planet_senders: OrchToPlanetSenders,
    to_planet_receivers: OrchToPlanReceivers,
}
impl PlanetsChannels {
    fn new() -> Self {
        Self {
            to_planet_senders: Arc::new(Mutex::new(HashMap::new())),
            to_planet_receivers: Arc::new(Mutex::new(HashMap::new()))
        }
    }

    fn set_orch_to_planet_sender(&self, planet_id: ID, sender: Sender<OrchestratorToPlanet>) {
        self.to_planet_senders.lock().unwrap().insert(planet_id, sender);
    }

    fn add_orch_to_planet_receiver(&self, planet_id: ID, receiver: Receiver<OrchestratorToPlanet>) {
        self.to_planet_receivers.lock().unwrap().insert(planet_id, receiver);
    }

    fn get_orch_to_planet_sender(&self, planet_id: ID) -> Option<Sender<OrchestratorToPlanet>> {
        self.to_planet_senders.lock().unwrap().get(&planet_id).cloned()
    }

    fn get_orch_to_planet_receiver(&self, planet_id: ID) -> Option<Receiver<OrchestratorToPlanet>> {
        self.to_planet_receivers.lock().unwrap().get(&planet_id).cloned()
    }

}

pub (crate) struct ExplorersChannels {
    //Channels to send messages to the explorers and respective receivers
    to_explorer_senders: OrchToExplorerSenders,
    to_explorer_receivers: OrchToExplorerReceivers,
}
impl ExplorersChannels {
    fn new() -> Self {
        Self {
            to_explorer_senders: Arc::new(Mutex::new(HashMap::new())),
            to_explorer_receivers: Arc::new(Mutex::new(HashMap::new()))
        }
    }

    fn set_orch_to_explorer_sender(&self, explorer_id: ID, sender: Sender<OrchestratorToExplorer>) {
        self.to_explorer_senders.lock().unwrap().insert(explorer_id, sender);
    }

    fn set_orch_to_explorer_receiver(&self, explorer_id: ID, receiver: Receiver<OrchestratorToExplorer>) {
        self.to_explorer_receivers.lock().unwrap().insert(explorer_id, receiver);
    }

    fn get_orch_to_explorer_sender(&self, explorer_id: ID) -> Option<Sender<OrchestratorToExplorer>> {
        self.to_explorer_senders.lock().unwrap().get(&explorer_id).cloned()
    }

    fn get_orch_to_explorer_receiver(&self, explorer_id: ID) -> Option<Receiver<OrchestratorToExplorer>> {
        self.to_explorer_receivers.lock().unwrap().get(&explorer_id).cloned()
    }
}

#[derive(Clone)]
pub(crate) struct PlanetExplorerChannels {
    planet_to_explorer_senders: PlanetToExplorerSenders,
    planet_to_explorer_receivers: PlanetToExplorerReceivers,
    explorer_to_planet_senders: ExplorerToPlanetSenders,
    explorer_to_planet_receivers: ExplorerToPlanetReceivers,
}
impl PlanetExplorerChannels {
    fn new() -> Self {
        Self{
            planet_to_explorer_senders: Arc::new(Mutex::new(HashMap::new())),
            planet_to_explorer_receivers: Arc::new(Mutex::new(HashMap::new())),
            explorer_to_planet_receivers: Arc::new(Mutex::new(HashMap::new())),
            explorer_to_planet_senders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn set_planet_to_exp_sender(&self, planet_id: ID, sender: Sender<PlanetToExplorer>) {
        self.planet_to_explorer_senders.lock().unwrap().insert(planet_id, sender);
    }
    fn set_planet_to_exp_recv(&self, planet_id: ID, rcv: Receiver<PlanetToExplorer>) {
        self.planet_to_explorer_receivers.lock().unwrap().insert(planet_id, rcv);
    }
    fn set_exp_to_planet_sender(&self, exp_id: ID, sender: Sender<ExplorerToPlanet>) {
        self.explorer_to_planet_senders.lock().unwrap().insert(exp_id, sender);
    }
    fn set_exp_to_planet_rcv(&self, exp_id: ID, rcv: Receiver<ExplorerToPlanet>) {
        self.explorer_to_planet_receivers.lock().unwrap().insert(exp_id, rcv);
    }

}
pub (crate) struct OrchestratorChannels {
    explorers_channels: ExplorersChannels,
    planets_channels: PlanetsChannels,
    from_planets_receiver: Receiver<PlanetToOrchestrator>,
    planet_to_orch_sender: Sender<PlanetToOrchestrator>,
    from_explorers_receiver: Receiver<ExplorerToOrchestrator<ExplorerBagContent>>,
    exp_to_orch_sender: Sender<ExplorerToOrchestrator<ExplorerBagContent>>,
}

impl OrchestratorChannels {
    fn new() -> Self {
        let (from_planet_tx, from_planet_rx) = crossbeam_channel::unbounded::<PlanetToOrchestrator>();
        let (from_exp_tx, from_exp_rx) = crossbeam_channel::unbounded::<ExplorerToOrchestrator<ExplorerBagContent>>();
        Self {
            explorers_channels: ExplorersChannels::new(),
            planets_channels: PlanetsChannels::new(),
            from_planets_receiver: from_planet_rx,
            planet_to_orch_sender: from_planet_tx,
            from_explorers_receiver: from_exp_rx,
            exp_to_orch_sender: from_exp_tx,
        }
    }

    fn get_planet_to_orch_sender(&self) -> Sender<PlanetToOrchestrator> {
        self.planet_to_orch_sender.clone()
    }

    fn get_exp_to_orch_sender(&self) ->  Sender<ExplorerToOrchestrator<ExplorerBagContent>> {
        self.exp_to_orch_sender.clone()
    }


    fn get_planets_channels_struct(&self) -> &PlanetsChannels {
        &self.planets_channels
    }

    fn get_explorers_channels_struct(&self) -> &ExplorersChannels {
        &self.explorers_channels
    }

}

pub (crate) struct ChannelsManager {
    pub(crate) ui_channels: UIChannels,
    pub(crate) orchestrator_channels: OrchestratorChannels,
    pub(crate) planet_explorer_channels: PlanetExplorerChannels,
}

impl ChannelsManager {
    pub fn new(ui_sender:Sender<OrchestratorToUiUpdate>, ui_receiver: Receiver<UiToOrchestratorCommand> ) -> Self {
        Self {
            ui_channels: UIChannels::new(ui_sender, ui_receiver),
            orchestrator_channels: OrchestratorChannels::new(),
            planet_explorer_channels: PlanetExplorerChannels::new()
        }
    }

    pub fn get_ui_receiver(&self) -> Receiver<UiToOrchestratorCommand> {
        self.ui_channels.ui_receiver.clone()
    }

    pub fn get_orch_channels_struct(&self) -> &OrchestratorChannels {
        &self.orchestrator_channels
    }

    pub fn get_planet_exp_channels(&self) -> &PlanetExplorerChannels {
        &self.planet_explorer_channels
    }

    pub fn create_orch_to_planet_channel(&self, planet_id: ID) -> (Sender<OrchestratorToPlanet>, Receiver<OrchestratorToPlanet>) {
        let (tx, rx) = crossbeam_channel::unbounded::<OrchestratorToPlanet>();
        //set new sender in senders Map
        self.orchestrator_channels.planets_channels.set_orch_to_planet_sender(planet_id, tx.clone());
        //set new receiver in receivers Map
        self.orchestrator_channels.planets_channels.add_orch_to_planet_receiver(planet_id, rx.clone());
        (tx,rx)
    }
    pub fn get_orch_to_planet_sender(&self, planet_id: ID) -> Option<Sender<OrchestratorToPlanet>> {
        self.orchestrator_channels.planets_channels.get_orch_to_planet_sender(planet_id)
    }
    pub fn get_orch_to_planet_receiver(&self, planet_id: ID) -> Option<Receiver<OrchestratorToPlanet>> {
        self.orchestrator_channels.planets_channels.get_orch_to_planet_receiver(planet_id)
    }

    pub fn create_orch_to_explorer_channel(&self, explorer_id: ID) -> (Sender<OrchestratorToExplorer>, Receiver<OrchestratorToExplorer>) {
        let (tx, rx) = crossbeam_channel::unbounded::<OrchestratorToExplorer>();
        //set new sender in senders Map
        self.orchestrator_channels.explorers_channels.set_orch_to_explorer_sender(explorer_id, tx.clone());
        //set new receiver in receivers Map
        self.orchestrator_channels.explorers_channels.set_orch_to_explorer_receiver(explorer_id, rx.clone());
        (tx,rx)
    }

    pub fn get_orch_to_explorer_sender(&self, explorer_id: ID) -> Option<Sender<OrchestratorToExplorer>> {
        self.orchestrator_channels.explorers_channels.get_orch_to_explorer_sender(explorer_id)
    }
    pub fn get_orch_to_explorer_receiver(&self, explorer_id: ID) -> Option<Receiver<OrchestratorToExplorer>> {
        self.orchestrator_channels.explorers_channels.get_orch_to_explorer_receiver(explorer_id)
    }
}