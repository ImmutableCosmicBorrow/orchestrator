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
#[must_use]
pub fn run(game_step: u64) -> Orchestrator {
    // Initialize and start logger
    logging_utils::start_logger();

    let mut orchestrator = Orchestrator::new(Path::new("galaxy/test_galaxy.txt"), game_step);

    orchestrator.run();

    orchestrator
}

/// Create the orchestrator with a custom galaxy file path
pub fn create_with_path<P: AsRef<Path>>(galaxy_path: P, game_step: u64) -> Orchestrator {
    logging_utils::start_logger();

    Orchestrator::new(galaxy_path.as_ref(), game_step)
}
