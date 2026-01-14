use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

///**Supported Resources Conversation**
///
/// This module manages the conversation between the Orchestrator and an Explorer regarding
/// the resources supported by the explorer's current location.
/// It uses a Finite State Machine (FSM) to ensure that the resource query and the resulting
/// list are handled in the correct order at compile time.
///
/// The conversation flow starts by sending a request to the explorer and terminates once the
/// [`ExplorerToOrchestrator::SupportedResourceResult`] is received and processed.
/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingSupportedResourcesRequest`] state, which sends an
/// [`OrchestratorToExplorer::SupportedResourceRequest`] when the [`Conversation::transition`] method is called.
struct SendingSupportedResourcesRequest {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
}

impl SendingSupportedResourcesRequest {
    /// Constructor for [`SendingSupportedResourcesRequest`] state struct
    fn new(to_explorer_struct: ToExplorerStruct) -> Self {
        Self { to_explorer_struct }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingSupportedResourcesResult`] state, the conversation expects an
/// [`ExplorerToOrchestrator::SupportedResourceResult`] message containing the specific resources
/// that can be gathered or interacted with by the explorer.
struct WaitingSupportedResourcesResult {
    /// ID of the explorer we are waiting for
    explorer_id: ID,
}

impl WaitingSupportedResourcesResult {
    /// The constructor for [`WaitingSupportedResourcesResult`] state struct
    fn new(explorer_id: ID) -> Self {
        Self { explorer_id }
    }
}

/// Supported Resources Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
struct SupportedResourcesConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING SUPPORTED RESOURCES REQUEST IMPLEMENTATION
impl Conversation<ExplorerBag>
    for SupportedResourcesConversation<SendingSupportedResourcesRequest>
{
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingSupportedResourcesRequest`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToExplorerFailed`] if the request failed to send.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`] if the communication channel is missing.
    ///
    /// The next state: [`SupportedResourcesConversation<WaitingSupportedResourcesResult>`] if the request was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::SupportedResourceRequest)
        {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let next_state =
                    SupportedResourcesConversation::<WaitingSupportedResourcesResult>::new(
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
        2
    }
}

impl SupportedResourcesConversation<SendingSupportedResourcesRequest> {
    /// The constructor for [`SupportedResourcesConversation`] in the [`SendingSupportedResourcesRequest`] state
    fn new(id: ID, state: SendingSupportedResourcesRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING SUPPORTED RESOURCES RESULT IMPLEMENTATION
impl Conversation<ExplorerBag> for SupportedResourcesConversation<WaitingSupportedResourcesResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingSupportedResourcesResult`] state:
    ///
    /// Returns:
    ///
    /// [None] if the [`ExplorerToOrchestrator::SupportedResourceResult`] is successfully received, closing the conversation.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected result kind.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::SupportedResourceResult {
                explorer_id,
                supported_resources,
            },
        )) = msg_wrapped
        {
            println!(
                "Supported resources in explorer {explorer_id} current planet: {supported_resources:?}"
            );
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        2
    }
}

impl SupportedResourcesConversation<WaitingSupportedResourcesResult> {
    /// The constructor for [`SupportedResourcesConversation`] in the [`WaitingSupportedResourcesResult`] state
    fn new(id: ID, explorer_id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::SupportedResourceResult,
            )),
            state: WaitingSupportedResourcesResult::new(explorer_id),
        }
    }
}
