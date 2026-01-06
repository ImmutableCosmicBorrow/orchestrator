use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

struct SendingSupportedCombinationRequest {
    to_explorer_struct: ToExplorerStruct,
}
impl SendingSupportedCombinationRequest {
    fn new(to_explorer_struct: ToExplorerStruct) -> Self {
        Self { to_explorer_struct }
    }
}

struct WaitingSupportedCombinationResult;

struct SupportedCombinationConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag>
    for SupportedCombinationConversation<SendingSupportedCombinationRequest>
{
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
            .to_explorer(OrchestratorToExplorer::SupportedCombinationRequest)
        {
            Ok(_) => {
                let next_state = SupportedCombinationConversation::<
                    WaitingSupportedCombinationResult,
                >::new(self.id);
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

impl SupportedCombinationConversation<SendingSupportedCombinationRequest> {
    fn new(id: ID, state: SendingSupportedCombinationRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

impl Conversation<ExplorerBag>
    for SupportedCombinationConversation<WaitingSupportedCombinationResult>
{
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
            ExplorerToOrchestrator::SupportedCombinationResult {
                explorer_id,
                combination_list,
            },
        )) = msg_wrapped
        {
            println!(
                "Supported combinations in explorer {explorer_id} current planet: {combination_list:?}"
            );
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }
}

impl SupportedCombinationConversation<WaitingSupportedCombinationResult> {
    fn new(id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::SupportedCombinationResult,
            )),
            state: WaitingSupportedCombinationResult,
        }
    }
}
