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
