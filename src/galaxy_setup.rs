use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::protocols::planet_explorer::{ExplorerToPlanet};
use common_game::utils::ID;
use crossbeam_channel::unbounded;
use crossbeam_channel::{Receiver, Sender};

use immutable_cosmic_borrow::{create_planet, Ai};

use crate::planet::{PlanetNode, Alive};

type SenderMap = HashMap<u32, Sender<OrchestratorToPlanet>>;
type ReceiverMap = HashMap<u32, Receiver<OrchestratorToPlanet>>;

#[allow(dead_code)]
pub fn create_planets_channels(
    n_planets: u32,
) -> (
    SenderMap,
    ReceiverMap,
    Receiver<PlanetToOrchestrator>,
    Sender<PlanetToOrchestrator>,
) {
    // Step 1: #n_planets channels Orchestrator --> Planets
    let mut orch_to_plan_send = HashMap::new();
    let mut orch_to_plan_rec = HashMap::new();

    for i in 0..n_planets {
        let (tx_orch_to_planet, rx_orch_to_planet) = unbounded::<OrchestratorToPlanet>();
        orch_to_plan_send.insert(i, tx_orch_to_planet);
        orch_to_plan_rec.insert(i, rx_orch_to_planet);
    }

    // Step 2: 1 channel Planet --> Orchestrator
    let (tx_planet_to_orch, rx_planet_to_orch) = unbounded::<PlanetToOrchestrator>();
    (
        orch_to_plan_send,
        orch_to_plan_rec,
        rx_planet_to_orch,
        tx_planet_to_orch,
    )
}

#[allow(dead_code)]
pub fn galaxy_loader(
    file_path: &Path,
) -> HashMap<ID, std::rc::Rc<PlanetNode<Alive>>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::rc::Rc;

    // Ensure the parent directory exists, create it if it doesn't
    if let Some(parent) = file_path.parent().filter(|p| !p.exists()) {
        std::fs::create_dir_all(parent).expect("Failed to create directory path");
    }

    let (_tx_orch_in, rx_orch_in) = unbounded::<OrchestratorToPlanet>();
    let (tx_orch_out, _rx_orch_out) = unbounded::<PlanetToOrchestrator>();
    let (_tx_expl_in, rx_expl_in) = unbounded::<ExplorerToPlanet>();

    // First pass: create all planet nodes
    let file = File::open(file_path).expect("Failed to open galaxy file");
    let reader = BufReader::new(file);
    let mut out: HashMap<ID, Rc<PlanetNode<Alive>>> = HashMap::new();

    // Store edges for second pass
    let mut edges: Vec<(ID, Vec<ID>)> = Vec::new();

    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        if line.trim().is_empty() { continue; }
        let mut parts = line.split_whitespace();
        let id: ID = parts.next().expect("Missing id").parse().expect("Invalid id");
        let neighbors: Vec<ID> = parts.map(|s| s.parse().expect("Invalid neighbor id")).collect();
        edges.push((id, neighbors.clone()));
        // Create planet node if not already present
        out.entry(id).or_insert_with(|| {
            let ai = Ai::new(
                false,
                0.4,
                0.3,
                Duration::from_millis(500),
                Duration::from_millis(2000),
            );
            let planet = create_planet(ai, id, (rx_orch_in.clone(), tx_orch_out.clone()), rx_expl_in.clone());
            Rc::new(PlanetNode::<Alive>::new(planet.expect("Failed to create planet")))
        });
        // Also ensure all neighbors exist as nodes
        for &neighbor_id in &neighbors {
            out.entry(neighbor_id).or_insert_with(|| {
                let ai = Ai::new(
                    false,
                    0.4,
                    0.3,
                    Duration::from_millis(500),
                    Duration::from_millis(2000),
                );
                let planet = create_planet(ai, neighbor_id, (rx_orch_in.clone(), tx_orch_out.clone()), rx_expl_in.clone());
                Rc::new(PlanetNode::<Alive>::new(planet.expect("Failed to create planet")))
            });
        }
    }

    // Second pass: add neighbors (edges)
    for (id, neighbors) in edges {
        let node = out.get(&id).expect("Node missing");
        for neighbor_id in neighbors {
            let neighbor = out.get(&neighbor_id).expect("Neighbor node missing");
            node.add_neighbor(Rc::downgrade(neighbor));
        }
    }

    out
}
