use common_game::logging::{Channel, LogEvent};

// ── Log targets (used by fern to route to different files) ──────────────

/// Target string for asteroid & sunray related events
pub(super) const TARGET_ASTEROIDS_SUNRAYS: &str = "orch::asteroids_sunrays";
/// Target string for conversation lifecycle events
pub(super) const TARGET_CONVERSATIONS: &str = "orch::conversations";
/// Target string for general orchestrator events
pub(super) const TARGET_GENERAL: &str = "orch::general";
/// Target string for raw channel messages (planet/explorer/UI ↔ orchestrator)
pub(super) const TARGET_CHANNEL_MESSAGES: &str = "orch::channel_messages";

/// Categories that determine which log file receives a message.
#[derive(Debug, Clone, Copy)]
pub enum LogTarget {
    /// Asteroid impacts and sunray events
    AsteroidsSunrays,
    /// Conversation state machine transitions, scheduling, queue operations
    Conversations,
    /// Galaxy setup, planet management, orchestrator lifecycle, and everything else
    General,
    /// Raw channel messages between orchestrator and planets/explorers/UI
    ChannelMessages,
}

impl LogTarget {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::AsteroidsSunrays => TARGET_ASTEROIDS_SUNRAYS,
            Self::Conversations => TARGET_CONVERSATIONS,
            Self::General => TARGET_GENERAL,
            Self::ChannelMessages => TARGET_CHANNEL_MESSAGES,
        }
    }
}

// ── Emit helper ─────────────────────────────────────────────────────────

pub(super) fn emit_with_target(event: &LogEvent, target: &str) {
    let level = match event.channel {
        Channel::Error => log::Level::Error,
        Channel::Warning => log::Level::Warn,
        Channel::Info => log::Level::Info,
        Channel::Debug => log::Level::Debug,
        Channel::Trace => log::Level::Trace,
    };
    let msg = super::format::format_log_event_from_string(format!("{event}"));
    log::log!(target: target, level, "{msg}");
}
