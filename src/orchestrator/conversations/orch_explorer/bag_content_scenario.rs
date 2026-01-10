use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

struct SendingBagContentRequest {
    to_explorer_struct: ToExplorerStruct,
}
impl SendingBagContentRequest {
    fn new(to_explorer_struct: ToExplorerStruct) -> Self {
        Self { to_explorer_struct }
    }
}

struct WaitingBagContentResponse {
    explorer_id: ID,
}

impl WaitingBagContentResponse {
    fn new(explorer_id: ID) -> Self {
        Self { explorer_id }
    }
}

struct BagContentConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for BagContentConversation<SendingBagContentRequest> {
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
            .to_explorer(OrchestratorToExplorer::BagContentRequest)
        {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let next_state =
                    BagContentConversation::<WaitingBagContentResponse>::new(self.id, explorer_id);
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
        3
    }
}

impl BagContentConversation<SendingBagContentRequest> {
    fn new(id: ID, state: SendingBagContentRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

impl Conversation<ExplorerBag> for BagContentConversation<WaitingBagContentResponse> {
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
        if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::BagContentResponse {
            explorer_id,
            bag_content,
        })) = msg_wrapped
        {
            //TODO: SEND THIS TO UI
            println!("Explorer {explorer_id} bag content: {bag_content:?}");
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        3
    }
}

impl BagContentConversation<WaitingBagContentResponse> {
    fn new(id: ID, explorer_id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::BagContentResponse,
            )),
            state: WaitingBagContentResponse::new(explorer_id),
        }
    }
}
