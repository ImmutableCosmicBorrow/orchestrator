use crate::logging_utils::log_internal;
use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use crate::payload;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

///**Start Explorer Conversation**
///
/// This module manages the conversation between the Orchestrator and an Explorer regarding the activation of its AI.
/// It uses a Finite State Machine (FSM) to ensure that the start command and the subsequent result
/// are handled in the correct order at compile time.
///
/// The conversation flow starts by sending a start request to the explorer and terminates once the
/// [`ExplorerToOrchestrator::StartExplorerAIResult`] is received.
/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingExplorerStart`] state, which sends an
/// [`OrchestratorToExplorer::StartExplorerAI`] when the [`Conversation::transition`] method is called.
struct SendingExplorerStart {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
}

impl SendingExplorerStart {
    /// Constructor for [`SendingExplorerStart`] state struct
    fn new(to_explorer_struct: ToExplorerStruct) -> Self {
        Self { to_explorer_struct }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingExplorerStartResult`] state, the conversation expects an
/// [`ExplorerToOrchestrator::StartExplorerAIResult`] message to confirm the AI has successfully initialized.
struct WaitingExplorerStartResult {
    /// ID of the explorer we are waiting for
    explorer_id: ID,
}

impl WaitingExplorerStartResult {
    /// The constructor for [`WaitingExplorerStartResult`] state struct
    fn new(explorer_id: ID) -> Self {
        Self { explorer_id }
    }
}

/// Start Explorer Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
struct StartExplorerConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING EXPLORER START IMPLEMENTATION
impl Conversation<ExplorerBag> for StartExplorerConversation<SendingExplorerStart> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingExplorerStart`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToExplorerFailed`] if the request failed to send.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`] if the communication channel is missing.
    ///
    /// The next state: [`StartExplorerConversation<WaitingExplorerStartResult>`] if the start command was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::StartExplorerAI)
        {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let next_state = StartExplorerConversation::<WaitingExplorerStartResult>::new(
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

impl StartExplorerConversation<SendingExplorerStart> {
    /// The constructor for [`StartExplorerConversation`] in the [`SendingExplorerStart`] state
    fn new(id: ID, state: SendingExplorerStart) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING EXPLORER START RESULT IMPLEMENTATION
impl Conversation<ExplorerBag> for StartExplorerConversation<WaitingExplorerStartResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingExplorerStartResult`] state:
    ///
    /// Returns:
    ///
    /// [None] if the [`ExplorerToOrchestrator::StartExplorerAIResult`] is successfully received, closing the conversation.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected result kind.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::StartExplorerAIResult { explorer_id },
        )) = msg_wrapped
        {
            log_internal(
                Channel::Info,
                payload!(
                    action : "Started Explorer, closing conversation",
                    explorer_id : explorer_id,
                    conversation_id : self.id
                ),
            );
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

impl StartExplorerConversation<WaitingExplorerStartResult> {
    /// The constructor for [`StartExplorerConversation`] in the [`WaitingExplorerStartResult`] state
    fn new(id: ID, explorer_id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::StartExplorerAIResult,
            )),
            state: WaitingExplorerStartResult::new(explorer_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: u32 = 1;
    const EXPLORER_ID: u32 = 2;
    #[test]
    fn send_success() {
        let (tx, _rx) = unbounded::<OrchestratorToExplorer>();
        //adding the correct channel
        let senders_to_explorers = Arc::new(Mutex::new(HashMap::from([(EXPLORER_ID, tx.clone())])));

        // We wrap this in the SendingExplorerStart state
        // Note: You'll need to satisfy the ToExplorerStruct requirements
        let to_explorer = ToExplorerStruct {
            explorer_id: EXPLORER_ID,
            explorers_senders: senders_to_explorers,
        };
        let state = SendingExplorerStart::new(to_explorer);
        let conv = Box::new(StartExplorerConversation::<SendingExplorerStart>::new(
            CONV_ID, state,
        ));

        // Act: Transition from the first state
        // The first transition sends the message and moves to 'Waiting'
        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");

        // Assert: Check if the expected message kind is now set correctly
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::StartExplorerAIResult
            ))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
    }

    #[test]
    fn send_missing_sender() {
        // Empty senders map to test missing sender error
        let senders_to_explorers = Arc::new(Mutex::new(HashMap::new()));

        // We wrap this in the SendingExplorerStart state
        let to_explorer = ToExplorerStruct {
            explorer_id: EXPLORER_ID,
            explorers_senders: senders_to_explorers,
        };
        let state = SendingExplorerStart::new(to_explorer);
        let conv = Box::new(StartExplorerConversation::<SendingExplorerStart>::new(
            CONV_ID, state,
        ));

        // Act: Transition from the first state
        // The first transition sends the message and moves to 'Waiting'
        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");

        // Assert: Error state no expected message
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(next_conv.get_id(), CONV_ID);
        //ASSERT: IT IS THE RIGHT ERROR
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("sender to explorer {EXPLORER_ID} not found"))
        );
    }

    #[test]
    fn send_message_failure() {
        // 1. Create the channel
        let (tx, rx) = unbounded::<OrchestratorToExplorer>();

        // 2. Drop the receiver immediately
        // This makes the channel "disconnected". Any future send attempts will fail.
        drop(rx);

        let senders_to_explorers = Arc::new(Mutex::new(HashMap::from([(EXPLORER_ID, tx)])));

        let to_explorer = ToExplorerStruct {
            explorer_id: EXPLORER_ID,
            explorers_senders: senders_to_explorers,
        };

        let state = SendingExplorerStart::new(to_explorer);
        let conv = Box::new(StartExplorerConversation::<SendingExplorerStart>::new(
            CONV_ID, state,
        ));

        // 3. Act: This should now trigger the Err(ToExplorerError::SendingMessageFailure) branch
        let next_conv = conv.transition(None).expect("Should return an ErrorState");

        // 4. Assert: Check for the specific MessageToExplorerFailed error
        // Note: The string here depends on how your CommonErrorTypes handles the "MessageToExplorerFailed" variant
        let error_msg = next_conv
            .get_error_details()
            .expect("Should return an Error Details String");
        assert_eq!(
            error_msg,
            format!("failed to send message to explorer {EXPLORER_ID}")
        );
    }

    #[test]
    fn wait_correct_transition() {
        // Start directly in the Waiting state to test the end of the FSM
        let conv = Box::new(
            StartExplorerConversation::<WaitingExplorerStartResult>::new(CONV_ID, EXPLORER_ID),
        );

        // Mock the incoming message from the explorer
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::StartExplorerAIResult {
            explorer_id: EXPLORER_ID,
        });

        // Act: Transition with the correct message
        let result = conv.transition(Some(msg));

        // Assert: Successful completion should return None
        assert!(
            result.is_none(),
            "Conversation should terminate successfully (None)"
        );
    }

    #[test]
    fn wait_wrong_message() {
        let conv = Box::new(
            StartExplorerConversation::<WaitingExplorerStartResult>::new(CONV_ID, EXPLORER_ID),
        );

        // Mock an irrelevant message (e.g., a different explorer message if available)
        // Using a dummy/wrong message variant here
        let wrong_msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::ResetExplorerAIResult {
                explorer_id: EXPLORER_ID,
            });

        // Act
        let result = conv
            .transition(Some(wrong_msg))
            .expect("Should return an ErrorState");

        // Assert: We expect an ErrorState, not None
        assert_eq!(result.get_id(), CONV_ID);
        // Assert: It is the right kind of error
        assert_eq!(
            result.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }
}
