use colored::Colorize;
use std::collections::HashMap;

use super::format::sanitize_log_message;
use super::targets::{
    TARGET_ASTEROIDS_SUNRAYS, TARGET_CHANNEL_MESSAGES, TARGET_CONVERSATIONS, TARGET_GENERAL,
};

/// Load a local `.env` file (if present) and set the variables into the
/// process environment. This intentionally overrides any existing values so
/// that `.env` takes precedence over the terminal environment.
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
pub(super) fn start_logger() {
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
    let shared = fern::Dispatch::new()
        .format(format)
        .chain(open_log_file(&shared_dir, &log_filename));

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
