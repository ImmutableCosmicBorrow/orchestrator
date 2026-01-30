mod galaxy_setup;
mod globals;
mod id;
mod logging_utils;
mod orchestrator;
pub mod planet;
mod planet_factory;
pub mod ui;

// Re-export public items that other crates can use
pub use globals::get_id_manager;
pub use orchestrator::ExplorerType;
pub use orchestrator::Orchestrator;

use common_game::utils::ID;
use std::path::Path;

use crate::ui::OrchestratorToUiUpdate;
use crate::ui::UiToOrchestratorCommand;

/// Run the orchestrator with the default galaxy configuration
/// - `explorer1`: The `ExplorerType` of the first Explorer.
/// - `explorer2`: An optional `ExplorerType` for the optional second Explorer
/// - `spawn_planet`: An `Option<ID>`. If provided, the Explorer will be spawned in this Planet, otherwise a random one will be chosen.
/// - `game_step`: A parameter that regulates the speed of the Explorer's actions.
#[must_use]
pub fn run(
    game_step: u64,
    explorer1: ExplorerType,
    explorer2: Option<ExplorerType>,
    spawn_planet: Option<ID>,
) -> (
    Orchestrator,
    crossbeam_channel::Sender<UiToOrchestratorCommand>,
    crossbeam_channel::Receiver<OrchestratorToUiUpdate>,
) {
    // Initialize and start logger
    logging_utils::start_logger();

    let (ui_to_orch_sender, ui_to_orch_receiver) =
        crossbeam_channel::unbounded::<UiToOrchestratorCommand>();
    let (orch_to_ui_sender, orch_to_ui_receiver) =
        crossbeam_channel::unbounded::<OrchestratorToUiUpdate>();

    let mut orchestrator = Orchestrator::new(
        Path::new("galaxy/test_galaxy.txt"),
        game_step,
        orch_to_ui_sender,
        ui_to_orch_receiver,
        explorer1,
        explorer2,
        spawn_planet,
    );

    // Initialize and start logger
    logging_utils::start_logger();

    orchestrator.run();

    (orchestrator, ui_to_orch_sender, orch_to_ui_receiver)
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
) -> (
    Orchestrator,
    crossbeam_channel::Sender<UiToOrchestratorCommand>,
    crossbeam_channel::Receiver<OrchestratorToUiUpdate>,
) {
    logging_utils::start_logger();

    let (ui_to_orch_sender, ui_to_orch_receiver) =
        crossbeam_channel::unbounded::<UiToOrchestratorCommand>();
    let (orch_to_ui_sender, orch_to_ui_receiver) =
        crossbeam_channel::unbounded::<OrchestratorToUiUpdate>();

    let orchestrator = Orchestrator::new(
        galaxy_path.as_ref(),
        game_step,
        orch_to_ui_sender,
        ui_to_orch_receiver,
        explorer1,
        explorer2,
        spawn_planet,
    );

    (orchestrator, ui_to_orch_sender, orch_to_ui_receiver)
}
