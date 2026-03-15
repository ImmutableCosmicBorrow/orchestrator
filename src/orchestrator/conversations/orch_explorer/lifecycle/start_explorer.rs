use crate::globals::get_explorer_timeout;
use crate::logging_utils::{LogTarget, log_internal};
use crate::orchestrator::ExplorerBagContent;
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
use std::time::Duration;

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
pub(crate) struct SendingExplorerStart {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
}

impl SendingExplorerStart {
    /// Constructor for [`SendingExplorerStart`] state struct
    pub(crate) fn new(to_explorer_struct: ToExplorerStruct) -> Self {
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
pub(crate) struct StartExplorerConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING EXPLORER START IMPLEMENTATION
impl Conversation for StartExplorerConversation<SendingExplorerStart> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, Some(self.state.to_explorer_struct.explorer_id))
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
        _msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation + Send + Sync>> {
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
                Some(Box::new(error_state)
                    as Box<dyn Conversation + Send + Sync>)
            }
        }
    }

    fn get_priority(&self) -> i32 {
        5
    }
}

impl StartExplorerConversation<SendingExplorerStart> {
    /// The constructor for [`StartExplorerConversation`] in the [`SendingExplorerStart`] state
    pub(crate) fn new(id: ID, state: SendingExplorerStart) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING EXPLORER START RESULT IMPLEMENTATION
impl Conversation for StartExplorerConversation<WaitingExplorerStartResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, Some(self.state.explorer_id))
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
        msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::StartExplorerAIResult { explorer_id },
        )) = msg_wrapped
        {
            log_internal(
                LogTarget::Conversations,
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
        Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
    }

    fn get_priority(&self) -> i32 {
        5
    }

    // Longer timeout, since it involves a communication with an Explorer
    fn get_timeout(&self) -> Option<Duration> {
        Some(get_explorer_timeout())
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
    use crate::orchestrator::conversations::OrchToExplorerSenders;
    use crate::orchestrator::conversations::orch_explorer::test_utils::{
        MakeSendersResult, make_empty_senders, make_senders_with, make_to_explorer_struct,
    };
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: u32 = 1;
    const EXPLORER_ID: u32 = 2;

    // --- Helper functions ---

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(
        senders: OrchToExplorerSenders,
    ) -> Box<StartExplorerConversation<SendingExplorerStart>> {
        let to_explorer = make_to_explorer_struct(EXPLORER_ID, senders);
        let state = SendingExplorerStart::new(to_explorer);
        Box::new(StartExplorerConversation::<SendingExplorerStart>::new(
            CONV_ID, state,
        ))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv() -> Box<StartExplorerConversation<WaitingExplorerStartResult>> {
        Box::new(StartExplorerConversation::<WaitingExplorerStartResult>::new(CONV_ID, EXPLORER_ID))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let MakeSendersResult(senders, _rx) = make_senders_with(EXPLORER_ID);
        let conv = make_send_conv(senders);
        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");
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
        let senders = make_empty_senders();
        let conv = make_send_conv(senders);
        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("sender to explorer {EXPLORER_ID} not found"))
        );
    }

    #[test]
    fn send_message_failure() {
        let (tx, rx) = unbounded::<OrchestratorToExplorer>();
        drop(rx);
        let senders = Arc::new(Mutex::new(HashMap::from([(EXPLORER_ID, tx)])));
        let conv = make_send_conv(senders);
        let next_conv = conv.transition(None).expect("Should return an ErrorState");
        let error_msg = next_conv
            .get_error_details()
            .expect("Should return an Error Details String");
        assert_eq!(
            error_msg,
            format!("failed to send message to explorer {EXPLORER_ID}")
        );
    }

    #[test]
    fn send_getters() {
        let MakeSendersResult(senders, _rx) = make_senders_with(EXPLORER_ID);
        let to_explorer = make_to_explorer_struct(EXPLORER_ID, senders);
        let state = SendingExplorerStart::new(to_explorer);
        let conv = StartExplorerConversation::<SendingExplorerStart>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 5);
    }

    #[test]
    fn wait_correct_transition() {
        let conv = make_wait_conv();
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::StartExplorerAIResult {
            explorer_id: EXPLORER_ID,
        });
        let result = conv.transition(Some(msg));
        assert!(
            result.is_none(),
            "Conversation should terminate successfully (None)"
        );
    }

    #[test]
    fn wait_wrong_message() {
        let conv = make_wait_conv();
        let wrong_msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::ResetExplorerAIResult {
                explorer_id: EXPLORER_ID,
            });
        let result = conv
            .transition(Some(wrong_msg))
            .expect("Should return an ErrorState");
        assert_eq!(result.get_id(), CONV_ID);
        assert_eq!(
            result.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }

    #[test]
    fn wait_getters() {
        let conv =
            StartExplorerConversation::<WaitingExplorerStartResult>::new(CONV_ID, EXPLORER_ID);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::StartExplorerAIResult
            ))
        );
        assert_eq!(conv.get_priority(), 5);
    }
}
