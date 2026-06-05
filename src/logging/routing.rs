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
    // asteroids & sunrays
    AsteroidsSunrays,
    Asteroids,
    Sunrays,
    // conversations
    Conversations,
    ConversationsPlanets,
    ConversationsExplorers,
    // channel messages
    ChannelMessages,
    ChannelMessagesPlanets,
    ChannelMessagesExplorers,
    ChannelMessagesUi,
    // lifecycle & other
    PlanetLifecycle,
    ExplorerLifecycle,
    OrchestratorLifecycle,
    /// Fallback for external crates that don't match a known category
    CommonGame,
    /// Fallback for `orch::general` messages that don't match a specific category
    General,
}

impl MessageClass {
    /// Returns the category name usable as a `RUST_LOG` directive key.
    ///
    /// These match the log subfolder names, so `explorer_lifecycle=debug`
    /// in `RUST_LOG` will enable debug logging for all records routed to
    /// the `log/explorer_lifecycle/` directory.
    pub(super) fn directive_key(&self) -> &'static str {
        match self {
            Self::AsteroidsSunrays => "asteroids_sunrays",
            Self::Asteroids => "asteroids",
            Self::Sunrays => "sunrays",
            Self::Conversations => "conversations",
            Self::ConversationsPlanets => "conversations_planets",
            Self::ConversationsExplorers => "conversations_explorers",
            Self::ChannelMessages => "channel_messages",
            Self::ChannelMessagesPlanets => "channel_messages_planets",
            Self::ChannelMessagesExplorers => "channel_messages_explorers",
            Self::ChannelMessagesUi => "channel_messages_ui",
            Self::PlanetLifecycle => "planet_lifecycle",
            Self::ExplorerLifecycle => "explorer_lifecycle",
            Self::OrchestratorLifecycle => "orchestrator_lifecycle",
            Self::CommonGame => "common_game",
            Self::General => "general",
        }
    }
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
        return classify_asteroids_sunrays(message);
    }
    if target.starts_with(TARGET_CONVERSATIONS) {
        return classify_conversations(message);
    }
    if target.starts_with(TARGET_CHANNEL_MESSAGES) {
        return classify_channel_messages(message);
    }
    // orch::general and any unrecognised orch:: sub-target → inspect content
    classify_general_content(message)
}

// ── asteroids / sunrays ──────────────────────────────────────────────────

fn classify_asteroids_sunrays(message: &str) -> MessageClass {
    let lower = message.to_lowercase();
    if lower.contains("asteroid") {
        MessageClass::Asteroids
    } else if lower.contains("sunray") {
        MessageClass::Sunrays
    } else {
        MessageClass::AsteroidsSunrays
    }
}

// ── conversations ────────────────────────────────────────────────────────

fn classify_conversations(message: &str) -> MessageClass {
    // Event-type markers embedded in LogEvent strings (from log_msg_to calls)
    if message.contains("MessageOrchestratorToPlanet") {
        return MessageClass::ConversationsPlanets;
    }
    if message.contains("MessageOrchestratorToExplorer") {
        return MessageClass::ConversationsExplorers;
    }
    // Action keyword matching for direct log_internal calls
    if message.contains("Killed Planet")
        || message.contains("Started Planet")
        || message.contains("Stopped Planet")
        || message.contains("Planet sent its internal state")
        || message.contains("Planet correctly handled dead explorer")
        || message.contains("Planet AI")
    {
        return MessageClass::ConversationsPlanets;
    }
    if message.contains("Explorer correctly")
        || message.contains("Explorer cannot")
        || message.contains("Changed Explorer location")
        || message.contains("neighbors to Explorer")
        || message.contains("Explorer location")
        || message.contains("Explorer AI")
    {
        return MessageClass::ConversationsExplorers;
    }
    // Queue/infra messages (QueueEnqueue, QueueDequeue, MessageParked,
    // "Message matched conversation", "Conversation Transition", on_timeout errors…)
    MessageClass::Conversations
}

// ── channel messages ─────────────────────────────────────────────────────

fn classify_channel_messages(message: &str) -> MessageClass {
    // Event-type markers embedded in LogEvent strings
    if message.contains("MessagePlanetToOrchestrator") || message.contains("from_planet:") {
        return MessageClass::ChannelMessagesPlanets;
    }
    if message.contains("MessageExplorerToOrchestrator") || message.contains("from_explorer:") {
        return MessageClass::ChannelMessagesExplorers;
    }
    if message.contains("UI->ORCH") {
        return MessageClass::ChannelMessagesUi;
    }
    MessageClass::ChannelMessages
}

// ── orch::general content-based routing ─────────────────────────────────

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
        return classify_channel_messages(message);
    }
    MessageClass::General
}

