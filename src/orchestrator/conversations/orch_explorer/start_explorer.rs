use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

struct SendingExplorerStart {
    to_explorer_struct: ToExplorerStruct,
}
impl SendingExplorerStart {
    fn new(to_explorer_struct: ToExplorerStruct) -> Self {
        Self { to_explorer_struct }
    }
}

struct WaitingExplorerStartResult;

struct StartExplorerConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for StartExplorerConversation<SendingExplorerStart> {
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
        match self
            .state
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::StartExplorerAI)
        {
            Ok(_) => {
                let next_state =
                    StartExplorerConversation::<WaitingExplorerStartResult>::new(self.id);
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

impl StartExplorerConversation<SendingExplorerStart> {
    fn new(id: ID, state: SendingExplorerStart) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

impl Conversation<ExplorerBag> for StartExplorerConversation<WaitingExplorerStartResult> {
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
            ExplorerToOrchestrator::StartExplorerAIResult { explorer_id },
        )) = msg_wrapped
        {
            println!("Started Explorer {explorer_id}");
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }
}

impl StartExplorerConversation<WaitingExplorerStartResult> {
    fn new(id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::StartExplorerAIResult,
            )),
            state: WaitingExplorerStartResult,
        }
    }
}
