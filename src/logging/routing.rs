//! Content-based log routing.
//!
//! [`ContentRouter`] is the global `log::Log` implementation. For every record it:
//!   1. Classifies the message by target prefix and content into a [`MessageClass`].
//!   2. Forwards to the matching per-category sub-logger (each backed by a fern `Dispatch`).
//!   3. Always forwards to the shared "all" logger and the terminal logger.

use super::targets::{TARGET_ASTEROIDS_SUNRAYS, TARGET_CHANNEL_MESSAGES, TARGET_CONVERSATIONS};

// ── External crate prefixes ──────────────────────────────────────────────

/// Log target prefixes produced by known planet crates.
const PLANET_CRATE_PREFIXES: &[&str] = &[
    "luna4",
    "trip",
    "orbitron",
    "rustrelli",
    "enterprise",
    "planet", // houston-we-have-a-borrow (package = "Planet")
    "rusty_crab",
];

/// Log target prefixes produced by known explorer crates.
const EXPLORER_CRATE_PREFIXES: &[&str] = &[
    "explorer_nico",
    "explorer_jacopo",
    "explorer_rob",
    "common_explorer",
];

// ── Classification ───────────────────────────────────────────────────────

pub(super) enum MessageClass {
    AsteroidsSunrays,
    Conversations,
    ChannelMessages,
    PlanetLifecycle,
    ExplorerLifecycle,
    OrchestratorLifecycle,
    /// Fallback for external crates that don't match a known category
    CommonGame,
    /// Fallback for `orch::general` messages that don't match a specific category
    General,
}

/// Classify a log record by its target and formatted message.
pub(super) fn classify(target: &str, message: &str) -> MessageClass {
    if target.starts_with("orch::") {
        return classify_orch(target, message);
    }
    if target.starts_with("common_game") {
        return classify_common_game(message);
    }
    if PLANET_CRATE_PREFIXES.iter().any(|p| target.starts_with(p)) {
        return MessageClass::PlanetLifecycle;
    }
    if EXPLORER_CRATE_PREFIXES
        .iter()
        .any(|p| target.starts_with(p))
    {
        return MessageClass::ExplorerLifecycle;
    }
    MessageClass::CommonGame
}

fn classify_orch(target: &str, message: &str) -> MessageClass {
    if target.starts_with(TARGET_ASTEROIDS_SUNRAYS) {
        return MessageClass::AsteroidsSunrays;
    }
    if target.starts_with(TARGET_CONVERSATIONS) {
        return MessageClass::Conversations;
    }
    if target.starts_with(TARGET_CHANNEL_MESSAGES) {
        return MessageClass::ChannelMessages;
    }
    // orch::general and any other orch:: sub-target → inspect content
    classify_general_content(message)
}

/// Route `orch::general` messages to a more specific category where possible.
fn classify_general_content(message: &str) -> MessageClass {
    if message.contains("Created Planet")
        || message.contains("Planet creation failed")
        || message.contains("Planet encountered an error")
        || message.contains("Replacing dead PlanetNode")
        || message.contains("rwlock_poison_recovered")
        || message.contains("mutex_poison_recovered")
    {
        return MessageClass::PlanetLifecycle;
    }
    if message.contains("Created Explorer") || message.contains("Explorer thread ended") {
        return MessageClass::ExplorerLifecycle;
    }
    if message.contains("Orchestrator switched to")
        || message.contains("EndGame")
        || message.contains("PauseGame")
        || message.contains("ResumeGame")
        || message.contains("No explorers left")
        || message.contains("spawn_planet was Some, but Planet was not found")
    {
        return MessageClass::OrchestratorLifecycle;
    }
    if message.contains("does not start a conversation. Ignoring.") {
        return MessageClass::ChannelMessages;
    }
    MessageClass::General
}

/// Route `common_game::logging` messages by the event type embedded in the `LogEvent` string.
fn classify_common_game(message: &str) -> MessageClass {
    if message.contains("InternalPlanetAction") {
        return MessageClass::PlanetLifecycle;
    }
    if message.contains("InternalExplorerAction") {
        return MessageClass::ExplorerLifecycle;
    }
    if message.contains("InternalOrchestratorAction") {
        return MessageClass::OrchestratorLifecycle;
    }
    if message.contains("MessageOrchestratorToPlanet")
        || message.contains("MessagePlanetToOrchestrator")
        || message.contains("MessageExplorerToPlanet")
        || message.contains("MessageExplorerToOrchestrator")
    {
        return MessageClass::ChannelMessages;
    }
    MessageClass::CommonGame
}

// ── ContentRouter ────────────────────────────────────────────────────────

/// Global logger. Classifies every record and dispatches it to the right
/// per-category sub-logger, then unconditionally to `shared` and `terminal`.
pub(super) struct ContentRouter {
    // Per-category file loggers
    pub(super) asteroids_sunrays: Box<dyn log::Log + Send + Sync>,
    pub(super) conversations: Box<dyn log::Log + Send + Sync>,
    pub(super) general: Box<dyn log::Log + Send + Sync>,
    pub(super) channel_messages: Box<dyn log::Log + Send + Sync>,
    pub(super) planet_lifecycle: Box<dyn log::Log + Send + Sync>,
    pub(super) explorer_lifecycle: Box<dyn log::Log + Send + Sync>,
    pub(super) orchestrator_lifecycle: Box<dyn log::Log + Send + Sync>,
    pub(super) common_game: Box<dyn log::Log + Send + Sync>,
    // Always receives every record
    pub(super) shared: Box<dyn log::Log + Send + Sync>,
    pub(super) terminal: Box<dyn log::Log + Send + Sync>,
    pub(super) level: log::LevelFilter,
}

impl log::Log for ContentRouter {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let msg = record.args().to_string();
        let primary: &dyn log::Log = match classify(record.target(), &msg) {
            MessageClass::AsteroidsSunrays => &*self.asteroids_sunrays,
            MessageClass::Conversations => &*self.conversations,
            MessageClass::ChannelMessages => &*self.channel_messages,
            MessageClass::PlanetLifecycle => &*self.planet_lifecycle,
            MessageClass::ExplorerLifecycle => &*self.explorer_lifecycle,
            MessageClass::OrchestratorLifecycle => &*self.orchestrator_lifecycle,
            MessageClass::CommonGame => &*self.common_game,
            MessageClass::General => &*self.general,
        };
        primary.log(record);
        self.shared.log(record);
        self.terminal.log(record);
    }

    fn flush(&self) {
        self.asteroids_sunrays.flush();
        self.conversations.flush();
        self.general.flush();
        self.channel_messages.flush();
        self.planet_lifecycle.flush();
        self.explorer_lifecycle.flush();
        self.orchestrator_lifecycle.flush();
        self.common_game.flush();
        self.shared.flush();
        self.terminal.flush();
    }
}
