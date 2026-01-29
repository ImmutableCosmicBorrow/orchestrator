mod galaxy_setup;
mod globals;
mod id;
mod logging_utils;
mod orchestrator;
mod planet;
mod planet_factory;

// Re-export public items that other crates can use
pub use globals::get_id_manager;
pub use orchestrator::ExplorerType;
pub use orchestrator::Orchestrator;

use common_game::utils::ID;
use std::path::Path;

/// Run the orchestrator with the default galaxy configuration
/// - `explorer1`: The `ExplorerType` of the first Explorer.
/// - `explorer2`: An optional `ExplorerType` for the optional second Explorer
/// - `spawn_planet`: An `Option<ID>`. If provided, the Explorer will be spawned in this Planet, otherwise a random one will be chosen.
/// - `game_step`: A parameter that regulates the speed of the Explorer's actions.
#[must_use]
pub fn run(
    explorer1: ExplorerType,
    explorer2: Option<ExplorerType>,
    spawn_planet: Option<ID>,
    game_step: u64,
) -> Orchestrator {
    // Initialize and start logger
    logging_utils::start_logger();

    let mut orchestrator = Orchestrator::new(
        Path::new("galaxy/test_galaxy.txt"),
        explorer1,
        explorer2,
        spawn_planet,
        game_step,
    );

    orchestrator.run();

    orchestrator
}

/// Create the orchestrator with a custom galaxy file path
/// - `file_path`: The path of the galaxy configuration file.
/// - `explorer1`: The `ExplorerType` of the first Explorer.
/// - `explorer2`: An optional `ExplorerType` for the optional second Explorer
/// - `spawn_planet`: An `Option<ID>`. If provided, the Explorer will be spawned in this Planet, otherwise a random one will be chosen.
/// - `game_step`: A parameter that regulates the speed of the Explorer's actions.
pub fn create_with_path<P: AsRef<Path>>(
    galaxy_path: P,
    explorer1: ExplorerType,
    explorer2: Option<ExplorerType>,
    spawn_planet: Option<ID>,
    game_step: u64,
) -> Orchestrator {
    logging_utils::start_logger();

    Orchestrator::new(
        galaxy_path.as_ref(),
        explorer1,
        explorer2,
        spawn_planet,
        game_step,
    )
}
