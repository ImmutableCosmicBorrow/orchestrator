use common_game::components::planet::Planet;
use common_game::components::resource::BasicResourceType;
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::protocols::planet_explorer::ExplorerToPlanet;
use common_game::utils::ID;
use crossbeam_channel::{Receiver, Sender};
use houston_we_have_a_borrow::houston_we_have_a_borrow;
use trip::trip;

pub fn create_trip_planet(
    id: ID,
    rx_orchestrator: Receiver<OrchestratorToPlanet>,
    tx_orchestrator: Sender<PlanetToOrchestrator>,
    rx_explorer: Receiver<ExplorerToPlanet>,
) -> Result<Planet, String> {
    trip(id, rx_orchestrator, tx_orchestrator, rx_explorer)
}

// Temp allows to ignore unused variables and needless pass by value warnings while the planet is not updated
#[allow(unused_variables, clippy::needless_pass_by_value)]
pub fn create_rustrelli_planet(
    id: ID,
    rx_orchestrator: Receiver<OrchestratorToPlanet>,
    tx_orchestrator: Sender<PlanetToOrchestrator>,
    rx_explorer: Receiver<ExplorerToPlanet>,
    request_limit: rustrelli::ExplorerRequestLimit,
) -> Result<Planet, String> {
    /*Ok(rustrelli::create_planet(
        id,
        rx_orchestrator,
        tx_orchestrator,
        rx_explorer,
        request_limit,
    ))*/
    Err(
        "Rustrelli planet still uses 2.0.0 common-game crate, needs to be updated to 3.0.0"
            .to_string(),
    )
}

pub fn create_luna4_planet(
    id: ID,
    rx_orchestrator: Receiver<OrchestratorToPlanet>,
    tx_orchestrator: Sender<PlanetToOrchestrator>,
    rx_explorer: Receiver<ExplorerToPlanet>,
) -> Result<Planet, String> {
    luna4::create_planet(id, rx_orchestrator, tx_orchestrator, rx_explorer)
}

// Wrapper allowed to return the same type for all planet creation functions
#[allow(clippy::unnecessary_wraps)]
pub fn create_rusty_crab_planet(
    id: ID,
    rx_orchestrator: Receiver<OrchestratorToPlanet>,
    tx_orchestrator: Sender<PlanetToOrchestrator>,
    rx_explorer: Receiver<ExplorerToPlanet>,
) -> Result<Planet, String> {
    Ok(rusty_crab_ap2025::planet::create_planet(
        rx_orchestrator,
        tx_orchestrator,
        rx_explorer,
        id,
    ))
}

// Wrapper allowed to return the same type for all planet creation functions
#[allow(clippy::unnecessary_wraps)]
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

// Wrapper allowed to return the same type for all planet creation functions
#[allow(clippy::unnecessary_wraps)]
pub fn create_orbitron_planet(
    id: ID,
    rx_orchestrator: Receiver<OrchestratorToPlanet>,
    tx_orchestrator: Sender<PlanetToOrchestrator>,
    rx_explorer: Receiver<ExplorerToPlanet>,
) -> Result<Planet, String> {
    Ok(orbitron::create_planet(
        rx_orchestrator,
        tx_orchestrator,
        rx_explorer,
        id,
    ))
}

pub fn create_houston_we_have_a_borrow_planet(
    rx_orchestrator: Receiver<OrchestratorToPlanet>,
    tx_orchestrator: Sender<PlanetToOrchestrator>,
    rx_explorer: Receiver<ExplorerToPlanet>,
    id: ID,
    rocket_strategy: houston_we_have_a_borrow::RocketStrategy,
    basic_resource: Option<BasicResourceType>,
) -> Result<Planet, String> {
    houston_we_have_a_borrow(
        rx_orchestrator,
        tx_orchestrator,
        rx_explorer,
        id,
        rocket_strategy,
        basic_resource,
    )
}
