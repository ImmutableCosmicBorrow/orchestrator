//! Event emission and conversation creation.

use super::EventKind;
use super::context::{DispatchCtx, WorldCtx};
use super::state::PlannedEvent;
use crate::logging_utils::{LogTarget, log_internal};
use crate::convo_manager::{convo_factory, ConvoManager};
use crate::payload;
use common_game::logging::Channel;

pub(super) fn dispatch(event: PlannedEvent, dispatch_ctx: &ConvoManager) {
    match event.kind {
        EventKind::Asteroid => dispatch_asteroid(event, dispatch_ctx),
        EventKind::Sunray => dispatch_sunray(event, dispatch_ctx),
    }
}

fn dispatch_asteroid(event: PlannedEvent, dispatch_ctx: &ConvoManager) {

    log_internal(
        LogTarget::AsteroidsSunrays,
        Channel::Trace,
        payload!(action: "Sending asteroid", planet_id: event.planet_id),
    );

    dispatch_ctx.create_asteroid_conversation(event.planet_id);
}

fn dispatch_sunray(event: PlannedEvent, dispatch_ctx: &ConvoManager) {

    log_internal(
        LogTarget::AsteroidsSunrays,
        Channel::Trace,
        payload!(action: "Sending sunray", planet_id: event.planet_id),
    );

    dispatch_ctx.create_sunray_conversation(event.planet_id);
}
