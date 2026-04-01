use std::sync::Arc;
use common_game::components::forge::Forge;
use crate::channels_manager::ChannelsManager;
use crate::convo_manager::convo_scheduler::ConvoScheduler;
use crate::orchestrator::{ChannelsManagerRef, ExplorersLocationRef};
use crate::planet::PlanetMap;

pub mod queue;
mod convo_scheduler;
mod message_handler;
pub mod convo_factory;

pub(crate) struct OrchContext {
    channels_manager: ChannelsManagerRef,
    forge: Arc<Forge>,
    galaxy: PlanetMap,
    explorers_location: ExplorersLocationRef,
}

impl OrchContext {
    pub(crate) fn new(
        channels_manager: ChannelsManagerRef,
        forge: Arc<Forge>,
        galaxy: PlanetMap,
        explorers_location: ExplorersLocationRef, 
    ) -> Self {
        Self {
            channels_manager,
            forge,
            galaxy,
            explorers_location 
        }
    }
    
    pub(crate) fn get_channels_manager(&self) -> ChannelsManagerRef {
        self.channels_manager.clone()
    }
    pub(crate) fn get_forge(&self) -> Arc<Forge> {
        self.forge.clone()
    }
    pub(crate) fn get_galaxy(&self) -> PlanetMap {
        self.galaxy.clone()
    }
    
    pub(crate) fn get_explorers_location(&self) -> ExplorersLocationRef {
        self.explorers_location.clone()
    }
}

pub(crate) struct ConvoManager {
    pub(crate) convo_scheduler: ConvoScheduler,
    orch_context: OrchContext
}

impl ConvoManager {
    pub(crate) fn new(orch_context: OrchContext) -> Self {
        Self { convo_scheduler: ConvoScheduler::new(), orch_context }
    }
    
    pub(crate) fn get_convo_scheduler_ref(&self) -> &ConvoScheduler {
        &self.convo_scheduler
    }
    
    pub(crate) fn get_orch_context_ref(&self) -> &OrchContext {
        &self.orch_context
    }
}

