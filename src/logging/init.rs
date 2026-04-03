use colored::Colorize;
use std::collections::HashMap;

use super::format::sanitize_log_message;
use super::routing::ContentRouter;
use super::targets::{
    TARGET_EXPLORER_LIFECYCLE, TARGET_ORCHESTRATOR_LIFECYCLE, TARGET_PLANET_LIFECYCLE,
};

// ── .env loader ──────────────────────────────────────────────────────────

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

// ── File helpers ─────────────────────────────────────────────────────────

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

// ── Format functions ─────────────────────────────────────────────────────

fn file_format(out: fern::FormatCallback, message: &std::fmt::Arguments, record: &log::Record) {
    let raw = format!("{message}");
    let cleaned = sanitize_log_message(&raw);
    out.finish(format_args!(
        "[{} {} {}] {}",
        humantime::format_rfc3339_seconds(std::time::SystemTime::now()),
        record.level(),
        record.target(),
        cleaned,
    ));
}

fn terminal_format(out: fern::FormatCallback, message: &std::fmt::Arguments, record: &log::Record) {
    let level_str = match record.level() {
        log::Level::Error => "ERROR".red().bold().to_string(),
        log::Level::Warn => "WARN".yellow().bold().to_string(),
        log::Level::Info => "INFO".green().to_string(),
        log::Level::Debug => "DEBUG".cyan().to_string(),
        log::Level::Trace => "TRACE".dimmed().to_string(),
    };
    let raw = format!("{message}");
    let cleaned = sanitize_log_message(&raw);
    out.finish(format_args!(
        "[{} {} {}] {}",
        humantime::format_rfc3339_seconds(std::time::SystemTime::now())
            .to_string()
            .dimmed(),
        level_str,
        record.target().dimmed(),
        cleaned,
    ));
}

// ── Sub-dispatch builders ────────────────────────────────────────────────

/// Build a file-backed sub-logger. Accepts every level — `ContentRouter` gates by level first.
fn build_file_log(file: std::fs::File) -> Box<dyn log::Log + Send + Sync> {
    let (_, log) = fern::Dispatch::new()
        .level(log::LevelFilter::Trace)
        .format(file_format)
        .chain(file)
        .into_log();
    log
}

fn build_terminal_log() -> Box<dyn log::Log + Send + Sync> {
    let (_, log) = fern::Dispatch::new()
        .level(log::LevelFilter::Trace)
        .format(terminal_format)
        .chain(std::io::stderr())
        .into_log();
    log
}

// ── Public entry point ───────────────────────────────────────────────────

/// Creates the `log/` directory tree and installs the multi-file logger.
///
/// Log routing (resolved by [`super::routing::ContentRouter`]):
/// | source / content                              | file                                    |
/// |-----------------------------------------------|-----------------------------------------|
/// | `orch::asteroids_sunrays`                     | `log/asteroids_sunrays/<ts>.log`        |
/// | `orch::conversations`                         | `log/conversations/<ts>.log`            |
/// | `orch::channel_messages` + routed messages    | `log/channel_messages/<ts>.log`         |
/// | planet crates + `InternalPlanetAction`        | `log/planet_lifecycle/<ts>.log`         |
/// | explorer crates + `InternalExplorerAction`    | `log/explorer_lifecycle/<ts>.log`       |
/// | orchestrator state changes + UI commands      | `log/orchestrator_lifecycle/<ts>.log`   |
/// | other external crates                         | `log/common_game/<ts>.log`              |
/// | remaining `orch::general` messages            | `log/general/<ts>.log`                  |
/// | *(all targets)*                               | `log/all/<ts>.log`                      |
///
/// All messages are also printed to **stderr** for terminal visibility.
/// The log level is controlled by `RUST_LOG` (default: `info`).
///
/// # Panics
/// Panics if any log directory or file cannot be created/opened.
pub(super) fn start_logger() {
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

    let f = |subdir: &str| open_log_file(&format!("{log_root}/{subdir}"), &log_filename);

    let router = ContentRouter {
        asteroids_sunrays: build_file_log(f("asteroids_sunrays")),
        conversations: build_file_log(f("conversations")),
        general: build_file_log(f("general")),
        channel_messages: build_file_log(f("channel_messages")),
        planet_lifecycle: build_file_log(f(TARGET_PLANET_LIFECYCLE.trim_start_matches("orch::"))),
        explorer_lifecycle: build_file_log(f(
            TARGET_EXPLORER_LIFECYCLE.trim_start_matches("orch::")
        )),
        orchestrator_lifecycle: build_file_log(f(
            TARGET_ORCHESTRATOR_LIFECYCLE.trim_start_matches("orch::")
        )),
        common_game: build_file_log(f("common_game")),
        shared: build_file_log(f("all")),
        terminal: build_terminal_log(),
        level,
    };

    log::set_boxed_logger(Box::new(router)).expect("failed to initialize logger");
    log::set_max_level(level);
}
