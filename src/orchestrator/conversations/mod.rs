use crate::galaxy_setup::OrchPlanSenderMap;
use common_game::components::planet::Planet;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::protocols::planet_explorer::ExplorerToPlanet;
use common_game::utils::ID;
use crossbeam_channel::Sender;
use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

mod orch_explorer;
mod orch_planet;

trait Conversation<T: Debug> {
    fn get_id(&self) -> ID;
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds>;
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<T>>,
    ) -> Result<Option<Box<dyn Conversation<T>>>, (Option<Box<dyn Conversation<T>>>, String)>;
}

#[derive(Debug, Clone)]
enum PossibleExpectedKinds {
    PlanetToOrchKind(PlanetToOrchestratorKind),
    ExplorerToOrchKind(ExplorerToOrchestratorKind),
}

enum PossibleMessage<T> {
    PlanetToOrch(PlanetToOrchestrator),
    ExplorerToOrch(ExplorerToOrchestrator<T>),
    OrchToPlanet(OrchestratorToPlanet),
    OrchToExplorer(OrchestratorToExplorer),
}

pub(crate) trait ConversationWithPlanet {
    fn to_planet(&self) -> Result<(), String>;
}

pub(crate) type SendersToPlanet = Arc<Mutex<OrchPlanSenderMap>>;
pub(crate) type SendersToExplorer = Arc<Mutex<HashMap<ID, Sender<OrchestratorToExplorer>>>>;
pub(crate) type ExplorersBagRef<T> = Arc<HashMap<ID, T>>;
