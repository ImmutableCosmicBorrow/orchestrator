//! Shared scheduler and dispatch contexts for background events.

use crate::channels_manager::ChannelsManager;
use crate::orchestrator::queue::ConvoScheduler;
use crate::orchestrator::{ExplorerBagContent, ExplorersLocationRef};
use crate::planet::PlanetMap;
use common_game::components::forge::Forge;
use std::sync::Arc;

pub(super) struct WorldCtx {
    pub(super) galaxy: PlanetMap,
    pub(super) explorers_location: ExplorersLocationRef,
}

pub(super) struct DispatchCtx {
    pub(super) channels_manager: Arc<ChannelsManager>,
    pub(super) forge: Arc<Forge>,
    pub(super) convo_scheduler: ConvoScheduler<ExplorerBagContent>,
}

impl WorldCtx {
    pub(super) fn new(galaxy: PlanetMap, explorers_location: ExplorersLocationRef) -> Self {
        Self {
            galaxy,
            explorers_location,
        }
    }
}

impl DispatchCtx {
    pub(super) fn new(
        channels_manager: Arc<ChannelsManager>,
        forge: Arc<Forge>,
        convo_scheduler: ConvoScheduler<ExplorerBagContent>,
    ) -> Self {
        Self {
            channels_manager,
            forge,
            convo_scheduler,
        }
    }
}
