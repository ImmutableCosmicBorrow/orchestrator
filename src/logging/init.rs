use colored::Colorize;
use std::collections::HashMap;

use super::format::sanitize_log_message;
use super::routing::ContentRouter;
use super::targets::{
    TARGET_EXPLORER_LIFECYCLE, TARGET_ORCHESTRATOR_LIFECYCLE, TARGET_PLANET_LIFECYCLE,
};
// (TARGET_* constants used only for directory naming via trim_start_matches("orch::"))

// ── RUST_LOG directive parsing ───────────────────────────────────────────

/// Represents parsed `RUST_LOG` directives for module-level filtering.
/// Example: "`explorer_rob`=trace,`orchestrator`=debug,info"
/// Parsed as: {
///   "`explorer_rob`" -> Trace,
///   "`orchestrator`" -> Debug,
/// }
/// with global fallback: Info
#[derive(Debug, Clone)]
pub struct LogDirectives {
    /// Maps module prefixes to their log levels (e.g., "`explorer_rob`" -> Trace)
    pub module_levels: HashMap<String, log::LevelFilter>,
    /// Global fallback level if no module matches
    pub global_level: log::LevelFilter,
}

impl LogDirectives {
    /// Parse `RUST_LOG` directive string into module levels and global fallback.
    /// Examples:
    ///   "debug" -> global Debug
    ///   "`explorer_rob`=trace" -> module-specific Trace, global Info
    ///   "`explorer_rob`=trace,debug" -> module-specific Trace, global Debug
    ///   "`orchestrator`=debug,`explorer_rob`=trace,info" -> multiple modules, global Info
    pub fn parse(input: &str) -> Self {
        let mut module_levels = HashMap::new();
        let mut global_level = log::LevelFilter::Info;

        for part in input.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if let Some((module, level_str)) = part.split_once('=') {
                // Module-specific directive: "module=level"
                if let Ok(level) = level_str.trim().parse::<log::LevelFilter>() {
                    module_levels.insert(module.trim().to_string(), level);
                }
            } else {
                // Global level directive (no '=')
                if let Ok(level) = part.parse::<log::LevelFilter>() {
                    global_level = level;
                }
            }
        }

        LogDirectives {
            module_levels,
            global_level,
        }
    }

    /// Check if a record should be logged based on its target (module prefix).
    pub fn is_enabled(&self, level: log::Level, target: &str) -> bool {
        // Check for module-specific overrides, trying progressively shorter prefixes
        let parts: Vec<&str> = target.split("::").collect();
        for i in (0..parts.len()).rev() {
            let prefix = parts[0..=i].join("::");
            if let Some(&level_filter) = self.module_levels.get(&prefix) {
                return level <= level_filter;
            }
        }

        // Fall back to global level
        level <= self.global_level
    }

    /// Returns `true` if *any* module-level directive would accept `level`.
    ///
    /// Used by [`ContentRouter::enabled`] as a permissive pre-filter: the
    /// `log` framework calls `enabled()` before `log()`, and we need to let
    /// records through that might match a category directive discovered only
    /// after content-based classification.
    pub fn any_module_enables(&self, level: log::Level) -> bool {
        self.module_levels
            .values()
            .any(|&filter| level <= filter)
    }
}

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
/// | source / content                              | file                                            |
/// |-----------------------------------------------|-------------------------------------------------|
/// | asteroid events                               | `log/asteroids_sunrays/asteroids/<ts>.log`      |
/// | sunray events                                 | `log/asteroids_sunrays/sunrays/<ts>.log`        |
/// | unclassified asteroids/sunrays                | `log/asteroids_sunrays/<ts>.log`                |
/// | planet conversation outcomes                  | `log/conversations/planets/<ts>.log`            |
/// | explorer conversation outcomes                | `log/conversations/explorers/<ts>.log`          |
/// | queue/scheduler infrastructure                | `log/conversations/<ts>.log`                    |
/// | planet→orch channel messages                  | `log/channel_messages/planets/<ts>.log`         |
/// | explorer→orch channel messages                | `log/channel_messages/explorers/<ts>.log`       |
/// | UI→orch channel messages                      | `log/channel_messages/ui/<ts>.log`              |
/// | unclassified channel messages                 | `log/channel_messages/<ts>.log`                 |
/// | planet crates + `InternalPlanetAction`        | `log/planet_lifecycle/<ts>.log`                 |
/// | explorer crates + `InternalExplorerAction`    | `log/explorer_lifecycle/<ts>.log`               |
/// | orchestrator state changes + UI commands      | `log/orchestrator_lifecycle/<ts>.log`           |
/// | other external crates                         | `log/common_game/<ts>.log`                      |
/// | remaining `orch::general` messages            | `log/general/<ts>.log`                          |
/// | *(all targets)*                               | `log/all/<ts>.log`                              |
///
/// All messages are also printed to **stderr** for terminal visibility.
///
/// ## `RUST_LOG` Configuration
/// The log level is controlled by `RUST_LOG` (environment variable or `.env`).
/// Supports module-level filtering directives:
///
/// - `RUST_LOG="debug"` — Global debug level
/// - `RUST_LOG="explorer_rob=trace,debug"` — Module-specific: `explorer_rob` at Trace, others at Debug
/// - `RUST_LOG="orchestrator=trace,info"` — Selective: orchestrator at Trace, others at Info
/// - Default if not set: `info`
///
/// # Panics
/// Panics if any log directory or file cannot be created/opened.
pub(super) fn start_logger() {
    start_logger_with_console(true);
}