// ── common_game::logging content-based routing ───────────────────────────

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
    {
        return MessageClass::ChannelMessagesPlanets;
    }
    if message.contains("MessageExplorerToPlanet")
        || message.contains("MessageExplorerToOrchestrator")
    {
        return MessageClass::ChannelMessagesExplorers;
    }
    MessageClass::CommonGame
}

// ── ContentRouter ────────────────────────────────────────────────────────

/// Global logger. Classifies every record and dispatches it to the right
/// per-category sub-logger, then unconditionally to `shared` and `terminal`.
pub(super) struct ContentRouter {
    // asteroids & sunrays
    pub(super) asteroids_sunrays: Box<dyn log::Log + Send + Sync>,
    pub(super) asteroids: Box<dyn log::Log + Send + Sync>,
    pub(super) sunrays: Box<dyn log::Log + Send + Sync>,
    // conversations
    pub(super) conversations: Box<dyn log::Log + Send + Sync>,
    pub(super) conversations_planets: Box<dyn log::Log + Send + Sync>,
    pub(super) conversations_explorers: Box<dyn log::Log + Send + Sync>,
    // channel messages
    pub(super) channel_messages: Box<dyn log::Log + Send + Sync>,
    pub(super) channel_messages_planets: Box<dyn log::Log + Send + Sync>,
    pub(super) channel_messages_explorers: Box<dyn log::Log + Send + Sync>,
    pub(super) channel_messages_ui: Box<dyn log::Log + Send + Sync>,
    // lifecycle & other
    pub(super) general: Box<dyn log::Log + Send + Sync>,
    pub(super) planet_lifecycle: Box<dyn log::Log + Send + Sync>,
    pub(super) explorer_lifecycle: Box<dyn log::Log + Send + Sync>,
    pub(super) orchestrator_lifecycle: Box<dyn log::Log + Send + Sync>,
    pub(super) common_game: Box<dyn log::Log + Send + Sync>,
    // always receives every record
    pub(super) shared: Box<dyn log::Log + Send + Sync>,
    pub(super) terminal: Option<Box<dyn log::Log + Send + Sync>>,
    pub(super) directives: super::init::LogDirectives,
}

impl log::Log for ContentRouter {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        // Permissive: allow if the record *might* pass after classification.
        // The real per-category gate is in `log()` below.
        self.directives
            .is_enabled(metadata.level(), metadata.target())
            || self.directives.any_module_enables(metadata.level())
    }

    fn log(&self, record: &log::Record) {
        // Classify first, then filter — this lets category-based directives
        // (e.g., `explorer_lifecycle=debug`) work for records whose
        // `target()` is a different module (e.g., `common_game::logging`).
        let msg = record.args().to_string();
        let class = classify(record.target(), &msg);

        let level = record.level();
        let target = record.target();
        let category = class.directive_key();

        if !self.directives.is_enabled(level, target)
            && !self.directives.is_enabled(level, category)
        {
            return;
        }

        let primary: &dyn log::Log = match class {
            MessageClass::AsteroidsSunrays => &*self.asteroids_sunrays,
            MessageClass::Asteroids => &*self.asteroids,
            MessageClass::Sunrays => &*self.sunrays,
            MessageClass::Conversations => &*self.conversations,
            MessageClass::ConversationsPlanets => &*self.conversations_planets,
            MessageClass::ConversationsExplorers => &*self.conversations_explorers,
            MessageClass::ChannelMessages => &*self.channel_messages,
            MessageClass::ChannelMessagesPlanets => &*self.channel_messages_planets,
            MessageClass::ChannelMessagesExplorers => &*self.channel_messages_explorers,
            MessageClass::ChannelMessagesUi => &*self.channel_messages_ui,
            MessageClass::PlanetLifecycle => &*self.planet_lifecycle,
            MessageClass::ExplorerLifecycle => &*self.explorer_lifecycle,
            MessageClass::OrchestratorLifecycle => &*self.orchestrator_lifecycle,
            MessageClass::CommonGame => &*self.common_game,
            MessageClass::General => &*self.general,
        };
        primary.log(record);
        self.shared.log(record);
        if let Some(terminal) = &self.terminal {
            terminal.log(record);
        }
    }

    fn flush(&self) {
        self.asteroids_sunrays.flush();
        self.asteroids.flush();
        self.sunrays.flush();
        self.conversations.flush();
        self.conversations_planets.flush();
        self.conversations_explorers.flush();
        self.channel_messages.flush();
        self.channel_messages_planets.flush();
        self.channel_messages_explorers.flush();
        self.channel_messages_ui.flush();
        self.general.flush();
        self.planet_lifecycle.flush();
        self.explorer_lifecycle.flush();
        self.orchestrator_lifecycle.flush();
        self.common_game.flush();
        self.shared.flush();
        if let Some(terminal) = &self.terminal {
            terminal.flush();
        }
    }
}
