use crate::galaxy_setup::PlanetMap;
use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

pub(crate) struct WaitingExplorerNeighborsRequest {
    to_explorer_struct: ToExplorerStruct,
    galaxy: PlanetMap,
}

impl WaitingExplorerNeighborsRequest {
    pub(crate) fn new(to_explorer_struct: ToExplorerStruct, galaxy: PlanetMap) -> Self {
        Self {
            to_explorer_struct,
            galaxy,
        }
    }
}

struct PlanetNotFound(ID);
impl ErrorType for PlanetNotFound {
    fn stringify(&self) -> String {
        format!("Planet {} not found in current galaxy", self.0)
    }
}

struct SendingNeighborsResponse {
    to_explorer_struct: ToExplorerStruct,
    neighbors: Vec<ID>,
}
impl SendingNeighborsResponse {
    fn new(to_explorer_struct: ToExplorerStruct, neighbors: Vec<ID>) -> Self {
        Self {
            to_explorer_struct,
            neighbors,
        }
    }
}

pub(crate) struct NeighborsDiscoveryConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for NeighborsDiscoveryConversation<SendingNeighborsResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_explorer_struct.explorer_id
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
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::NeighborsResponse {
                neighbors: self.state.neighbors,
            }) {
            Ok(()) => {
                println!(
                    "Explorer {} obtained its neighbors correctly",
                    self.state.to_explorer_struct.explorer_id
                );
                None
            }
            Err(err) => {
                let error = match err {
                    ToExplorerError::SendingMessageFailure(id) => {
                        CommonErrorTypes::MessageToExplorerFailed(id)
                    }
                    ToExplorerError::SenderNotFound(id) => {
                        CommonErrorTypes::ExplorerSenderNotFound(id)
                    }
                };
                let error_state = ErrorState::new(Box::new(error), self.id);
                Some(Box::new(error_state))
            }
        }
    }

    fn get_priority(&self) -> i32 {
        3
    }
}

impl NeighborsDiscoveryConversation<SendingNeighborsResponse> {
    fn new(id: ID, state: SendingNeighborsResponse) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

impl Conversation<ExplorerBag> for NeighborsDiscoveryConversation<WaitingExplorerNeighborsRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::NeighborsRequest {
            explorer_id: _explorer_id,
            current_planet_id,
        })) = msg_wrapped
        {
            return match self.get_neighbors(current_planet_id) {
                Ok(neighbors) => {
                    let state_struct =
                        SendingNeighborsResponse::new(self.state.to_explorer_struct, neighbors);
                    let next_state =
                        NeighborsDiscoveryConversation::<SendingNeighborsResponse>::new(
                            self.id,
                            state_struct,
                        );
                    Some(Box::new(next_state))
                }
                Err(err) => {
                    let error_struct = ErrorState::new(err, self.id);
                    Some(Box::new(error_struct))
                }
            };
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        3
    }
}

impl NeighborsDiscoveryConversation<WaitingExplorerNeighborsRequest> {
    pub(crate) fn new(id: ID, state: WaitingExplorerNeighborsRequest) -> Self {
        Self {
            id,
            expected_message: Some(ExplorerToOrchKind(
                ExplorerToOrchestratorKind::NeighborsRequest,
            )),
            state,
        }
    }

    fn get_neighbors(
        &self,
        curr_planet_id: ID,
    ) -> Result<Vec<ID>, Box<dyn ErrorType + Send + Sync>> {
        if let Some(curr_planet_ref) = self.state.galaxy.lock().unwrap().get(&curr_planet_id) {
            let neighbors = curr_planet_ref.lock().unwrap().get_neighbors();
            Ok(neighbors)
        } else {
            Err(Box::new(PlanetNotFound(curr_planet_id)))
        }
    }
}
