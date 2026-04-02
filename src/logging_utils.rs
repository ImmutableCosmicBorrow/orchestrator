use colored::Colorize;
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
    let msg = format_log_event_from_string(format!("{event}"));
    log::log!(target: target, level, "{msg}");
}

/// Given a formatted `LogEvent` string, clean it up by removing internal
/// timestamp, and `sender`/`receiver` fields when they are `none`.
fn format_log_event_from_string(mut msg: String) -> String {
    // Remove internal timestamp fields if present
    if let Some(start) = msg.find("ts: ").or_else(|| msg.find("timestamp_unix: ")) {
        let rel_comma = msg[start..].find(',');
        let rel_brace = msg[start..].find('}');
        let rel_end = match (rel_comma, rel_brace) {
            (Some(c), Some(b)) => Some(std::cmp::min(c, b)),
            (Some(c), None) => Some(c),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        if let Some(rel) = rel_end {
            let mut end = start + rel + 1; // include comma or brace
            if msg.as_bytes().get(end) == Some(&b' ') {
                end += 1;
            }
            msg.replace_range(start..end, "");
        }
    }

    // Remove `sender: none` / `receiver: none` patterns
    let patterns_with_comma = [
        "sender: none, ",
        "receiver: none, ",
        "sender: None, ",
        "receiver: None, ",
    ];
    for pat in &patterns_with_comma {
        while let Some(pos) = msg.find(pat) {
            msg.replace_range(pos..pos + pat.len(), "");
        }
    }
    let patterns = ["sender: none", "receiver: none", "sender: None", "receiver: None"];
    for pat in &patterns {
        while let Some(pos) = msg.find(pat) {
            let mut start = pos;
            if start >= 2 && &msg[start - 2..start] == ", " {
                start -= 2;
            } else if start >= 1 && &msg[start - 1..start] == "," {
                start -= 1;
            }
            let mut end = pos + pat.len();
            if msg.as_bytes().get(end) == Some(&b' ') {
                end += 1;
            }
            msg.replace_range(start..end, "");
        }
    }

    // Cleanup: remove a trailing comma before a closing brace and collapse double spaces
    msg = msg.replace(", }", " }");
    while msg.contains("  ") {
        msg = msg.replacen("  ", " ", 1);
    }

    msg
}

/// If a message contains a `LogEvent { ... }` payload, normalize that payload
/// by stripping duplicated timestamp fields and empty sender/receiver fields.
fn sanitize_log_message(message: &str) -> String {
    if let Some(start) = message.find("LogEvent {") {
        let (prefix, event_part) = message.split_at(start);
        let cleaned_event = format_log_event_from_string(event_part.to_string());
        format!("{prefix}{cleaned_event}")
    } else {
        message.to_string()
    }
}

/// Load a local `.env` file (if present) and set the variables into the
/// process environment. This intentionally overrides any existing values so
/// that `.env` takes precedence over the terminal environment.
use std::collections::HashMap;

fn read_local_dotenv() -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(content) = std::fs::read_to_string(".env") {
        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(2, '=');
            let key = parts.next().unwrap().trim();
            if key.is_empty() {
                continue;
            }
            let mut value = parts.next().unwrap_or("").trim().to_string();
            if value.len() >= 2 {
                let first = value.chars().next().unwrap();
                let last = value.chars().last().unwrap();
                if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
                    value = value[1..value.len() - 1].to_string();
                }
            }
            map.insert(key.to_string(), value);
        }
    }
    map
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

fn open_log_file(dir: &str, filename: &str) -> std::fs::File {
    std::fs::create_dir_all(dir)
        .unwrap_or_else(|e| panic!("failed to create log directory {dir}: {e}"));
    let path = format!("{dir}/{filename}");
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap_or_else(|e| panic!("failed to open log file {path}: {e}"))
}

