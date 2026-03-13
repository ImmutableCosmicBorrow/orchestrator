use clap::{Parser, ValueEnum};
use common_game::components::resource::{BasicResourceType, ComplexResourceType};
use common_game::utils::ID;
use orchestrator::ExplorerType;
use orchestrator::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::thread;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ExplorerKindArg {
    Explorer,
    Vojager,
    Nomad,
}

impl From<ExplorerKindArg> for ExplorerType {
    fn from(value: ExplorerKindArg) -> Self {
        match value {
            ExplorerKindArg::Explorer => ExplorerType::Explorer,
            ExplorerKindArg::Vojager => ExplorerType::Vojager,
            ExplorerKindArg::Nomad => ExplorerType::Nomad,
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "orchestrator", version, about = "Orchestrator CLI")]
struct Cli {
    /// Galaxy config file path.
    #[arg(long, default_value = "galaxy/test_galaxy.txt")]
    galaxy: PathBuf,

    /// Game step in milliseconds.
    #[arg(long, default_value_t = 1000)]
    game_step: u64,

    /// First explorer type.
    #[arg(long, value_enum, default_value_t = ExplorerKindArg::Explorer)]
    explorer1: ExplorerKindArg,

    /// Optional second explorer type.
    #[arg(long, value_enum)]
    explorer2: Option<ExplorerKindArg>,

    /// Optional spawn planet id for initial explorers.
    #[arg(long)]
    spawn_planet: Option<ID>,

    /// Disable interactive REPL and just run.
    #[arg(long, default_value_t = false)]
    no_repl: bool,
}

fn parse_id(input: &str, field: &str) -> Result<ID, String> {
    input
        .parse::<ID>()
        .map_err(|_| format!("Invalid {field}: '{input}'"))
}

fn parse_explorer_kind(input: &str) -> Result<ExplorerType, String> {
    match input.to_ascii_lowercase().as_str() {
        "explorer" => Ok(ExplorerType::Explorer),
        "vojager" => Ok(ExplorerType::Vojager),
        "nomad" => Ok(ExplorerType::Nomad),
        _ => Err(format!(
            "Invalid explorer type: '{input}'. Expected one of: explorer, vojager, nomad"
        )),
    }
}

fn parse_basic_resource(input: &str) -> Result<BasicResourceType, String> {
    match input.to_ascii_lowercase().as_str() {
        "carbon" => Ok(BasicResourceType::Carbon),
        "hydrogen" => Ok(BasicResourceType::Hydrogen),
        "oxygen" => Ok(BasicResourceType::Oxygen),
        "silicon" => Ok(BasicResourceType::Silicon),
        _ => Err(format!(
            "Invalid basic resource: '{input}'. Expected one of: carbon, hydrogen, oxygen, silicon"
        )),
    }
}

fn parse_complex_resource(input: &str) -> Result<ComplexResourceType, String> {
    match input.to_ascii_lowercase().as_str() {
        "water" => Ok(ComplexResourceType::Water),
        "robot" => Ok(ComplexResourceType::Robot),
        "life" => Ok(ComplexResourceType::Life),
        "diamond" => Ok(ComplexResourceType::Diamond),
        "dolphin" => Ok(ComplexResourceType::Dolphin),
        "aipartner" | "ai-partner" | "ai_partner" => Ok(ComplexResourceType::AIPartner),
        _ => Err(format!(
            "Invalid complex resource: '{input}'. Expected one of: water, robot, life, diamond, dolphin, aipartner"
        )),
    }
}

fn print_help() {
    println!("Commands:");
    println!("  help");
    println!("  end | quit");
    println!("  pause | resume | mode");
    println!("  galaxy");
    println!("  positions");
    println!("  planet <planet_id>");
    println!("  explorer <explorer_id>");
    println!("  start-planet <planet_id>");
    println!("  stop-planet <planet_id>");
    println!("  reset-planet <planet_id>");
    println!("  kill-planet <planet_id>");
    println!("  start-explorer <explorer_id>");
    println!("  stop-explorer <explorer_id>");
    println!("  reset-explorer <explorer_id>");
    println!("  kill-explorer <explorer_id>");
    println!("  asteroid <planet_id>");
    println!("  sunray <planet_id>");
    println!("  add-explorer <explorer|vojager|nomad> <planet_id>");
    println!("  move <explorer_id> <dst_planet_id> [current_planet_id|-]");
    println!("  generate <explorer_id> <carbon|hydrogen|oxygen|silicon>");
    println!("  craft <explorer_id> <water|robot|life|diamond|dolphin|aipartner>");
    println!("  supported-resources <explorer_id>");
    println!("  supported-combinations <explorer_id>");
}

fn print_ui_update(update: OrchestratorToUiUpdate) {
    match update {
        OrchestratorToUiUpdate::Galaxy(galaxy) => {
            let mut ids: Vec<ID> = galaxy
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .keys()
                .copied()
                .collect();
            ids.sort_unstable();

            if ids.is_empty() {
                println!("Planets: none");
            } else {
                let list = ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("Planets ({}): {}", ids.len(), list);
            }
        }
        OrchestratorToUiUpdate::ExplorersPosition(loc) => {
            let map = loc
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            if map.is_empty() {
                println!("Explorers by planet: none");
                return;
            }

            let mut by_planet: BTreeMap<ID, Vec<ID>> = BTreeMap::new();
            for (explorer_id, planet_id) in map.iter() {
                by_planet.entry(*planet_id).or_default().push(*explorer_id);
            }

            let mut chunks = Vec::new();
            for (planet_id, mut explorers) in by_planet {
                explorers.sort_unstable();
                let explorers_str = explorers
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                chunks.push(format!("{planet_id}:[{explorers_str}]"));
            }

            println!("Explorers by planet -> {}", chunks.join(" "));
        }
        OrchestratorToUiUpdate::PlanetSnapshot(id, snapshot) => {
            println!(
                "Planet {} status: charged_cells={}/{}, rocket={}",
                id,
                snapshot.charged_cells_count,
                snapshot.energy_cells.len(),
                if snapshot.has_rocket { "yes" } else { "no" }
            );
        }
        OrchestratorToUiUpdate::ExplorerSnapshot(id, bag) => {
            let total: u64 = bag.resources_amounts.values().sum();
            println!(
                "Explorer {} status: total_resources={}, resource_kinds={}",
                id,
                total,
                bag.resources_amounts.len()
            );
        }
        OrchestratorToUiUpdate::SupportedCombinations(id, combinations) => {
            let mut values = combinations
                .iter()
                .map(|c| format!("{c:?}"))
                .collect::<Vec<_>>();
            values.sort_unstable();
            println!(
                "Explorer {} supported combinations ({}): {}",
                id,
                values.len(),
                values.join(", ")
            );
        }
        OrchestratorToUiUpdate::SupportedResources(id, resources) => {
            let mut values = resources
                .iter()
                .map(|r| format!("{r:?}"))
                .collect::<Vec<_>>();
            values.sort_unstable();
            println!(
                "Explorer {} supported resources ({}): {}",
                id,
                values.len(),
                values.join(", ")
            );
        }
        OrchestratorToUiUpdate::DeadPlanet(id) => {
            println!("Planet {id} status: dead");
        }
        OrchestratorToUiUpdate::SendAutoSunray(_) | OrchestratorToUiUpdate::SendAutoAsteroid(_) => {
            // Ignored in CLI mode (background events are disabled).
        }
    }
}

#[allow(clippy::too_many_lines)]
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

