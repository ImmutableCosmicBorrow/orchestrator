//! Event emission and conversation creation.

use super::EventKind;
use super::state::PlannedEvent;
use crate::convo_manager::ConvoManager;
use crate::logging::{LogTarget, log_internal};
use crate::payload;
use common_game::logging::Channel;

pub(super) fn dispatch(event: PlannedEvent, dispatch_ctx: &ConvoManager) {
    match event.kind {
        EventKind::Asteroid => dispatch_asteroid(event, dispatch_ctx),
        EventKind::Sunray => dispatch_sunray(event, dispatch_ctx),
    }
}

fn dispatch_asteroid(event: PlannedEvent, dispatch_ctx: &ConvoManager) {
    if !dispatch_ctx
        .get_orch_context()
        .channels_manager
        .to_planet_senders_contains(event.planet_id)
    {
        log_internal(
            LogTarget::AsteroidsSunrays,
            Channel::Debug,
            payload!(
                action: "Skipping asteroid for missing planet sender",
                planet_id: event.planet_id,
            ),
        );
        return;
    }

    if !dispatch_ctx
        .get_orch_context()
        .channels_manager
        .to_planet_senders_contains(event.planet_id)
    {
        log_internal(
            LogTarget::AsteroidsSunrays,
            Channel::Debug,
            payload!(
                action: "Skipping asteroid for missing planet sender",
                planet_id: event.planet_id,
            ),
        );
        return;
    }

    log_internal(
        LogTarget::AsteroidsSunrays,
        Channel::Trace,
        payload!(action: "Sending asteroid", planet_id: event.planet_id),
    );

    dispatch_ctx.create_asteroid_conversation(event.planet_id);
}

fn dispatch_sunray(event: PlannedEvent, dispatch_ctx: &ConvoManager) {
    if !dispatch_ctx
        .get_orch_context()
        .channels_manager
        .to_planet_senders_contains(event.planet_id)
    {
        log_internal(
            LogTarget::AsteroidsSunrays,
            Channel::Debug,
            payload!(
                action: "Skipping sunray for missing planet sender",
                planet_id: event.planet_id,
            ),
        );
        return;
    }

    if !dispatch_ctx
        .get_orch_context()
        .channels_manager
        .to_planet_senders_contains(event.planet_id)
    {
        log_internal(
            LogTarget::AsteroidsSunrays,
            Channel::Debug,
            payload!(
                action: "Skipping sunray for missing planet sender",
                planet_id: event.planet_id,
            ),
        );
        return;
    }

    log_internal(
        LogTarget::AsteroidsSunrays,
        Channel::Trace,
        payload!(action: "Sending sunray", planet_id: event.planet_id),
    );

    dispatch_ctx.create_sunray_conversation(event.planet_id);
}
