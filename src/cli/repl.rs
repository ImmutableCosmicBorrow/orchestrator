use orchestrator::ui::UiToOrchestratorCommand;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use crossbeam_channel::Sender;

use super::output::print_help;
use super::parser::{
    parse_basic_resource, parse_complex_resource, parse_explorer_kind, parse_id,
};

pub fn run_repl(ui_to_orch_sender: Sender<UiToOrchestratorCommand>) {
    println!("Orchestrator CLI started. Type 'help' for commands.");

    let history_path = ".orchestrator_cli_history";
    let mut rl = DefaultEditor::new().expect("failed to initialize command line editor");
    let _ = rl.load_history(history_path);

    let mut end_sent = false;

    loop {
        let line = match rl.readline("> ") {
            Ok(line) => {
                let _ = rl.add_history_entry(line.as_str());
                line
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("input error: {e}");
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        let cmd = parts[0].to_ascii_lowercase();
        let parsed_command: Result<Option<UiToOrchestratorCommand>, String> = match cmd.as_str() {
            "help" => {
                print_help();
                Ok(None)
            }
            "end" | "quit" => {
                end_sent = true;
                Ok(Some(UiToOrchestratorCommand::EndGame))
            }
            "pause" => Ok(Some(UiToOrchestratorCommand::PauseGame)),
            "resume" => Ok(Some(UiToOrchestratorCommand::ResumeGame)),
            "mode" => Ok(Some(UiToOrchestratorCommand::SwitchGameMode)),
            "galaxy" | "gal" => Ok(Some(UiToOrchestratorCommand::GetGalaxy)),
            "positions" | "pos" => Ok(Some(UiToOrchestratorCommand::GetExplorersPosition)),
            "planet" | "p" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::GetPlanetSnapshot(id))),
                _ => Err("Usage: planet|p <planet_id>".to_string()),
            },
            "explorer" | "e" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::GetExplorerSnapshot(id))),
                _ => Err("Usage: explorer|e <explorer_id>".to_string()),
            },
            "start-planet" | "startp" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::StartPlanetAI(id))),
                _ => Err("Usage: start-planet|startp <planet_id>".to_string()),
            },
            "stop-planet" | "stopp" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::StopPlanetAI(id))),
                _ => Err("Usage: stop-planet|stopp <planet_id>".to_string()),
            },
            "reset-planet" | "rp" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::ResetPlanetAI(id))),
                _ => Err("Usage: reset-planet|rp <planet_id>".to_string()),
            },
            "kill-planet" | "kp" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::KillPlanet(id))),
                _ => Err("Usage: kill-planet|kp <planet_id>".to_string()),
            },
            "start-explorer" | "starte" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::StartExplorerAI(id))),
                _ => Err("Usage: start-explorer|starte <explorer_id>".to_string()),
            },
            "stop-explorer" | "stope" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::StopExplorerAI(id))),
                _ => Err("Usage: stop-explorer|stope <explorer_id>".to_string()),
            },
            "reset-explorer" | "re" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::ResetExplorerAI(id))),
                _ => Err("Usage: reset-explorer|re <explorer_id>".to_string()),
            },
            "kill-explorer" | "ke" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::KillExplorer(id))),
                _ => Err("Usage: kill-explorer|ke <explorer_id>".to_string()),
            },
            "asteroid" | "ast" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::SendManualAsteroid(id))),
                _ => Err("Usage: asteroid|ast <planet_id>".to_string()),
            },
            "sunray" | "sun" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::SendManualSunray(id))),
                _ => Err("Usage: sunray|sun <planet_id>".to_string()),
            },
            "add-explorer" | "ae" => match parts.as_slice() {
                [_, explorer_kind, planet_id] => {
                    match (
                        parse_explorer_kind(explorer_kind),
                        parse_id(planet_id, "planet_id"),
                    ) {
                        (Ok(explorer), Ok(id)) => {
                            Ok(Some(UiToOrchestratorCommand::AddExplorer(explorer, id)))
                        }
                        (Err(e), _) | (_, Err(e)) => Err(e),
                    }
                }
                _ => Err("Usage: add-explorer|ae <explorer|vojager|nomad> <planet_id>".to_string()),
            },
            "move" | "m" => match parts.as_slice() {
                [_, explorer_id, dst_planet_id] => {
                    match (
                        parse_id(explorer_id, "explorer_id"),
                        parse_id(dst_planet_id, "dst_planet_id"),
                    ) {
                        (Ok(explorer), Ok(dst)) => Ok(Some(
                            UiToOrchestratorCommand::ManualMoveExplorer(explorer, None, dst),
                        )),
                        (Err(e), _) | (_, Err(e)) => Err(e),
                    }
                }
                [_, explorer_id, dst_planet_id, current_planet_id] => {
                    match (
                        parse_id(explorer_id, "explorer_id"),
                        parse_id(dst_planet_id, "dst_planet_id"),
                    ) {
                        (Ok(explorer), Ok(dst)) => {
                            let current = if *current_planet_id == "-" {
                                Ok(None)
                            } else {
                                parse_id(current_planet_id, "current_planet_id").map(Some)
                            };

                            current.map(|curr| {
                                Some(UiToOrchestratorCommand::ManualMoveExplorer(
                                    explorer, curr, dst,
                                ))
                            })
                        }
                        (Err(e), _) | (_, Err(e)) => Err(e),
                    }
                }
                _ => Err(
                    "Usage: move|m <explorer_id> <dst_planet_id> [current_planet_id|-]".to_string(),
                ),
            },
            "generate" | "gen" => match parts.as_slice() {
                [_, explorer_id, resource] => {
                    match (
                        parse_id(explorer_id, "explorer_id"),
                        parse_basic_resource(resource),
                    ) {
                        (Ok(id), Ok(resource_type)) => Ok(Some(
                            UiToOrchestratorCommand::ExplorerGenerateResource(id, resource_type),
                        )),
                        (Err(e), _) | (_, Err(e)) => Err(e),
                    }
                }
                _ => Err(
                    "Usage: generate|gen <explorer_id> <carbon|hydrogen|oxygen|silicon>".to_string(),
                ),
            },
            "craft" | "c" => match parts.as_slice() {
                [_, explorer_id, resource] => {
                    match (
                        parse_id(explorer_id, "explorer_id"),
                        parse_complex_resource(resource),
                    ) {
                        (Ok(id), Ok(resource_type)) => Ok(Some(
                            UiToOrchestratorCommand::ExplorerCombineResource(id, resource_type),
                        )),
                        (Err(e), _) | (_, Err(e)) => Err(e),
                    }
                }
                _ => Err(
                    "Usage: craft|c <explorer_id> <water|robot|life|diamond|dolphin|aipartner>"
                        .to_string(),
                ),
            },
            "supported-resources" | "res" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::SupportedResources(id))),
                _ => Err("Usage: supported-resources|res <explorer_id>".to_string()),
            },
            "supported-combinations" | "comb" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::SupportedCombinations(id))),
                _ => Err("Usage: supported-combinations|comb <explorer_id>".to_string()),
            },
            _ => Err("Unknown command. Type 'help'.".to_string()),
        };

        match parsed_command {
            Ok(Some(command)) => {
                if ui_to_orch_sender.send(command).is_err() {
                    eprintln!("orchestrator channel closed");
                    break;
                }
                if end_sent {
                    break;
                }
            }
            Ok(None) => {}
            Err(e) => eprintln!("{e}"),
        }
    }

    if !end_sent {
        let _ = ui_to_orch_sender.send(UiToOrchestratorCommand::EndGame);
    }

    let _ = rl.save_history(history_path);
}