    let ui_printer_thread = thread::spawn(move || {
        while let Ok(update) = orch_to_ui_receiver.recv() {
            print_ui_update(update);
        }
    });

    if cli.no_repl {
        let _ = orchestrator_thread.join();
        let _ = ui_printer_thread.join();
        return;
    }

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
            "galaxy" => Ok(Some(UiToOrchestratorCommand::GetGalaxy)),
            "positions" => Ok(Some(UiToOrchestratorCommand::GetExplorersPosition)),
            "planet" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::GetPlanetSnapshot(id))),
                _ => Err("Usage: planet <planet_id>".to_string()),
            },
            "explorer" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::GetExplorerSnapshot(id))),
                _ => Err("Usage: explorer <explorer_id>".to_string()),
            },
            "start-planet" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::StartPlanetAI(id))),
                _ => Err("Usage: start-planet <planet_id>".to_string()),
            },
            "stop-planet" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::StopPlanetAI(id))),
                _ => Err("Usage: stop-planet <planet_id>".to_string()),
            },
            "reset-planet" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::ResetPlanetAI(id))),
                _ => Err("Usage: reset-planet <planet_id>".to_string()),
            },
            "kill-planet" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::KillPlanet(id))),
                _ => Err("Usage: kill-planet <planet_id>".to_string()),
            },
            "start-explorer" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::StartExplorerAI(id))),
                _ => Err("Usage: start-explorer <explorer_id>".to_string()),
            },
            "stop-explorer" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::StopExplorerAI(id))),
                _ => Err("Usage: stop-explorer <explorer_id>".to_string()),
            },
            "reset-explorer" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::ResetExplorerAI(id))),
                _ => Err("Usage: reset-explorer <explorer_id>".to_string()),
            },
            "kill-explorer" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::KillExplorer(id))),
                _ => Err("Usage: kill-explorer <explorer_id>".to_string()),
            },
            "asteroid" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::SendManualAsteroid(id))),
                _ => Err("Usage: asteroid <planet_id>".to_string()),
            },
            "sunray" => match parts.as_slice() {
                [_, planet_id] => parse_id(planet_id, "planet_id")
                    .map(|id| Some(UiToOrchestratorCommand::SendManualSunray(id))),
                _ => Err("Usage: sunray <planet_id>".to_string()),
            },
            "add-explorer" => match parts.as_slice() {
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
                _ => Err("Usage: add-explorer <explorer|vojager|nomad> <planet_id>".to_string()),
            },
            "move" => match parts.as_slice() {
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
                    "Usage: move <explorer_id> <dst_planet_id> [current_planet_id|-]".to_string(),
                ),
            },
            "generate" => match parts.as_slice() {
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
                    "Usage: generate <explorer_id> <carbon|hydrogen|oxygen|silicon>".to_string(),
                ),
            },
            "craft" => match parts.as_slice() {
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
                    "Usage: craft <explorer_id> <water|robot|life|diamond|dolphin|aipartner>"
                        .to_string(),
                ),
            },
            "supported-resources" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::SupportedResources(id))),
                _ => Err("Usage: supported-resources <explorer_id>".to_string()),
            },
            "supported-combinations" => match parts.as_slice() {
                [_, explorer_id] => parse_id(explorer_id, "explorer_id")
                    .map(|id| Some(UiToOrchestratorCommand::SupportedCombinations(id))),
                _ => Err("Usage: supported-combinations <explorer_id>".to_string()),
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

    let _ = orchestrator_thread.join();
    drop(ui_to_orch_sender);
    let _ = ui_printer_thread.join();
}
