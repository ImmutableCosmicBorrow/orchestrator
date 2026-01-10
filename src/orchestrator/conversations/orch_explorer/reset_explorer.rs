use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

struct SendingExplorerReset {
    to_explorer_struct: ToExplorerStruct,
}
impl SendingExplorerReset {
    fn new(to_explorer_struct: ToExplorerStruct) -> Self {
        Self { to_explorer_struct }
    }
}

struct WaitingExplorerResetResult {
    explorer_id: ID,
}

impl WaitingExplorerResetResult {
    fn new(explorer_id: ID) -> Self {
        Self { explorer_id }
    }
}

struct ResetExplorerConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for ResetExplorerConversation<SendingExplorerReset> {
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
            .to_explorer(OrchestratorToExplorer::ResetExplorerAI)
        {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let next_state = ResetExplorerConversation::<WaitingExplorerResetResult>::new(
                    self.id,
                    explorer_id,
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
        5
    }
}

impl ResetExplorerConversation<SendingExplorerReset> {
    fn new(id: ID, state: SendingExplorerReset) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

impl Conversation<ExplorerBag> for ResetExplorerConversation<WaitingExplorerResetResult> {
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
            ExplorerToOrchestrator::ResetExplorerAIResult { explorer_id },
        )) = msg_wrapped
        {
            println!("Reset Explorer {explorer_id}");
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        5
    }
}

impl ResetExplorerConversation<WaitingExplorerResetResult> {
    fn new(id: ID, explorer_id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::ResetExplorerAIResult,
            )),
            state: WaitingExplorerResetResult::new(explorer_id),
        }
    }
}
