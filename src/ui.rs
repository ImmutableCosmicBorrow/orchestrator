use crate::orchestrator::ExplorersLocationRef;
use common_explorer::ExplorerBagContent;
use common_game::components::planet::DummyPlanetState;
use common_game::{
    components::resource::{BasicResourceType, ComplexResourceType},
    utils::ID,
};

use crate::planet::PlanetMap;

//TODO: modify create_with_path to return also ui channels
//TODO: create conversation factory?

// Commands you can send to the orchestrator
pub enum UiToOrchestratorCommand {
    //rendering commands
    GetGalaxy,
    DeadPlanetAck(ID),
    AddedPlanet(ID),
    GetExplorersPosition,
    GetPlanetSnapshot(ID),
    GetExplorerSnapshot(ID),

    // explorer commands: move and resource crafting/combining
    ManualMoveExplorer(ID, ID, ID), // Explorer ID, current planet, dst planet
    ManualExplorerCraftsRes(ID, BasicResourceType),
    ManualExplorerCombineRes(ID, ComplexResourceType),
    AutoMoveExplorerAck(ID, ID, ID),
    AutoExplorerCraftsResAck(ID, BasicResourceType),
    AutoExplorerCombineResAck(ID, ComplexResourceType),
    SupportedCombinations(ID),
    SupportedResources(ID),

    // asteroid/sunrays commands
    SendManualAsteroid(ID),
    SendManualAsteroidAck(ID),
    SendAutoSunray(ID),
    SendAutoSunrayAck(ID),

    // start/stop/reset/kill AI commands
    StartPlanetAI(ID),
    StopPlanetAI(ID),
    ResetPlanetAI(ID),
    StartExplorerAI(ID),
    StopExplorerAI(ID),
    ResetExplorerAI(ID),
    KillExplorerAI(ID),
    KillPlanetAI(ID),
    StartPlanetAIAck(ID),
    StopPlanetAIAck(ID),
    ResetPlanetAIAck(ID),
    StartExplorerAIAck(ID),
    StopExplorerAIAck(ID),
    ResetExplorerAIAck(ID),
    KillExplorerAIAck(ID),
    KillPlanetAIAck(ID),
}

// Updates the orchestrator sends back
pub enum OrchestratorToUiUpdate {
    //rendering commands
    Galaxy(PlanetMap), //it's not a vec<planet>
    DeadPlanet(ID),
    AddedPlanetAck(ID),
    ExplorersPosition(ExplorersLocationRef),
    PlanetSnapshot(ID, DummyPlanetState),
    ExplorerSnapshot(ID, ExplorerBagContent),

    // explorer commands: move and resource crafting/combining
    ManualMoveExplorerAck(ID, ID, ID),
    ManualExplorerCraftsResAck(ID, BasicResourceType),
    ManualExplorerCombineResAck(ID, ComplexResourceType),
    AutoMoveExplorer(ID, ID, ID),
    AutoExplorerCraftsRes(ID, BasicResourceType),
    AutoExplorerCombineRes(ID, ComplexResourceType),
    SupportedCombinations(ID, Vec<ComplexResourceType>),
    SupportedResources(ID, Vec<BasicResourceType>),

    // asteroid/sunrays commands
    SendManualAsteroidAck(ID),
    SendManualSunrayAck(ID),
    SendAutoSunray(ID),
    SendAutoAsteroid(ID),

    // start/stop/reset AI commands
    StartPlanetAIAck(ID),
    StopPlanetAIAck(ID),
    ResetPlanetAIAck(ID),
    StartExplorerAIAck(ID),
    StopExplorerAIAck(ID),
    ResetExplorerAIAck(ID),
    KillExplorerAIAck(ID),
    KillPlanetAIAck(ID),
    StartPlanetAI(ID),
    StopPlanetAI(ID),
    ResetPlanetAI(ID),
    StartExplorerAI(ID),
    StopExplorerAI(ID),
    ResetExplorerAI(ID),
    KillExplorerAI(ID),
    KillPlanetAI(ID),
}
