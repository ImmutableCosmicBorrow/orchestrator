//! Background event scheduler API and internal module wiring.

mod context;
mod control;
mod dispatch;
mod planning;
mod regimes;
mod scheduler;
mod state;
mod timing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EventKind {
    Asteroid,
    Sunray,
}

use crate::channels_manager::ChannelsManager;
use crate::orchestrator::queue::ConvoScheduler;
use crate::orchestrator::{ChannelsManagerRef, ExplorerBagContent, ExplorersLocationRef};
use crate::planet::PlanetMap;
use common_game::components::forge::Forge;
use std::sync::Arc;

pub(super) struct BackgroundEventsGuard(control::BackgroundEventsGuard);

pub(super) fn enable_asteroids() {
    control::enable_asteroids();
}

pub(super) fn disable_asteroids() {
    control::disable_asteroids();
}

pub(super) fn enable_sunrays() {
    control::enable_sunrays();
}

pub(super) fn disable_sunrays() {
    control::disable_sunrays();
}

pub(super) fn enable_auto_regime_progression() {
    control::enable_auto_regime_progression();
}

pub(super) fn disable_auto_regime_progression() {
    control::disable_auto_regime_progression();
}

pub(super) fn set_auto_regime_progression(enabled: bool) {
    control::set_auto_regime_progression(enabled);
}

pub(super) fn increase_asteroid_regime() {
    control::increase_asteroid_regime();
}

pub(super) fn decrease_asteroid_regime() {
    control::decrease_asteroid_regime();
}

pub(super) fn increase_sunray_regime() {
    control::increase_sunray_regime();
}

pub(super) fn decrease_sunray_regime() {
    control::decrease_sunray_regime();
}

pub(super) fn shutdown_background_events() {
    control::shutdown_background_events();
}

pub(super) fn background_events_guard() -> BackgroundEventsGuard {
    BackgroundEventsGuard(control::background_events_guard())
}

pub(super) fn init_background_event_scheduler(
    channels_manager: ChannelsManagerRef,
    forge: Arc<Forge>,
    explorers_location: ExplorersLocationRef,
    convo_scheduler: Arc<ConvoScheduler>,
    galaxy: PlanetMap,
) {
    scheduler::init_background_event_scheduler(
        channels_manager,
        forge,
        explorers_location,
        convo_scheduler,
        galaxy,
    );
}
