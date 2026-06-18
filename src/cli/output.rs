use std::collections::BTreeMap;

use common_game::utils::ID;
use orchestrator::ui::OrchestratorToUiUpdate;

pub fn print_help() {
    println!("Commands:");
    println!("  help");
    println!("  end | quit");
    println!("  pause | resume | mode");
    println!("  galaxy | gal");
    println!("  positions | pos");
    println!("  planet | p <planet_id>");
    println!("  explorer | e <explorer_id>");
    println!("  start-planet | startp <planet_id>");
    println!("  stop-planet | stopp <planet_id>");
    println!("  reset-planet | rp <planet_id>");
    println!("  kill-planet | kp <planet_id>");
    println!("  start-explorer | starte <explorer_id>");
    println!("  stop-explorer | stope <explorer_id>");
    println!("  reset-explorer | re <explorer_id>");
    println!("  kill-explorer | ke <explorer_id>");
    println!("  asteroid | ast <planet_id>");
    println!("  sunray | sun <planet_id>");
    println!("  add-explorer | ae <explorer|vojager|nomad> <planet_id>");
    println!("  move | m <explorer_id> <dst_planet_id> [current_planet_id|-]");
    println!("  generate | gen <explorer_id> <carbon|hydrogen|oxygen|silicon>");
    println!("  craft | c <explorer_id> <water|robot|life|diamond|dolphin|aipartner>");
    println!("  supported-resources | res <explorer_id>");
    println!("  supported-combinations | comb <explorer_id>");
}

fn print_line(msg: String) {
    #[allow(clippy::collapsible_if)]
    if let Ok(mut lock) = orchestrator::logging::EXTERNAL_PRINTER.lock() {
        if let Some(printer) = lock.as_mut() {
            let _ = printer.print(msg);
            return;
        }
    }
    std::println!("{msg}");
}

macro_rules! println {
    ($($arg:tt)*) => {
        print_line(format!($($arg)*));
    };
}

pub fn print_ui_update(update: OrchestratorToUiUpdate) {
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
            // `loc` is a DashMap of explorer_id -> planet_id. Iterate its entries.
            if loc.is_empty() {
                println!("No explorers found");
                return;
            }

            let mut by_planet: BTreeMap<ID, Vec<ID>> = BTreeMap::new();
            for entry in &loc {
                let explorer_id = *entry.key();
                let planet_id = *entry.value();
                by_planet.entry(planet_id).or_default().push(explorer_id);
            }

            for (planet_id, mut explorers) in by_planet {
                explorers.sort_unstable();
                let explorers_str = explorers
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");

                println!("Explorers on planet {} -> {}", planet_id, explorers_str);
            }
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
            println!("Explorer {} status", id);
            for (resource, amount) in &bag.resources_amounts {
                println!("{:?}: {}", resource, amount);
            }
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
