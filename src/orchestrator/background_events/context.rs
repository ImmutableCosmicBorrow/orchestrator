//! Shared scheduler and dispatch contexts for background events.

use crate::convo_manager::ConvoManager;
use crate::orchestrator::ExplorersLocationRef;
use crate::planet::PlanetMap;
use std::sync::{Arc, Mutex};

//TODO: MAYBE SIMPLIFY AND JUST TAKE CONVO_MANAGER?
pub(super) struct WorldCtx {
    pub(super) galaxy: PlanetMap,
    pub(super) explorers_location: ExplorersLocationRef,
}

pub(super) struct DispatchCtx {
    pub(super) convo_manager: Arc<Mutex<ConvoManager>>,
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
    pub(super) fn new(convo_manager: Arc<Mutex<ConvoManager>>) -> Self {
        Self { convo_manager }
    }
}
