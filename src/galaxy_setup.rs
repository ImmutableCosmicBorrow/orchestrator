use crate::id::IdManager;
use crate::logging_utils::log_internal;
use crate::planet::{PlanetMap, add_planet_with_neighbors};
use crate::{get_id_manager, payload};

use common_explorer::{ExplorerAI, ExplorerBagContent};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::protocols::planet_explorer::{ExplorerToPlanet, PlanetToExplorer};
use common_game::utils::ID;

use crossbeam_channel::unbounded;
use crossbeam_channel::{Receiver, Sender};

use std::collections::HashMap;
use std::path::Path;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

// Planets are removed from PlanetMap and stopped via OrchestratorToPlanet message.
pub(crate) type OrchPlanSenderMap = HashMap<ID, Sender<OrchestratorToPlanet>>;
pub(crate) type OrchExplSenderMap = HashMap<ID, Sender<OrchestratorToExplorer>>;
pub(crate) type ExplPlanSenderMap = HashMap<ID, Sender<ExplorerToPlanet>>;
pub(crate) type PlanExplSenderMap = HashMap<ID, Sender<PlanetToExplorer>>;

/// Holds handles so the orchestrator can join or inspect planet threads if needed.
pub(crate) type PlanetThreadMap = HashMap<ID, JoinHandle<()>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanetKind {
    Trip,
    Rustrelli,
    Luna4,
    RustyCrab,
    Enterprise,
    Orbitron,
    Houston,
}

fn planet_kind(id: ID) -> PlanetKind {
    use PlanetKind::{Enterprise, Houston, Luna4, Orbitron, Rustrelli, RustyCrab, Trip};

    if IdManager::is_trip_id(id) {
        Trip
    } else if IdManager::is_rustrelli_id(id) {
        Rustrelli
    } else if IdManager::is_luna4_id(id) {
        Luna4
    } else if IdManager::is_rusty_crab_id(id) {
        RustyCrab
    } else if IdManager::is_enterprise_id(id) {
        Enterprise
    } else if IdManager::is_orbitron_id(id) {
        Orbitron
    } else if IdManager::is_houston_id(id) {
        Houston
    } else {
        panic!("Invalid planet id (no known planet subtype bit set): {id}");
    }
}

// Option: spawn planet threads at creation time.
// Returns a JoinHandle<()> for the spawned planet thread.
pub(crate) fn spawn_planet_with_channels(
    orch_sender_map: &mut OrchPlanSenderMap,
    expl_sender_map: &mut ExplPlanSenderMap,
    planet_id: ID,
    tx_orch_out: Sender<PlanetToOrchestrator>,
) -> JoinHandle<()> {
    let (tx_orch_in, rx_orch_in) = unbounded::<OrchestratorToPlanet>();
    orch_sender_map.insert(planet_id, tx_orch_in);

    let (tx_expl_in, rx_expl_in) = unbounded::<ExplorerToPlanet>();
    expl_sender_map.insert(planet_id, tx_expl_in);

    let planet_res = match planet_kind(planet_id) {
        PlanetKind::Trip => crate::planet_factory::create_trip_planet(
            planet_id,
            rx_orch_in,
            tx_orch_out,
            rx_expl_in,
        ),

        PlanetKind::Rustrelli => crate::planet_factory::create_rustrelli_planet(
            planet_id,
            rx_orch_in,
            tx_orch_out,
            rx_expl_in,
            rustrelli::ExplorerRequestLimit::FairShare,
        ),

        PlanetKind::Luna4 => crate::planet_factory::create_luna4_planet(
            planet_id,
            rx_orch_in,
            tx_orch_out,
            rx_expl_in,
        ),

        PlanetKind::RustyCrab => crate::planet_factory::create_rusty_crab_planet(
            planet_id,
            rx_orch_in,
            tx_orch_out,
            rx_expl_in,
        ),

        PlanetKind::Enterprise => crate::planet_factory::create_enterprise_planet(
            planet_id,
            rx_orch_in,
            tx_orch_out,
            rx_expl_in,
        ),

        PlanetKind::Orbitron => crate::planet_factory::create_orbitron_planet(
            planet_id,
            rx_orch_in,
            tx_orch_out,
            rx_expl_in,
        ),

        PlanetKind::Houston => crate::planet_factory::create_houston_we_have_a_borrow_planet(
            rx_orch_in,
            tx_orch_out,
            rx_expl_in,
            planet_id,
            houston_we_have_a_borrow::RocketStrategy::Safe,
            None,
        ),
    };

    if let Err(ref e) = planet_res {
        log_internal(
            Channel::Error,
            payload!(
                action : "Planet creation failed",
                planet_id : planet_id,
                error : e,
            ),
        );
    } else {
        log_internal(
            Channel::Info,
            payload!(
                action : "Created Planet",
                planet_id : planet_id,
            ),
        );
    }

    // Own the Planet in this scope so we can move it into the thread.
    let mut planet = planet_res.expect("Failed to create planet");

    // Spawn the blocking planet.run() immediately.
    thread::spawn(move || {
        let res = planet.run();

        if let Err(e) = res {
            log_internal(
                Channel::Error,
                payload!(
                    action : "Planet encountered an error during its main loop",
                    planet_id : planet_id,
                    error : e,
                ),
            );
        }
    })
}

