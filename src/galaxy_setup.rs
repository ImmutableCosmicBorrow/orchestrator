use std::collections::HashMap;
use std::hash::Hash;

use common_game::protocols::messages::{OrchestratorToPlanet, PlanetToOrchestrator};
use crossbeam_channel::unbounded;
use crossbeam_channel::{Receiver, Sender};

pub fn create_planets_channels(
    n_planets: u32,
) -> (
    HashMap<u32, Sender<OrchestratorToPlanet>>,
    HashMap<u32, Receiver<OrchestratorToPlanet>>,
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
