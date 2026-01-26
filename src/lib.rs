mod galaxy_setup;
mod globals;
mod id;
mod logging_utils;
mod orchestrator;
mod planet;
mod planet_factory;

// Re-export public items that other crates can use
pub use globals::get_id_manager;
pub use orchestrator::Orchestrator;

use std::path::Path;

/// Run the orchestrator with the default galaxy configuration
pub fn run() {
    // Initialize and start logger
    logging_utils::start_logger();

    let mut orchestrator = orchestrator::Orchestrator::new(Path::new("galaxy/test_galaxy.txt"));

    orchestrator.run();
}

/// Run the orchestrator with a custom galaxy file path
pub fn run_with_path<P: AsRef<Path>>(galaxy_path: P) {
    logging_utils::start_logger();

    let mut orchestrator = orchestrator::Orchestrator::new(galaxy_path.as_ref());

    orchestrator.run();
}
