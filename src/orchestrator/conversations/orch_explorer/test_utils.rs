#[cfg(test)]
use crate::channels_manager::OrchToExplorerSenders;
#[cfg(test)]
use crate::orchestrator::conversations::util::get_test_forge;
#[cfg(test)]
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer;
#[cfg(test)]
use common_game::utils::ID;
#[cfg(test)]
use crate::convo_manager::OrchContextRef;

#[cfg(test)]
pub(crate) struct MakeSendersResult(
    pub(crate) OrchToExplorerSenders,
    pub(crate) crossbeam_channel::Receiver<OrchestratorToExplorer>,
);

// --- Helper functions ---
#[cfg(test)]
pub(crate) fn make_senders_with(explorer_id: ID) -> MakeSendersResult {
    use crossbeam_channel::unbounded;
    use dashmap::DashMap;

    let (tx, rx) = unbounded::<OrchestratorToExplorer>();
    let map = DashMap::new();
    map.insert(explorer_id, tx);
    MakeSendersResult(map, rx)
}

#[cfg(test)]
pub(crate) fn make_empty_senders() -> OrchToExplorerSenders {
    use dashmap::DashMap;
    DashMap::new()
}

#[cfg(test)]
pub(crate) fn make_orch_context(senders: OrchToExplorerSenders) -> OrchContextRef {
    use crate::channels_manager::ChannelsManager;
    use crate::orchestrator::OrchContext;
    use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
    use dashmap::DashMap;
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    let (ui_tx, _ui_rx) = crossbeam_channel::unbounded::<OrchestratorToUiUpdate>();
    let (_orch_tx, orch_rx) = crossbeam_channel::unbounded::<UiToOrchestratorCommand>();

    let cm = Arc::new(ChannelsManager::new(ui_tx, orch_rx));
    
    let cm_senders = cm.get_orch_to_exp_senders_struct_ref();
    for (id, sender) in senders.into_iter() {
        cm_senders.insert(id, sender);
    }

    let forge = get_test_forge();
    let galaxy = Arc::new(RwLock::new(HashMap::new()));
    let explorers_location = DashMap::new();

    Arc::new(OrchContext::new(
        cm,
        forge,
        galaxy,
        explorers_location,
    ))
}
