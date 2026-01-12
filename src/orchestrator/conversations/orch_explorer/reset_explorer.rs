use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

///**Reset Explorer Conversation**
///
/// This module manages the conversation between the Orchestrator and an Explorer regarding the reset of its AI meaning it will reset all the knowledge it already acquired.
/// It uses a Finite State Machine (FSM) to ensure that the reset command and the subsequent result
/// are handled in the correct order at compile time.
///
/// The conversation flow starts by sending a reset request to the explorer and terminates once the
/// [`ExplorerToOrchestrator::ResetExplorerAIResult`] is received.

/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingExplorerReset`] state, which sends an
/// [`OrchestratorToExplorer::ResetExplorerAI`] when the [`Conversation::transition`] method is called.
struct SendingExplorerReset {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
}

impl SendingExplorerReset {
    /// Constructor for [`SendingExplorerReset`] state struct
    fn new(to_explorer_struct: ToExplorerStruct) -> Self {
        Self { to_explorer_struct }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingExplorerResetResult`] state, the conversation expects an
/// [`ExplorerToOrchestrator::ResetExplorerAIResult`] message to confirm the AI reset was successful.
struct WaitingExplorerResetResult {
    /// ID of the explorer we are waiting for
    explorer_id: ID,
}

impl WaitingExplorerResetResult {
    /// The constructor for [`WaitingExplorerResetResult`] state struct
    fn new(explorer_id: ID) -> Self {
        Self { explorer_id }
    }
}

/// Reset Explorer Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
struct ResetExplorerConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING EXPLORER RESET IMPLEMENTATION
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

    /// Transition Function for [`SendingExplorerReset`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToExplorerFailed`] if the request failed to send.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`] if the communication channel is missing.
    ///
    /// The next state: [`ResetExplorerConversation<WaitingExplorerResetResult>`] if the reset command was sent successfully.
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
    /// The constructor for [`ResetExplorerConversation`] in the [`SendingExplorerReset`] state
    fn new(id: ID, state: SendingExplorerReset) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING EXPLORER RESET RESULT IMPLEMENTATION
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

    /// Transition Function for [`WaitingExplorerResetResult`] state:
    ///
    /// Returns:
    ///
    /// [None] if the [`ExplorerToOrchestrator::ResetExplorerAIResult`] is successfully received, closing the conversation.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected result kind.
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
    /// The constructor for [`ResetExplorerConversation`] in the [`WaitingExplorerResetResult`] state
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
