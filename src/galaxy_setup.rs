use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::protocols::planet_explorer::ExplorerToPlanet;
use common_game::utils::ID;
use crossbeam_channel::unbounded;
use crossbeam_channel::{Receiver, Sender};
use immutable_cosmic_borrow::{Ai, create_planet};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::logging_utils::log_internal;
use crate::payload;
use crate::planet::{Alive, PlanetNode};

pub(crate) type OrchPlanSenderMap = HashMap<ID, Sender<OrchestratorToPlanet>>;
//TODO: Allow the PlanetMap to have dead planets so that they can be revived later
pub(crate) type PlanetMap = Arc<Mutex<HashMap<ID, Arc<Mutex<PlanetNode<Alive>>>>>>;

// TODO: add a parameter to customize planet creation with other groups planets
fn create_planet_with_channels(
    sender_map: &mut OrchPlanSenderMap,
    planet_id: ID,
    tx_orch_out: Sender<PlanetToOrchestrator>,
    rx_expl_in: Receiver<ExplorerToPlanet>,
) -> PlanetNode<Alive> {
    let (tx_orch_in, rx_orch_in) = unbounded::<OrchestratorToPlanet>();
    sender_map.insert(planet_id, tx_orch_in);

    let ai = Ai::new(
        false,
        0.4,
        0.3,
        Duration::from_millis(500),
        Duration::from_millis(2000),
    );

    let planet = create_planet(ai, planet_id, (rx_orch_in, tx_orch_out), rx_expl_in);

    log_internal(
        Channel::Info,
        payload!(
            action : "Created Planet",
            planet_id : planet_id,
        ),
    );

    PlanetNode::<Alive>::new(planet.expect("Failed to create planet"))
}

#[allow(dead_code)]
pub fn galaxy_loader(
    file_path: &Path,
) -> (PlanetMap, Receiver<PlanetToOrchestrator>, OrchPlanSenderMap) {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    // Ensure the parent directory exists, create it if it doesn't
    if let Some(parent) = file_path.parent().filter(|p| !p.exists()) {
        std::fs::create_dir_all(parent).expect("Failed to create directory path");
    }

    let (tx_orch_out, rx_orch_out) = unbounded::<PlanetToOrchestrator>();
    let (_tx_expl_in, rx_expl_in) = unbounded::<ExplorerToPlanet>();

    // First pass: create all planet nodes
    let file = File::open(file_path).expect("Failed to open galaxy file");
    let reader = BufReader::new(file);
    let mut out: HashMap<ID, Arc<Mutex<PlanetNode<Alive>>>> = HashMap::new();

    // Store edges for second pass
    let mut edges: Vec<(ID, Vec<ID>)> = Vec::new();

    let mut orch_to_plan_send = HashMap::new();
    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        if line.trim().is_empty() {
            continue;
        }

        let mut parts = line.split_whitespace();
        let id: ID = parts
            .next()
            .expect("Missing id")
            .parse()
            .expect("Invalid id");

        let neighbors: Vec<ID> = parts
            .map(|s| s.parse().expect("Invalid neighbor id"))
            .collect();

        edges.push((id, neighbors.clone()));

        // Create planet node if not already present
        out.entry(id).or_insert_with(|| {
            Arc::new(Mutex::new(create_planet_with_channels(
                &mut orch_to_plan_send,
                id,
                tx_orch_out.clone(),
                rx_expl_in.clone(),
            )))
        });

        // Also ensure all neighbors exist as nodes
        for &neighbor_id in &neighbors {
            out.entry(neighbor_id).or_insert_with(|| {
                Arc::new(Mutex::new(create_planet_with_channels(
                    &mut orch_to_plan_send,
                    neighbor_id,
                    tx_orch_out.clone(),
                    rx_expl_in.clone(),
                )))
            });
        }
    }

    // Second pass: add neighbors (edges)
    for (id, neighbors) in edges {
        let node = out.get(&id).expect("Node missing");
        for neighbor_id in neighbors {
            let neighbor: &Arc<Mutex<PlanetNode<Alive>>> =
                out.get(&neighbor_id).expect("Neighbor node missing");
            node.lock().unwrap().add_neighbor(Arc::downgrade(neighbor));
        }
    }

    log_internal(
        Channel::Info,
        payload!(
            action : "Loaded galaxy from file"
        ),
    );

    (Arc::new(Mutex::new(out)), rx_orch_out, orch_to_plan_send)
}
