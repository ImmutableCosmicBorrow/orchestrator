//! Logging utilities: public API and module wiring.

mod format;
mod init;
mod targets;

pub use targets::LogTarget;

use common_game::logging::{ActorType, Channel, EventType, LogEvent, Participant, Payload};
use common_game::utils::ID;

#[macro_export]
macro_rules! payload {
    ($($key:ident : $val:expr),* $(,)?) => {{
        let mut p = common_game::logging::Payload::new();
        $(
            p.insert(stringify!($key).to_string(), format!{"{}", $val});
        )*
        p
    }};
}

/// Creates and emits a log event with `ActorType::Orchestrator` as sender
pub fn log_msg_to(
    target: LogTarget,
    channel: Channel,
    event_type: EventType,
    to: (ActorType, ID),
    payload: Payload,
) {
    let event = LogEvent::new(
        Some(Participant::new(ActorType::Orchestrator, 0u8)),
        Some(Participant::new(to.0, to.1)),
        event_type,
        channel,
        payload,
    );
    targets::emit_with_target(&event, target.as_str());
}

/// Creates and emits a log event with `ActorType::Orchestrator` as receiver
pub fn log_msg_from(
    target: LogTarget,
    channel: Channel,
    event_type: EventType,
    from: (ActorType, ID),
    payload: Payload,
) {
    let event = LogEvent::new(
        Some(Participant::new(from.0, from.1)),
        Some(Participant::new(ActorType::Orchestrator, 0u8)),
        event_type,
        channel,
        payload,
    );
    targets::emit_with_target(&event, target.as_str());
}

/// Creates and emits a log event without sender and receiver, and with `EventType::InternalOrchestratorAction`
pub fn log_internal(target: LogTarget, channel: Channel, payload: Payload) {
    let event = LogEvent::system(EventType::InternalOrchestratorAction, channel, payload);
    targets::emit_with_target(&event, target.as_str());
}

/// Creates the `log/` directory and starts the multi-file logger.
pub fn start_logger() {
    init::start_logger();
}
