pub mod args;
pub mod output;
pub mod parser;
pub mod repl;

pub use args::Cli;

pub mod cli {
    use crossbeam_channel::{Receiver, Sender};

    use orchestrator::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};

    use super::{args::Cli, output::print_ui_update, repl::run_repl};
    use std::thread;

    pub fn run(
        ui_to_orch_sender: Sender<UiToOrchestratorCommand>,
        orch_to_ui_receiver: Receiver<OrchestratorToUiUpdate>,
        cli_args: Cli,
    ) {
        let ui_printer_thread = thread::spawn(move || {
            while let Ok(update) = orch_to_ui_receiver.recv() {
                print_ui_update(update);
            }
        });

        if !cli_args.no_repl {
            run_repl(ui_to_orch_sender.clone());
        }

        drop(ui_to_orch_sender);
        let _ = ui_printer_thread.join();
    }
}
