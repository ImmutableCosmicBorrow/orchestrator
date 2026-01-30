use common_game::logging::{ActorType, Channel, EventType, LogEvent, Participant, Payload};
use common_game::utils::ID;
use env_logger::{Builder, Env};

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
pub fn log_msg_to(channel: Channel, event_type: EventType, to: (ActorType, ID), payload: Payload) {
    LogEvent::new(
        Some(Participant::new(ActorType::Orchestrator, 0u8)),
        Some(Participant::new(to.0, to.1)),
        event_type,
        channel,
        payload,
    )
    .emit();
}

/// Creates and emits a log event with `ActorType::Orchestrator` as receiver
pub fn log_msg_from(
    channel: Channel,
    event_type: EventType,
    from: (ActorType, ID),
    payload: Payload,
) {
    LogEvent::new(
        Some(Participant::new(from.0, from.1)),
        Some(Participant::new(ActorType::Orchestrator, 0u8)),
        event_type,
        channel,
        payload,
    )
    .emit();
}

/// Creates and emits a log event without sender and receiver, and with `EventType::InternalOrchestratorAction`
pub fn log_internal(channel: Channel, payload: Payload) {
    LogEvent::system(EventType::InternalOrchestratorAction, channel, payload).emit();
}

/// Creates and starts the logger. Uses the `RUST_LOG` environmental variable, or `default_level` if not found. Returns the logger handle
pub fn start_logger() {
    let env = Env::default().filter_or("RUST_LOG", "info");
    Builder::from_env(env).init();
}
