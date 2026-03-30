//! Shared scheduler and dispatch contexts for background events.

use crate::orchestrator::{ChannelsManagerRef, ExplorerBagContent, ExplorersLocationRef};
use crate::planet::PlanetMap;
use common_game::components::forge::Forge;
use std::sync::Arc;
use crate::convo_manager::convo_factory::ConvoFactory;

pub(super) struct WorldCtx {
    pub(super) galaxy: PlanetMap,
    pub(super) explorers_location: ExplorersLocationRef,
}

pub(super) struct DispatchCtx {
    pub(super) channels_manager: ChannelsManagerRef,
    pub(super) forge: Arc<Forge>,
    pub(super) convo_factory: Arc<ConvoFactory>,
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
        channels_manager: ChannelsManagerRef,
        forge: Arc<Forge>,
        convo_factory: Arc<ConvoFactory>,
    ) -> Self {
        Self {
            channels_manager,
            forge,
            convo_factory,
        }
    }
}
