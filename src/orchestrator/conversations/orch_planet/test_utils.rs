#[cfg(test)]
use crate::channels_manager::ChannelsManager;
#[cfg(test)]
use crate::orchestrator::conversations::util::get_test_forge;
#[cfg(test)]
use crate::orchestrator::{ExplorersLocationRef, OrchContext, OrchContextRef};
#[cfg(test)]
use crate::planet::PlanetMap;
#[cfg(test)]
use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
#[cfg(test)]
use common_game::protocols::orchestrator_planet::OrchestratorToPlanet;
#[cfg(test)]
use common_game::utils::ID;
#[cfg(test)]
use crossbeam_channel::{unbounded, Receiver, Sender};
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Arc, RwLock};

#[cfg(test)]
pub(crate) fn make_test_context(
    galaxy: Option<PlanetMap>,
    explorers_location: Option<ExplorersLocationRef>,
    ui_tx: Sender<OrchestratorToUiUpdate>,
    ui_cmd_rx: Receiver<UiToOrchestratorCommand>,
) -> OrchContextRef {
    let channels_manager = Arc::new(ChannelsManager::new(ui_tx, ui_cmd_rx));
    let forge = get_test_forge();
    let galaxy = galaxy.unwrap_or_else(|| Arc::new(RwLock::new(HashMap::new())));
    let explorers_location = explorers_location.unwrap_or_default();

    Arc::new(OrchContext::new(
        channels_manager,
        forge,
        galaxy,
        explorers_location,
    ))
}

#[cfg(test)]
pub(crate) fn add_working_planet_sender(
    channels_manager: &ChannelsManager,
    planet_id: ID,
) -> Receiver<OrchestratorToPlanet> {
    let (tx, rx) = unbounded::<OrchestratorToPlanet>();
    channels_manager
        .get_to_planet_senders_struct_ref()
        .insert(planet_id, tx);
    rx
}

#[cfg(test)]
pub(crate) fn add_broken_planet_sender(channels_manager: &ChannelsManager, planet_id: ID) {
    let (tx, rx) = unbounded::<OrchestratorToPlanet>();
    drop(rx);
    channels_manager
        .get_to_planet_senders_struct_ref()
        .insert(planet_id, tx);
}