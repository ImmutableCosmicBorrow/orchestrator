use common_game::logging::{ActorType, Channel, EventType, LogEvent, Participant, Payload};
use common_game::utils::ID;
use colored::Colorize;

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

// ── Log targets (used by fern to route to different files) ──────────────

/// Target string for asteroid & sunray related events
const TARGET_ASTEROIDS_SUNRAYS: &str = "orch::asteroids_sunrays";
/// Target string for conversation lifecycle events
const TARGET_CONVERSATIONS: &str = "orch::conversations";
/// Target string for general orchestrator events
const TARGET_GENERAL: &str = "orch::general";
/// Target string for raw channel messages (planet/explorer/UI ↔ orchestrator)
const TARGET_CHANNEL_MESSAGES: &str = "orch::channel_messages";

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
    const fn as_str(self) -> &'static str {
        match self {
            Self::AsteroidsSunrays => TARGET_ASTEROIDS_SUNRAYS,
            Self::Conversations => TARGET_CONVERSATIONS,
            Self::General => TARGET_GENERAL,
            Self::ChannelMessages => TARGET_CHANNEL_MESSAGES,
        }
    }
}

// ── Emit helper (replaces LogEvent::emit, routes via log target) ────────

fn emit_with_target(event: &LogEvent, target: &str) {
    let level = match event.channel {
        Channel::Error => log::Level::Error,
        Channel::Warning => log::Level::Warn,
        Channel::Info => log::Level::Info,
        Channel::Debug => log::Level::Debug,
        Channel::Trace => log::Level::Trace,
    };
    log::log!(target: target, level, "{event}");
}

// ── Public logging helpers ──────────────────────────────────────────────

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
    emit_with_target(&event, target.as_str());
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
    emit_with_target(&event, target.as_str());
}

/// Creates and emits a log event without sender and receiver, and with `EventType::InternalOrchestratorAction`
pub fn log_internal(target: LogTarget, channel: Channel, payload: Payload) {
    let event = LogEvent::system(EventType::InternalOrchestratorAction, channel, payload);
    emit_with_target(&event, target.as_str());
}

// ── Logger initialisation ───────────────────────────────────────────────

fn open_log_file(path: &str) -> std::fs::File {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap_or_else(|e| panic!("failed to open log file {path}: {e}"))
}

/// Creates the `log/` directory and starts the multi-file logger.
///
/// Log routing:
/// | target prefix             | file                          |
/// |---------------------------|-------------------------------|
/// | `orch::asteroids_sunrays` | `log/asteroids_sunrays.log`   |
/// | `orch::conversations`     | `log/conversations.log`       |
/// | `orch::general`           | `log/general.log`             |
/// | `orch::channel_messages`  | `log/channel_messages.log`    |
/// | anything else             | `log/common_game.log`         |
///
/// All messages are also printed to **stderr** for terminal visibility.
///
/// The log level is controlled by the `RUST_LOG` env var (default: `info`).
///
/// # Panics
/// Panics if the log directory or any log file cannot be created/opened.
pub fn start_logger() {
    std::fs::create_dir_all("log").expect("failed to create log/ directory");

    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|s| s.parse::<log::LevelFilter>().ok())
        .unwrap_or(log::LevelFilter::Info);

    let format =
        |out: fern::FormatCallback, message: &std::fmt::Arguments, record: &log::Record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                humantime::format_rfc3339_seconds(std::time::SystemTime::now()),
                record.level(),
                record.target(),
                message,
            ));
        };

    let asteroids_sunrays = fern::Dispatch::new()
        .format(format)
        .filter(|meta| meta.target().starts_with(TARGET_ASTEROIDS_SUNRAYS))
        .chain(open_log_file("log/asteroids_sunrays.log"));

    let conversations = fern::Dispatch::new()
        .format(format)
        .filter(|meta| meta.target().starts_with(TARGET_CONVERSATIONS))
        .chain(open_log_file("log/conversations.log"));

    let general = fern::Dispatch::new()
        .format(format)
        .filter(|meta| meta.target().starts_with(TARGET_GENERAL))
        .chain(open_log_file("log/general.log"));

    let channel_messages = fern::Dispatch::new()
        .format(format)
        .filter(|meta| meta.target().starts_with(TARGET_CHANNEL_MESSAGES))
        .chain(open_log_file("log/channel_messages.log"));

    // Everything that does NOT come from orchestrator targets (e.g. common_game, planet crates, explorer crates)
    let common_game = fern::Dispatch::new()
        .format(format)
        .filter(|meta| !meta.target().starts_with("orch::"))
        .chain(open_log_file("log/common_game.log"));

    // Terminal output: all orchestrator logs go to stderr as well, with color
    let terminal = fern::Dispatch::new()
        .format(|out, message, record| {
            let level = record.level();
            let level_str = match level {
                log::Level::Error => "ERROR".red().bold().to_string(),
                log::Level::Warn => "WARN".yellow().bold().to_string(),
                log::Level::Info => "INFO".green().to_string(),
                log::Level::Debug => "DEBUG".cyan().to_string(),
                log::Level::Trace => "TRACE".dimmed().to_string(),
            };
            let target = record.target().dimmed();
            let timestamp = humantime::format_rfc3339_seconds(std::time::SystemTime::now());
            out.finish(format_args!(
                "[{} {} {}] {}",
                timestamp.to_string().dimmed(),
                level_str,
                target,
                message,
            ));
        })
        .chain(std::io::stderr());

    fern::Dispatch::new()
        .level(level)
        .chain(asteroids_sunrays)
        .chain(conversations)
        .chain(general)
        .chain(channel_messages)
        .chain(common_game)
        .chain(terminal)
        .apply()
        .expect("failed to initialize logger");
}
