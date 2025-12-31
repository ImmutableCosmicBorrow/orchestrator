use crate::galaxy_setup::OrchPlanSenderMap;
use common_game::components::planet::Planet;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::protocols::planet_explorer::ExplorerToPlanet;
use common_game::utils::ID;
use crossbeam_channel::Sender;
use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

mod orch_explorer;
mod orch_planet;

trait Conversation<T: Debug> {
    fn get_id(&self) -> ID;
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds>;
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<T>>,
    ) -> Result<Option<Box<dyn Conversation<T>>>, (Option<Box<dyn Conversation<T>>>, String)>;
}

#[derive(Debug, Clone)]
enum PossibleExpectedKinds {
    PlanetToOrchKind(PlanetToOrchestratorKind),
    ExplorerToOrchKind(ExplorerToOrchestratorKind),
}

enum PossibleMessage<T> {
    PlanetToOrch(PlanetToOrchestrator),
    ExplorerToOrch(ExplorerToOrchestrator<T>),
}

enum ExpectedMessageKind {
    PlanetToOrchestrator(PlanetToOrchestratorKind),
    ExplorerToOrchestrator(ExplorerToOrchestratorKind),
}

pub(crate) trait ConversationWithPlanet {
    fn to_planet(&self, msg: OrchestratorToPlanet, planet_id: ID) -> Result<(), String>;
}

pub(crate) trait ConversationWithExplorer {
    fn to_explorer(&self, msg: OrchestratorToExplorer, explorer_id: ID) -> Result<(), String>;
}

pub(crate) type SendersToPlanet = Arc<Mutex<OrchPlanSenderMap>>;
pub(crate) type SendersToExplorer = Arc<Mutex<HashMap<ID, Sender<OrchestratorToExplorer>>>>;
pub(crate) type ExplorersBagRef<T> = Arc<HashMap<ID, T>>;
pub(crate) struct ToPlanetStruct {
    planets_senders: SendersToPlanet,
    planet_id: ID,
}

impl ToPlanetStruct {
    pub(crate) fn new(planets_senders: SendersToPlanet, planet_id: ID) -> Self {
        Self {
            planets_senders,
            planet_id,
        }
    }

    pub(crate) fn to_planet(&self, msg: OrchestratorToPlanet) -> Result<(), String> {
        let sender = {
            let lock = self.planets_senders.lock().unwrap();
            lock.get(&self.planet_id).cloned() // Clone the Sender handle
        };

        if let Some(s) = sender {
            s.send(msg)
                .map_err(|e| format!("Failed to send message to planet {}: {e}", self.planet_id))
        } else {
            Err("Sender not Found!".to_string())
        }
    }
}

pub(crate) struct ToExplorerStruct {
    explorers_senders: SendersToExplorer,
    explorer_id: ID,
}

impl ToExplorerStruct {
    pub(crate) fn to_explorer(&self, msg: OrchestratorToExplorer) -> Result<(), String> {
        let sender = {
            let lock = self.explorers_senders.lock().unwrap();
            lock.get(&self.explorer_id).cloned() // Clone the Sender handle
        };

        if let Some(s) = sender {
            s.send(msg).map_err(|e| {
                format!(
                    "Failed to send message to explorer {}: {e}",
                    self.explorer_id
                )
            })
        } else {
            Err("Sender not Found!".to_string())
        }
    }
}
