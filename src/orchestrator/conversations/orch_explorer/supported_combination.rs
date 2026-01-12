use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

///**Supported Combination Conversation**
///
/// This module manages the conversation between the Orchestrator and an Explorer regarding the combinations
/// supported by the explorer's current planet.
/// It uses a Finite State Machine (FSM) to ensure that the request for combinations and the subsequent
/// result list are handled in the correct order at compile time.
///
/// The conversation flow starts by sending a request to the explorer and terminates once the
/// [`ExplorerToOrchestrator::SupportedCombinationResult`] is received and processed.

/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingSupportedCombinationRequest`] state, which sends an
/// [`OrchestratorToExplorer::SupportedCombinationRequest`] when the [`Conversation::transition`] method is called.
struct SendingSupportedCombinationRequest {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
}

impl SendingSupportedCombinationRequest {
    /// Constructor for [`SendingSupportedCombinationRequest`] state struct
    fn new(to_explorer_struct: ToExplorerStruct) -> Self {
        Self { to_explorer_struct }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingSupportedCombinationResult`] state, the conversation expects an
/// [`ExplorerToOrchestrator::SupportedCombinationResult`] message containing the list of valid
/// recipes or combinations available to the explorer.
struct WaitingSupportedCombinationResult {
    /// ID of the explorer we are waiting for
    explorer_id: ID,
}

impl WaitingSupportedCombinationResult {
    /// The constructor for [`WaitingSupportedCombinationResult`] state struct
    fn new(explorer_id: ID) -> Self {
        Self { explorer_id }
    }
}

/// Supported Combination Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
struct SupportedCombinationConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING SUPPORTED COMBINATION REQUEST IMPLEMENTATION
impl Conversation<ExplorerBag>
    for SupportedCombinationConversation<SendingSupportedCombinationRequest>
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

    /// Transition Function for [`SendingSupportedCombinationRequest`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToExplorerFailed`] if the request failed to send.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`] if the communication channel is missing.
    ///
    /// The next state: [`SupportedCombinationConversation<WaitingSupportedCombinationResult>`] if the request was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::SupportedCombinationRequest)
        {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let next_state = SupportedCombinationConversation::<
                    WaitingSupportedCombinationResult,
                >::new(self.id, explorer_id);
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

impl SupportedCombinationConversation<SendingSupportedCombinationRequest> {
    /// The constructor for [`SupportedCombinationConversation`] in the [`SendingSupportedCombinationRequest`] state
    fn new(id: ID, state: SendingSupportedCombinationRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING SUPPORTED COMBINATION RESULT IMPLEMENTATION
impl Conversation<ExplorerBag>
    for SupportedCombinationConversation<WaitingSupportedCombinationResult>
{
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingSupportedCombinationResult`] state:
    ///
    /// Returns:
    ///
    /// [None] if the [`ExplorerToOrchestrator::SupportedCombinationResult`] is successfully received, closing the conversation.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected result kind.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
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

    fn get_priority(&self) -> i32 {
        2
    }
}

impl SupportedCombinationConversation<WaitingSupportedCombinationResult> {
    /// The constructor for [`SupportedCombinationConversation`] in the [`WaitingSupportedCombinationResult`] state
    fn new(id: ID, explorer_id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::SupportedCombinationResult,
            )),
            state: WaitingSupportedCombinationResult::new(explorer_id),
        }
    }
}
