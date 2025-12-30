use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestratorKind};
use common_game::utils::ID;
use crossbeam_channel::Sender;
use std::marker::PhantomData;

//TODO: ADD TRAIT
//TODO: ADD TIMEOUTS
//TODO: CHANGE LOGIC TO COMPLY WITH HAVING A PRIORITY QUEUE

struct GotTravelRequest;
struct WaitPlanetAcks;
struct WaitMoveToPlanetResponse;

enum ExpectedMessageKind {
    PlanetToOrchestrator(PlanetToOrchestratorKind),
    ExplorerToOrchestrator(ExplorerToOrchestratorKind),
}

enum LandingStates {
    WaitAcks(MoveToPlanetConversation<WaitPlanetAcks>),
    WaitMoveResult(MoveToPlanetConversation<WaitMoveToPlanetResponse>),
}

struct MoveToPlanetConversation<S> {
    _state: PhantomData<S>,
    expected_message: ExpectedMessageKind,
    to_planet: Sender<OrchestratorToPlanet>,
    //TODO: ADD OTHER PLANET SENDER
    to_explorer: Sender<OrchestratorToExplorer>,
    incoming_ack: bool,
    outgoing_ack: bool,
    curr_planet_id: ID,
    dst_planet_id: ID,
}

impl MoveToPlanetConversation<GotTravelRequest> {
    fn new() -> Self {
        todo!()
    }

    fn check_neighbors(self) -> LandingStates {
        todo!()
    }
}
