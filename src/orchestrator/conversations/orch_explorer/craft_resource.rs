use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use common_game::components::resource::BasicResourceType;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

struct CraftingFailed {
    explorer_id: ID,
    err: String,
    resource: BasicResourceType,
}

impl ErrorType for CraftingFailed {
    fn stringify(&self) -> String {
        format!(
            "Explorer {}, failed to craft {:?}: {}",
            self.explorer_id, self.resource, self.err
        )
    }
}
struct SendingCraftResourceRequest {
    to_explorer_struct: ToExplorerStruct,
    to_craft: BasicResourceType,
}
impl SendingCraftResourceRequest {
    fn new(to_explorer_struct: ToExplorerStruct, to_craft: BasicResourceType) -> Self {
        Self {
            to_explorer_struct,
            to_craft,
        }
    }
}

struct WaitingCraftResourceResult {
    explorer_id: ID,
    to_craft: BasicResourceType,
}

impl WaitingCraftResourceResult {
    fn new(explorer_id: ID, to_craft: BasicResourceType) -> Self {
        Self {
            explorer_id,
            to_craft,
        }
    }
}

struct CraftResourceConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for CraftResourceConversation<SendingCraftResourceRequest> {
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
        match self.state.to_explorer_struct.to_explorer(
            OrchestratorToExplorer::GenerateResourceRequest {
                to_generate: self.state.to_craft,
            },
        ) {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let state_struct =
                    WaitingCraftResourceResult::new(explorer_id, self.state.to_craft);
                let next_state = CraftResourceConversation::<WaitingCraftResourceResult>::new(
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

    fn get_priority(&self) -> i32 {
        2
    }
}

impl CraftResourceConversation<SendingCraftResourceRequest> {
    fn new(id: ID, state: SendingCraftResourceRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

impl Conversation<ExplorerBag> for CraftResourceConversation<WaitingCraftResourceResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::GenerateResourceResponse {
                explorer_id,
                generated,
            },
        )) = msg_wrapped
        {
            return match generated {
                Ok(()) => {
                    println!("Explorer {explorer_id} generated the requested resource correctly");
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

    fn get_priority(&self) -> i32 {
        2
    }
}

impl CraftResourceConversation<WaitingCraftResourceResult> {
    fn new(id: ID, state: WaitingCraftResourceResult) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::GenerateResourceResponse,
            )),
            state,
        }
    }
}
