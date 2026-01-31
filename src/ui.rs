use std::collections::HashSet;

use crate::ExplorerType;
use crate::orchestrator::ExplorersLocationRef;
use common_explorer::ExplorerBagContent;
use common_game::components::planet::DummyPlanetState;
use common_game::{
    components::resource::{BasicResourceType, ComplexResourceType},
    utils::ID,
};

use crate::planet::PlanetMap;

// Commands you can send to the orchestrator
pub enum UiToOrchestratorCommand {
    //rendering commands
    GetGalaxy,
    AddPlanet(ID, Vec<ID>), //new planet id and connected planet ids
    GetExplorersPosition,
    GetPlanetSnapshot(ID),
    GetExplorerSnapshot(ID),
    ///explorer type, planet id
    AddExplorer(ExplorerType, ID), 
    SwitchGameMode,
    EndGame,
    PauseGame,
    ResumeGame,

    // explorer commands: move and resource crafting/combining
    ///Explorer ID, current planet, dst planet
    ManualMoveExplorer(ID, ID, ID), 
    ManualExplorerCraftsRes(ID, BasicResourceType),
    ManualExplorerCombineRes(ID, ComplexResourceType),
    SupportedCombinations(ID),
    SupportedResources(ID),

    // asteroid/sunrays commands
    SendManualAsteroid(ID),
    SendManualSunray(ID),

    // start/stop/reset/kill AI commands
    StartPlanetAI(ID),
    StopPlanetAI(ID),
    ResetPlanetAI(ID),
    StartExplorerAI(ID),
    StopExplorerAI(ID),
    ResetExplorerAI(ID),
    KillExplorer(ID),
    KillPlanet(ID),
}

// Updates the orchestrator sends back
pub enum OrchestratorToUiUpdate {
    //rendering commands
    Galaxy(PlanetMap),
    DeadPlanet(ID),
    ExplorersPosition(ExplorersLocationRef),
    PlanetSnapshot(ID, DummyPlanetState),
    ExplorerSnapshot(ID, ExplorerBagContent),

    // explorer commands: move and resource crafting/combining
    AutoMoveExplorer(ID, ID, ID),
    AutoExplorerCraftsRes(ID, BasicResourceType),
    AutoExplorerCombineRes(ID, ComplexResourceType),
    SupportedCombinations(ID, HashSet<ComplexResourceType>),
    SupportedResources(ID, HashSet<BasicResourceType>),

    // asteroid/sunrays commands
    SendAutoSunray(ID),
    SendAutoAsteroid(ID),
}
