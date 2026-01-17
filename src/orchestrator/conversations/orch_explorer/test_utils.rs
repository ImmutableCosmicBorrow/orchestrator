use crate::orchestrator::conversations::{
    SendersToExplorer, SendersToPlanet, ToExplorerStruct, ToPlanetStruct,
};
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer;
use common_game::utils::ID;
use crossbeam_channel::unbounded;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[cfg(test)]
pub(crate) struct MakeSendersResult(
    pub(crate) SendersToExplorer,
    pub(crate) crossbeam_channel::Receiver<OrchestratorToExplorer>,
);

// --- Helper functions ---
#[cfg(test)]
pub(crate) fn make_senders_with(explorer_id: ID) -> MakeSendersResult {
    let (tx, rx) = unbounded::<OrchestratorToExplorer>();
    MakeSendersResult(Arc::new(Mutex::new(HashMap::from([(explorer_id, tx)]))), rx)
}
#[cfg(test)]
pub(crate) fn make_empty_senders() -> SendersToExplorer {
    Arc::new(Mutex::new(HashMap::new()))
}

#[cfg(test)]
pub(crate) fn make_to_explorer_struct(
    explorer_id: ID,
    senders: SendersToExplorer,
) -> ToExplorerStruct {
    ToExplorerStruct {
        explorer_id,
        explorers_senders: senders,
    }
}

pub(crate) fn make_to_planet_struct(planet_id: ID, senders: SendersToPlanet) -> ToPlanetStruct {
    ToPlanetStruct {
        planet_id,
        planets_senders: senders,
    }
}