/// Same as [`start_logger`], but allows disabling terminal (stderr) output.
///
/// When `print_to_stderr` is `false`, logs are still written to files.
/// # Panics
/// Panics if the log directory or any log file cannot be created/opened.
pub fn start_logger_with_console(print_to_stderr: bool) {
    let dotenv_map = read_local_dotenv();

    let log_root = std::env::var("LOG_DIR")
        .ok()
        .or_else(|| dotenv_map.get("LOG_DIR").cloned())
        .unwrap_or_else(|| "log".to_string());
    std::fs::create_dir_all(&log_root).expect("failed to create log/ directory");

    let now = chrono::Local::now();
    let log_filename = format!("{}.log", now.format("%Y_%m_%d_%H-%M-%S"));

    // Parse RUST_LOG with support for module-level directives
    let directives = std::env::var("RUST_LOG")
        .ok()
        .or_else(|| dotenv_map.get("RUST_LOG").cloned())
        .map_or_else(
            || LogDirectives::parse("info"),
            |s| LogDirectives::parse(&s),
        );

    let f = |subdir: &str| open_log_file(&format!("{log_root}/{subdir}"), &log_filename);

    let terminal = if print_to_stderr {
        Some(build_terminal_log())
    } else {
        None
    };

    let router = ContentRouter {
        // asteroids & sunrays — nested under asteroids_sunrays/
        asteroids_sunrays: build_file_log(f("asteroids_sunrays")),
        asteroids: build_file_log(f("asteroids_sunrays/asteroids")),
        sunrays: build_file_log(f("asteroids_sunrays/sunrays")),
        // conversations — nested under conversations/
        conversations: build_file_log(f("conversations")),
        conversations_planets: build_file_log(f("conversations/planets")),
        conversations_explorers: build_file_log(f("conversations/explorers")),
        // channel messages — nested under channel_messages/
        channel_messages: build_file_log(f("channel_messages")),
        channel_messages_planets: build_file_log(f("channel_messages/planets")),
        channel_messages_explorers: build_file_log(f("channel_messages/explorers")),
        channel_messages_ui: build_file_log(f("channel_messages/ui")),
        // lifecycle & other
        general: build_file_log(f("general")),
        planet_lifecycle: build_file_log(f(TARGET_PLANET_LIFECYCLE.trim_start_matches("orch::"))),
        explorer_lifecycle: build_file_log(f(
            TARGET_EXPLORER_LIFECYCLE.trim_start_matches("orch::")
        )),
        orchestrator_lifecycle: build_file_log(f(
            TARGET_ORCHESTRATOR_LIFECYCLE.trim_start_matches("orch::")
        )),
        common_game: build_file_log(f("common_game")),
        shared: build_file_log(f("all")),
        terminal,
        directives: directives.clone(),
    };

    log::set_boxed_logger(Box::new(router)).expect("failed to initialize logger");
    // Set max_level to Trace to allow all levels through; actual filtering happens in ContentRouter
    log::set_max_level(log::LevelFilter::Trace);
}
