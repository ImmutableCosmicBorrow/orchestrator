pub mod args;
pub mod output;
pub mod parser;
pub mod repl;

pub use args::Cli;
use crossbeam_channel::{Receiver, Sender};
use orchestrator::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};

use self::{output::print_ui_update, repl::run_repl};
use std::thread;

pub fn run(
    ui_to_orch_sender: Sender<UiToOrchestratorCommand>,
    orch_to_ui_receiver: Receiver<OrchestratorToUiUpdate>,
    cli_args: &Cli,
) {
    let ui_printer_thread = thread::spawn(move || {
        while let Ok(update) = orch_to_ui_receiver.recv() {
            let is_game_over = matches!(update, OrchestratorToUiUpdate::GameOver(_));
            print_ui_update(update);
            if is_game_over {
                break;
            }
        }
    });

    if !cli_args.no_repl {
        let sender_clone = ui_to_orch_sender.clone();
        thread::spawn(move || {
            run_repl(&sender_clone);
        });
    }

    drop(ui_to_orch_sender);
    let _ = ui_printer_thread.join();
}
