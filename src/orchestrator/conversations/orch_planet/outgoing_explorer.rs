use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::orch_explorer::kill_explorers_manager::KillExplorersManager;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError, ToPlanetStruct,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;

struct FailedToHandleOutgoingExplorer {
    planet_id: ID,
    explorer_id: ID,
}
impl ErrorType for FailedToHandleOutgoingExplorer {
    fn stringify(&self) -> String {
        format!(
            "Planet {} failed to handle outgoing explorer {}",
            self.planet_id, self.explorer_id
        )
    }
}
pub(crate) struct SendingOutgoingRequest {
    to_planet_struct: ToPlanetStruct,
    outgoing_explorer_id: ID,
    kill_explorers_manager: Box<KillExplorersManager>,
}

impl SendingOutgoingRequest {
    pub(crate) fn new(
        to_planet_struct: ToPlanetStruct,
        outgoing_explorer_id: ID,
        kill_explorers_manager: Box<KillExplorersManager>,
    ) -> Self {
        Self {
            to_planet_struct,
            outgoing_explorer_id,
            kill_explorers_manager,
        }
    }
}

struct WaitingOutgoingResponse {
    planet_id: ID,
    kill_explorers_manager: Box<KillExplorersManager>,
}

impl WaitingOutgoingResponse {
    fn new(planet_id: ID, kill_explorers_manager: Box<KillExplorersManager>) -> Self {
        Self {
            planet_id,
            kill_explorers_manager,
        }
    }
}

pub(crate) struct OutgoingExplorer<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for OutgoingExplorer<SendingOutgoingRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_planet_struct.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::OutgoingExplorerRequest {
                explorer_id: self.state.outgoing_explorer_id,
            }) {
            Ok(()) => {
                let planet_id = self.state.to_planet_struct.planet_id;
                let state_struct =
                    WaitingOutgoingResponse::new(planet_id, self.state.kill_explorers_manager);
                let next_state =
                    OutgoingExplorer::<WaitingOutgoingResponse>::new(self.id, state_struct);
                Some(Box::new(next_state))
            }
            Err(err) => {
                let error = match err {
                    ToPlanetError::SendingMessageFailure(id) => {
                        CommonErrorTypes::MessageToPlanetFailed(id)
                    }
                    ToPlanetError::SenderNotFound(id) => CommonErrorTypes::PlanetSenderNotFound(id),
                };
                let error_state = ErrorState::new(Box::new(error), self.id);
                Some(Box::new(error_state))
            }
        }
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl OutgoingExplorer<SendingOutgoingRequest> {
    pub(crate) fn new(id: ID, state: SendingOutgoingRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

impl Conversation<ExplorerBag> for OutgoingExplorer<WaitingOutgoingResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::PlanetToOrch(
            PlanetToOrchestrator::OutgoingExplorerResponse {
                planet_id,
                explorer_id,
                res,
            },
        )) = msg_wrapped
        {
            return if res.is_ok() {
                println!(
                    "Planet {planet_id} correctly handled outgoing explorer {explorer_id}, going back to manager"
                );
                Some(self.state.kill_explorers_manager)
            } else {
                let error = FailedToHandleOutgoingExplorer {
                    planet_id,
                    explorer_id,
                };
                let error_state = ErrorState::new(Box::new(error), self.id);
                Some(Box::new(error_state))
            };
        }

        //Wrong Message, close conversation
        Some(self.state.kill_explorers_manager)
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl OutgoingExplorer<WaitingOutgoingResponse> {
    pub(crate) fn new(id: ID, state: WaitingOutgoingResponse) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::OutgoingExplorerResponse,
            )),
            state,
        }
    }
}