pub fn galaxy_loader(
    file_path: &Path,
) -> (
    PlanetMap,
    Receiver<PlanetToOrchestrator>,
    OrchPlanSenderMap,
    ExplPlanSenderMap,
    PlanetThreadMap,
) {
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::sync::Arc;

    // Ensure the parent directory exists, create it if it doesn't
    if let Some(parent) = file_path.parent().filter(|p| !p.exists()) {
        std::fs::create_dir_all(parent).expect("Failed to create directory path");
    }

    let (tx_orch_out, rx_orch_out) = unbounded::<PlanetToOrchestrator>();

    // ✅ Create the shared PlanetMap FIRST (empty).
    let planet_map: PlanetMap = Arc::new(std::sync::RwLock::new(HashMap::new()));

    let mut orch_to_plan_send: OrchPlanSenderMap = HashMap::new();
    let mut expl_to_plan_send: ExplPlanSenderMap = HashMap::new();
    let mut planet_threads: PlanetThreadMap = HashMap::new();

    // Read file: build topology with the centralized edge store (planet.rs) and
    // spawn planet threads once per unique planet id.
    let file = File::open(file_path).expect("Failed to open galaxy file");
    let reader = BufReader::new(file);

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

        // Topology: ensure nodes exist and connect id <-> neighbors in one lock pass.
        // Edges are stored centrally, so one-sided links cannot be created.
        add_planet_with_neighbors(&planet_map, id, neighbors.iter().copied());

        // Runtime: spawn planet threads once per unique id, including neighbors.
        planet_threads.entry(id).or_insert_with(|| {
            spawn_planet_with_channels(
                &mut orch_to_plan_send,
                &mut expl_to_plan_send,
                id,
                tx_orch_out.clone(),
            )
        });

        for &neighbor_id in &neighbors {
            planet_threads.entry(neighbor_id).or_insert_with(|| {
                spawn_planet_with_channels(
                    &mut orch_to_plan_send,
                    &mut expl_to_plan_send,
                    neighbor_id,
                    tx_orch_out.clone(),
                )
            });
        }
    }

    (
        planet_map,
        rx_orch_out,
        orch_to_plan_send,
        expl_to_plan_send,
        planet_threads,
    )
}

/// Creates Explorers and starts their threads.
/// Returns:
/// - A `HashMap<ID, JoinHandle<()>>` containing the handles of the Explorers' threads
/// - An `OrchExplSenderMap`, a hashmap with the senders from Orchestrator to Explorer
/// - A `PlanExplSenderMap`, a hashmap with the senders from Planet to Explorer
// TODO: right now it just spawns an explorer_nico, needs to be changed. Also, Explorer is not sent to any Planet right now
pub(crate) fn create_and_spawn_explorers(
    tx_to_orchestrator: Sender<ExplorerToOrchestrator<ExplorerBagContent>>,
) -> (
    HashMap<ID, JoinHandle<()>>,
    OrchExplSenderMap,
    PlanExplSenderMap,
) {
    let mut handles = HashMap::new();
    let mut explorers_senders = HashMap::new();
    let mut planet_to_explorer_senders = HashMap::new();

    let (tx_orchestrator_to_explorer, rx_orchestrator_to_explorer) =
        unbounded::<OrchestratorToExplorer>();
    let (tx_planet_to_explorer, rx_planet_to_explorer) = unbounded::<PlanetToExplorer>();
    let id = get_id_manager().get_next_explorer_id();

    let planet_sender: Sender<ExplorerToPlanet> = unbounded::<ExplorerToPlanet>().0;
    let mut explorer = explorer_nico::Explorer::new(
        id,
        1,//TODO fix this, added at random
        planet_sender, // added at random 
        tx_to_orchestrator,
        rx_orchestrator_to_explorer,
        rx_planet_to_explorer,
        Duration::new(1000, 2000000), // added at random 
    );

    log_internal(
        Channel::Info,
        payload!(
            action : "Created Explorer",
            explorer_id : id,
        ),
    );

    let handle = thread::spawn(move || {
        let res = explorer.run();

        match res {
            Ok(()) => {
                log_internal(
                    Channel::Debug,
                    payload!(
                        action : "Explorer thread closed correctly"
                    ),
                );
            }
            Err(e) => {
                log_internal(
                    Channel::Warning,
                    payload!(
                        action : "Error in closing Explorer thread",
                        error : e
                    ),
                );
            }
        }
    });

    handles.insert(id, handle);
    explorers_senders.insert(id, tx_orchestrator_to_explorer);
    planet_to_explorer_senders.insert(id, tx_planet_to_explorer);

    (handles, explorers_senders, planet_to_explorer_senders)
}
