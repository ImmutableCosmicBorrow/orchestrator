use common_game::components::planet::Planet;
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::protocols::planet_explorer::ExplorerToPlanet;
use common_game::utils::ID;
use crossbeam_channel::{Receiver, Sender};

pub fn create_trip_planet() -> Result<Planet, String> {
    // Implementation for creating a Trip Planet
    todo!()
}

pub fn create_rustrelli_planet(
    id: ID,
    rx_orchestrator: Receiver<OrchestratorToPlanet>,
    tx_orchestrator: Sender<PlanetToOrchestrator>,
    rx_explorer: Receiver<ExplorerToPlanet>,
    request_limit: rustrelli::ExplorerRequestLimit,
) -> Result<Planet, String> {
    Ok(rustrelli::create_planet(
        id,
        rx_orchestrator,
        tx_orchestrator,
        rx_explorer,
        request_limit,
    ))
}

pub fn create_luna4_planet(
    id: ID,
    rx_orchestrator: Receiver<OrchestratorToPlanet>,
    tx_orchestrator: Sender<PlanetToOrchestrator>,
    rx_explorer: Receiver<ExplorerToPlanet>,
) -> Result<Planet, String> {
    luna4::create_planet(id, rx_orchestrator, tx_orchestrator, rx_explorer)
}

pub fn create_rusty_crab_planet() -> Result<Planet, String> {
    // Implementation for creating a Rusty Crab Planet
    todo!()
}

pub fn create_enterprise_planet(
    id: ID,
    rx_orchestrator: Receiver<OrchestratorToPlanet>,
    tx_orchestrator: Sender<PlanetToOrchestrator>,
    rx_explorer: Receiver<ExplorerToPlanet>,
) -> Result<Planet, String> {
    Ok(enterprise::create_planet(
        id,
        rx_orchestrator,
        tx_orchestrator,
        rx_explorer,
    ))
}

pub fn create_orbitron_planet() -> Result<Planet, String> {
    // Implementation for creating an Orbitron Planet
    todo!()
}

pub fn create_houston_we_have_a_borrow_planet() -> Result<Planet, String> {
    // Implementation for creating a Houston We Have A Borrow Planet
    todo!()
}
