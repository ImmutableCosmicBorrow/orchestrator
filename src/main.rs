use clap::Parser;
use std::thread;

mod cli;

use cli::Cli;

fn main() {
    let cli = Cli::parse();

    let (mut orchestrator, ui_to_orch_sender, orch_to_ui_receiver) =
        orchestrator::create_with_path_options(
            &cli.galaxy,
            cli.explorer1.into(),
            cli.explorer2.map(Into::into),
            cli.spawn_planet,
            cli.game_step,
            false,
            false,
        );

    let orchestrator_thread = thread::spawn(move || {
        orchestrator.run();
    });

    cli::cli::run(ui_to_orch_sender, orch_to_ui_receiver, cli);

    let _ = orchestrator_thread.join();
}
