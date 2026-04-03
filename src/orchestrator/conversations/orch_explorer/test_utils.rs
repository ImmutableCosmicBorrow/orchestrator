#[cfg(test)]
use crate::channels_manager::OrchToExplorerSenders;
#[cfg(test)]
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer;
use common_game::utils::ID;

#[cfg(test)]
pub(crate) struct MakeSendersResult(
    pub(crate) OrchToExplorerSenders,
    pub(crate) crossbeam_channel::Receiver<OrchestratorToExplorer>,
);

// --- Helper functions ---
/*#[cfg(test)]
pub(crate) fn make_senders_with(explorer_id: ID) -> MakeSendersResult {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use crossbeam_channel::unbounded;

    let (tx, rx) = unbounded::<OrchestratorToExplorer>();
    MakeSendersResult(Arc::new(Mutex::new(HashMap::from([(explorer_id, tx)]))), rx)
}
#[cfg(test)]
pub(crate) fn make_empty_senders() -> OrchToExplorerSenders {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    Arc::new(Mutex::new(HashMap::new()))
}

 */
/*
#[cfg(test)]
pub(crate) fn make_to_explorer_struct(
    explorer_id: ID,
    senders: OrchToExplorerSenders,
) -> ToExplorerStruct {
    ToExplorerStruct {
        explorer_id,
        explorers_senders: senders,
    }
}

pub(crate) fn make_to_planet_struct(planet_id: ID, senders: OrchToPlanetSenders) -> ToPlanetStruct {
    ToPlanetStruct {
        planet_id,
        planets_senders: senders,
    }
}
*/