/// Creates the `log/` directory and starts the multi-file logger.
///
/// Log routing:
/// | target prefix             | file                          |
/// |---------------------------|-------------------------------|
/// | `orch::asteroids_sunrays` | `log/asteroids_sunrays/<timestamp>.log` |
/// | `orch::conversations`     | `log/conversations/<timestamp>.log`     |
/// | `orch::general`           | `log/general/<timestamp>.log`           |
/// | `orch::channel_messages`  | `log/channel_messages/<timestamp>.log`  |
/// | anything else             | `log/common_game/<timestamp>.log`       |
/// | *(all targets)*           | `log/all/<timestamp>.log`               |
///
/// All messages are also printed to **stderr** for terminal visibility.
///
/// The log level is controlled by the `RUST_LOG` env var (default: `info`).
///
/// # Panics
/// Panics if the log directory or any log file cannot be created/opened.
pub fn start_logger() {
    // Read local `.env` (if present); terminal environment variables take
    // precedence. Use `.env` only as a fallback for `RUST_LOG` and `LOG_DIR`.
    let dotenv_map = read_local_dotenv();

    let log_root = std::env::var("LOG_DIR")
        .ok()
        .or_else(|| dotenv_map.get("LOG_DIR").cloned())
        .unwrap_or_else(|| "log".to_string());
    std::fs::create_dir_all(&log_root).expect("failed to create log/ directory");

    let now = chrono::Local::now();
    let log_filename = format!("{}.log", now.format("%Y_%m_%d_%H-%M-%S"));

    let level = std::env::var("RUST_LOG")
        .ok()
        .or_else(|| dotenv_map.get("RUST_LOG").cloned())
        .and_then(|s| s.parse::<log::LevelFilter>().ok())
        .unwrap_or(log::LevelFilter::Info);

    let format =
        |out: fern::FormatCallback, message: &std::fmt::Arguments, record: &log::Record| {
            let raw_message = format!("{message}");
            let cleaned_message = sanitize_log_message(&raw_message);
            out.finish(format_args!(
                "[{} {} {}] {}",
                humantime::format_rfc3339_seconds(std::time::SystemTime::now()),
                record.level(),
                record.target(),
                cleaned_message,
            ));
        };

    let asteroids_sunrays_dir = format!("{log_root}/asteroids_sunrays");
    let conversations_dir = format!("{log_root}/conversations");
    let general_dir = format!("{log_root}/general");
    let channel_messages_dir = format!("{log_root}/channel_messages");
    let common_game_dir = format!("{log_root}/common_game");
    let shared_dir = format!("{log_root}/all");

    let asteroids_sunrays = fern::Dispatch::new()
        .format(format)
        .filter(|meta| meta.target().starts_with(TARGET_ASTEROIDS_SUNRAYS))
        .chain(open_log_file(&asteroids_sunrays_dir, &log_filename));

    let conversations = fern::Dispatch::new()
        .format(format)
        .filter(|meta| meta.target().starts_with(TARGET_CONVERSATIONS))
        .chain(open_log_file(&conversations_dir, &log_filename));

    let general = fern::Dispatch::new()
        .format(format)
        .filter(|meta| meta.target().starts_with(TARGET_GENERAL))
        .chain(open_log_file(&general_dir, &log_filename));

    let channel_messages = fern::Dispatch::new()
        .format(format)
        .filter(|meta| meta.target().starts_with(TARGET_CHANNEL_MESSAGES))
        .chain(open_log_file(&channel_messages_dir, &log_filename));

    // Everything that does NOT come from orchestrator targets (e.g. common_game, planet crates, explorer crates)
    let common_game = fern::Dispatch::new()
        .format(format)
        .filter(|meta| !meta.target().starts_with("orch::"))
        .chain(open_log_file(&common_game_dir, &log_filename));

    // Shared file: all messages regardless of target
    let shared = fern::Dispatch::new().format(format).chain(open_log_file(&shared_dir, &log_filename));

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
            let raw_message = format!("{message}");
            let cleaned_message = sanitize_log_message(&raw_message);
            out.finish(format_args!(
                "[{} {} {}] {}",
                timestamp.to_string().dimmed(),
                level_str,
                target,
                cleaned_message,
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
        .chain(shared)
        .chain(terminal)
        .apply()
        .expect("failed to initialize logger");
}
