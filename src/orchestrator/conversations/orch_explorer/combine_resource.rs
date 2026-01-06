use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use common_game::components::resource::ComplexResourceType;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

struct CraftingFailed {
    explorer_id: ID,
    err: String,
    resource: ComplexResourceType,
}

impl ErrorType for CraftingFailed {
    fn stringify(&self) -> String {
        format!(
            "Explorer {}, failed to craft {:?}: {}",
            self.explorer_id, self.resource, self.err
        )
    }
}
struct SendingCombineResourceRequest {
    to_explorer_struct: ToExplorerStruct,
    to_craft: ComplexResourceType,
}
impl SendingCombineResourceRequest {
    fn new(to_explorer_struct: ToExplorerStruct, to_craft: ComplexResourceType) -> Self {
        Self {
            to_explorer_struct,
            to_craft,
        }
    }
}

struct WaitingCombineResourceResult {
    to_craft: ComplexResourceType,
}

impl WaitingCombineResourceResult {
    fn new(to_craft: ComplexResourceType) -> Self {
        Self { to_craft }
    }
}

struct CombineResourceConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for CombineResourceConversation<SendingCombineResourceRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        match self.state.to_explorer_struct.to_explorer(
            OrchestratorToExplorer::CombineResourceRequest {
                to_generate: self.state.to_craft,
            },
        ) {
            Ok(_) => {
                let state_struct = WaitingCombineResourceResult::new(self.state.to_craft);
                let next_state = CombineResourceConversation::<WaitingCombineResourceResult>::new(
                    self.id,
                    state_struct,
                );
                Some(Box::new(next_state))
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
}

impl CombineResourceConversation<SendingCombineResourceRequest> {
    fn new(id: ID, state: SendingCombineResourceRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

impl Conversation<ExplorerBag> for CombineResourceConversation<WaitingCombineResourceResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::CombineResourceResponse {
                explorer_id,
                generated,
            },
        )) = msg_wrapped
        {
            return match generated {
                Ok(_) => {
                    println!(
                        "Explorer {explorer_id} generated {:?} resource correctly",
                        self.state.to_craft
                    );
                    None
                }
                Err(e) => {
                    let error_struct = CraftingFailed {
                        explorer_id,
                        err: e,
                        resource: self.state.to_craft,
                    };
                    let error_state = ErrorState::new(Box::new(error_struct), self.id);
                    Some(Box::new(error_state))
                }
            };
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }
}

impl CombineResourceConversation<WaitingCombineResourceResult> {
    fn new(id: ID, state: WaitingCombineResourceResult) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::CombineResourceResponse,
            )),
            state,
        }
    }
}
