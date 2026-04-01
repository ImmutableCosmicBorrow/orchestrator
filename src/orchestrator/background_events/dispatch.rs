//! Event emission and conversation creation.

use super::EventKind;
use super::context::{DispatchCtx, WorldCtx};
use super::state::PlannedEvent;
use crate::logging_utils::{LogTarget, log_internal};
use crate::convo_manager::convo_factory;
use crate::payload;
use common_game::logging::Channel;

pub(super) fn dispatch(event: PlannedEvent, world: &WorldCtx, dispatch_ctx: &DispatchCtx) {
    match event.kind {
        EventKind::Asteroid => dispatch_asteroid(event, world, dispatch_ctx),
        EventKind::Sunray => dispatch_sunray(event, dispatch_ctx),
    }
}

fn dispatch_asteroid(event: PlannedEvent, _world_ctx: &WorldCtx, dispatch_ctx: &DispatchCtx) {

    log_internal(
        LogTarget::AsteroidsSunrays,
        Channel::Trace,
        payload!(action: "Sending asteroid", planet_id: event.planet_id),
    );

    dispatch_ctx.convo_manager.lock().unwrap().create_asteroid_conversation(event.planet_id);
}

fn dispatch_sunray(event: PlannedEvent, dispatch_ctx: &DispatchCtx) {

    log_internal(
        LogTarget::AsteroidsSunrays,
        Channel::Trace,
        payload!(action: "Sending sunray", planet_id: event.planet_id),
    );

    dispatch_ctx.convo_manager.lock().unwrap().create_sunray_conversation(event.planet_id);
}
