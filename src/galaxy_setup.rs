use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::protocols::planet_explorer::ExplorerToPlanet;
use common_game::utils::ID;
use crossbeam_channel::unbounded;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use common_game::logging::{ActorType, Channel, EventType, LogEvent, Participant, Payload};
use crate::planet::{Alive, PlanetNode};

type OrchPlanSenderMap = HashMap<ID, Sender<OrchestratorToPlanet>>;
type ExplPlanSenderMap = HashMap<ID, Sender<ExplorerToPlanet>>;
pub(crate) type PlanetMap = Arc<Mutex<HashMap<ID, Arc<Mutex<PlanetNode<Alive>>>>>>;

// TODO: add a parameter to customize planet creation with other groups planets
fn create_planet_with_channels(
    orch_sender_map: &mut OrchPlanSenderMap,
    expl_sender_map: &mut ExplPlanSenderMap,
    planet_id: ID,
    tx_orch_out: Sender<PlanetToOrchestrator>,
) -> PlanetNode<Alive> {
    let (tx_orch_in, rx_orch_in) = unbounded::<OrchestratorToPlanet>();
    orch_sender_map.insert(planet_id, tx_orch_in);

    let (tx_expl_in, rx_expl_in) = unbounded::<ExplorerToPlanet>();
    expl_sender_map.insert(planet_id, tx_expl_in);

    let planet = match planet_id % 7 {
        0 => crate::planet_factory::create_trip_planet(),
        1 => crate::planet_factory::create_rustrelli_planet(
            planet_id,
            rx_orch_in,
            tx_orch_out,
            rx_expl_in,
            rustrelli::ExplorerRequestLimit::FairShare,
        ),
        2 => crate::planet_factory::create_luna4_planet(
            planet_id,
            rx_orch_in,
            tx_orch_out,
            rx_expl_in,
        ),
        3 => crate::planet_factory::create_rusty_crab_planet(),
        4 => crate::planet_factory::create_enterprise_planet(
            planet_id,
            rx_orch_in,
            tx_orch_out,
            rx_expl_in,
        ),
        5 => crate::planet_factory::create_orbitron_planet(),
        6 => crate::planet_factory::create_houston_we_have_a_borrow_planet(),
        _ => unreachable!(),
    };

    // Emit log event
    let mut payload = Payload::new();
    payload.insert("event".into(), "Planet creation".into());
    payload.insert("planet_id".into(), planet_id.to_string());
    payload.insert("success".into(), planet.is_ok().to_string());

    let mut channel = Channel::Info;
    if let Err(ref error) = planet {
        payload.insert("error".into(), error.into());
        channel = Channel::Error;
    }

    LogEvent::system(EventType::InternalOrchestratorAction, channel, payload).emit();

    PlanetNode::<Alive>::new(planet.expect("Failed to create planet"))
}

pub fn galaxy_loader(
    file_path: &Path,
) -> (
    PlanetMap,
    Receiver<PlanetToOrchestrator>,
    OrchPlanSenderMap,
    ExplPlanSenderMap,
) {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    // Ensure the parent directory exists, create it if it doesn't
    if let Some(parent) = file_path.parent().filter(|p| !p.exists()) {
        std::fs::create_dir_all(parent).expect("Failed to create directory path");
    }

    let (tx_orch_out, rx_orch_out) = unbounded::<PlanetToOrchestrator>();

    // First pass: create all planet nodes
    let file = File::open(file_path).expect("Failed to open galaxy file");
    let reader = BufReader::new(file);
    let mut out: HashMap<ID, Arc<Mutex<PlanetNode<Alive>>>> = HashMap::new();

    // Store edges for second pass
    let mut edges: Vec<(ID, Vec<ID>)> = Vec::new();

    let mut orch_to_plan_send = HashMap::new();
    let mut expl_to_plan_send = HashMap::new();
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
                &mut expl_to_plan_send,
                id,
                tx_orch_out.clone(),
            )))
        });

        // Also ensure all neighbors exist as nodes
        for &neighbor_id in &neighbors {
            out.entry(neighbor_id).or_insert_with(|| {
                Arc::new(Mutex::new(create_planet_with_channels(
                    &mut orch_to_plan_send,
                    &mut expl_to_plan_send,
                    neighbor_id,
                    tx_orch_out.clone(),
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

    // Emit log event
    let mut payload = Payload::new();
    payload.insert("event".into(), "Galaxy loaded".into());
    payload.insert("file_path".into(), file_path.display().to_string());

    LogEvent::system(EventType::InternalOrchestratorAction, Channel::Debug, payload).emit();

    (
        Arc::new(Mutex::new(out)),
        rx_orch_out,
        orch_to_plan_send,
        expl_to_plan_send,
    )
}
