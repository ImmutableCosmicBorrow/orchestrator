use common_game::logging::{Channel, LogEvent};

// ── Log targets (used by fern to route to different files) ──────────────

// ── asteroids & sunrays ──────────────────────────────────────────────────
/// Combined fallback when the event kind cannot be determined
pub(super) const TARGET_ASTEROIDS_SUNRAYS: &str = "orch::asteroids_sunrays";
pub(super) const TARGET_ASTEROIDS: &str = "orch::asteroids";
pub(super) const TARGET_SUNRAYS: &str = "orch::sunrays";

// ── conversations ────────────────────────────────────────────────────────
/// Combined fallback (queue ops, transitions, unclassified)
pub(super) const TARGET_CONVERSATIONS: &str = "orch::conversations";
pub(super) const TARGET_CONVERSATIONS_PLANETS: &str = "orch::conversations::planets";
pub(super) const TARGET_CONVERSATIONS_EXPLORERS: &str = "orch::conversations::explorers";

// ── channel messages ─────────────────────────────────────────────────────
/// Combined fallback
pub(super) const TARGET_CHANNEL_MESSAGES: &str = "orch::channel_messages";
pub(super) const TARGET_CHANNEL_MESSAGES_PLANETS: &str = "orch::channel_messages::planets";
pub(super) const TARGET_CHANNEL_MESSAGES_EXPLORERS: &str = "orch::channel_messages::explorers";
pub(super) const TARGET_CHANNEL_MESSAGES_UI: &str = "orch::channel_messages::ui";

// ── other ────────────────────────────────────────────────────────────────
/// Target string for general orchestrator events
pub(super) const TARGET_GENERAL: &str = "orch::general";
/// Target string for planet lifecycle events (creation, errors, node replacement)
pub(super) const TARGET_PLANET_LIFECYCLE: &str = "orch::planet_lifecycle";
/// Target string for explorer lifecycle events (spawn, thread end)
pub(super) const TARGET_EXPLORER_LIFECYCLE: &str = "orch::explorer_lifecycle";
/// Target string for orchestrator state changes (mode switch, pause/resume, shutdown)
pub(super) const TARGET_ORCHESTRATOR_LIFECYCLE: &str = "orch::orchestrator_lifecycle";

/// Categories that determine which log file receives a message.
#[derive(Debug, Clone, Copy)]
pub enum LogTarget {
    // ── asteroids & sunrays ──────────────────────────────────────────────
    /// Fallback when the event kind cannot be inferred from content
    AsteroidsSunrays,
    Asteroids,
    Sunrays,
    // ── conversations ────────────────────────────────────────────────────
    /// Fallback for queue/scheduler infrastructure messages
    Conversations,
    ConversationsPlanets,
    ConversationsExplorers,
    // ── channel messages ─────────────────────────────────────────────────
    /// Fallback when the message direction cannot be inferred
    ChannelMessages,
    ChannelMessagesPlanets,
    ChannelMessagesExplorers,
    ChannelMessagesUi,
    // ── lifecycle & other ────────────────────────────────────────────────
    General,
    PlanetLifecycle,
    ExplorerLifecycle,
    OrchestratorLifecycle,
}

impl LogTarget {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::AsteroidsSunrays => TARGET_ASTEROIDS_SUNRAYS,
            Self::Asteroids => TARGET_ASTEROIDS,
            Self::Sunrays => TARGET_SUNRAYS,
            Self::Conversations => TARGET_CONVERSATIONS,
            Self::ConversationsPlanets => TARGET_CONVERSATIONS_PLANETS,
            Self::ConversationsExplorers => TARGET_CONVERSATIONS_EXPLORERS,
            Self::ChannelMessages => TARGET_CHANNEL_MESSAGES,
            Self::ChannelMessagesPlanets => TARGET_CHANNEL_MESSAGES_PLANETS,
            Self::ChannelMessagesExplorers => TARGET_CHANNEL_MESSAGES_EXPLORERS,
            Self::ChannelMessagesUi => TARGET_CHANNEL_MESSAGES_UI,
            Self::General => TARGET_GENERAL,
            Self::PlanetLifecycle => TARGET_PLANET_LIFECYCLE,
            Self::ExplorerLifecycle => TARGET_EXPLORER_LIFECYCLE,
            Self::OrchestratorLifecycle => TARGET_ORCHESTRATOR_LIFECYCLE,
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
