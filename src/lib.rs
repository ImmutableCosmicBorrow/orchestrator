mod galaxy_setup;
mod globals;
mod id;
mod logging_utils;
mod orchestrator;
mod planet;
mod planet_factory;
mod ui;

// Re-export public items that other crates can use
pub use globals::get_id_manager;
pub use orchestrator::ExplorerType;
pub use orchestrator::Orchestrator;

use std::path::Path;

use crate::ui::OrchestratorToUiUpdate;
use crate::ui::UiToOrchestratorCommand;

/// Run the orchestrator with the default galaxy configuration
#[must_use]
pub fn run(
    game_step: u64,
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
    );

    orchestrator.run();

    (orchestrator, ui_to_orch_sender, orch_to_ui_receiver)
}

/// Create the orchestrator with a custom galaxy file path
pub fn create_with_path<P: AsRef<Path>>(
    galaxy_path: P,
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
    );

    (orchestrator, ui_to_orch_sender, orch_to_ui_receiver)
}
