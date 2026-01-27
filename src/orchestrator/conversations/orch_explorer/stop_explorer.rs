use crate::globals::get_explorer_timeout;
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
use std::time::Duration;

///**Stop Explorer Conversation**
///
/// This module manages the conversation between the Orchestrator and an Explorer regarding the deactivation of its AI.
/// It uses a Finite State Machine (FSM) to ensure that the stop command and the subsequent result
/// are handled in the correct order at compile time.
///
/// The conversation flow starts by sending a stop request to the explorer and terminates once the
/// [`ExplorerToOrchestrator::StopExplorerAIResult`] is received.
/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingExplorerStop`] state, which sends an
/// [`OrchestratorToExplorer::StopExplorerAI`] when the [`Conversation::transition`] method is called.
pub(crate) struct SendingExplorerStop {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
}

impl SendingExplorerStop {
    /// Constructor for [`SendingExplorerStop`] state struct
    pub(crate) fn new(to_explorer_struct: ToExplorerStruct) -> Self {
        Self { to_explorer_struct }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingExplorerStopResult`] state, the conversation expects an
/// [`ExplorerToOrchestrator::StopExplorerAIResult`] message to confirm the AI has successfully halted.
pub(crate) struct WaitingExplorerStopResult {
    /// ID of the explorer we are waiting for
    explorer_id: ID,
}

impl WaitingExplorerStopResult {
    /// The constructor for [`WaitingExplorerStopResult`] state struct
    pub(crate) fn new(explorer_id: ID) -> Self {
        Self { explorer_id }
    }
}

/// Stop Explorer Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
pub(crate) struct StopExplorerConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING EXPLORER STOP IMPLEMENTATION
impl Conversation<ExplorerBag> for StopExplorerConversation<SendingExplorerStop> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, Some(self.state.to_explorer_struct.explorer_id))
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingExplorerStop`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToExplorerFailed`] if the request failed to send.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`] if the communication channel is missing.
    ///
    /// The next state: [`StopExplorerConversation<WaitingExplorerStopResult>`] if the stop command was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::StopExplorerAI)
        {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let next_state = StopExplorerConversation::<WaitingExplorerStopResult>::new(
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
                Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBag> + Send + Sync>)
            }
        }
    }

    fn get_priority(&self) -> i32 {
        5
    }
}

impl StopExplorerConversation<SendingExplorerStop> {
    /// The constructor for [`StopExplorerConversation`] in the [`SendingExplorerStop`] state
    pub(crate) fn new(id: ID, state: SendingExplorerStop) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING EXPLORER STOP RESULT IMPLEMENTATION
impl Conversation<ExplorerBag> for StopExplorerConversation<WaitingExplorerStopResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, Some(self.state.explorer_id))
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingExplorerStopResult`] state:
    ///
    /// Returns:
    ///
    /// [None] if the [`ExplorerToOrchestrator::StopExplorerAIResult`] is successfully received, closing the conversation.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected result kind.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::StopExplorerAIResult { explorer_id },
        )) = msg_wrapped
        {
            log_internal(
                Channel::Debug,
                payload!(
                    action : "Stopped Explorer, closing conversation",
                    explorer_id : explorer_id,
                    conversation_id : self.id
                ),
            );
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBag> + Send + Sync>)
    }

    fn get_priority(&self) -> i32 {
        5
    }

    // Longer timeout, since it involves a communication with an Explorer
    fn get_timeout(&self) -> Option<Duration> {
        Some(Duration::from_millis(get_explorer_timeout()))
    }
}

impl StopExplorerConversation<WaitingExplorerStopResult> {
    /// The constructor for [`StopExplorerConversation`] in the [`WaitingExplorerStopResult`] state
    fn new(id: ID, explorer_id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::StopExplorerAIResult,
            )),
            state: WaitingExplorerStopResult::new(explorer_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::SendersToExplorer;
    use crate::orchestrator::conversations::orch_explorer::test_utils::{
        MakeSendersResult, make_empty_senders, make_senders_with, make_to_explorer_struct,
    };
    use common_game::protocols::orchestrator_explorer::ExplorerToOrchestratorKind;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: ID = 100;
    const EXPLORER_ID: ID = 200;

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(
        senders: SendersToExplorer,
    ) -> Box<StopExplorerConversation<SendingExplorerStop>> {
        let to_explorer = make_to_explorer_struct(EXPLORER_ID, senders);
        let state = SendingExplorerStop::new(to_explorer);
        Box::new(StopExplorerConversation::<SendingExplorerStop>::new(
            CONV_ID, state,
        ))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv() -> Box<StopExplorerConversation<WaitingExplorerStopResult>> {
        Box::new(StopExplorerConversation::<WaitingExplorerStopResult>::new(
            CONV_ID,
            EXPLORER_ID,
        ))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let MakeSendersResult(senders, _rx) = make_senders_with(EXPLORER_ID);
        let conv = make_send_conv(senders);
        let next_conv = conv
            .transition(None)
            .expect("Should transition to WaitingExplorerStopResult");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::StopExplorerAIResult
            ))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn send_missing_sender() {
        let senders = make_empty_senders();
        let conv = make_send_conv(senders);
        let next_conv = conv.transition(None).expect("Should return an ErrorState");
        assert!(next_conv.get_expected_kind().is_none());
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
        let state = SendingExplorerStop::new(to_explorer);
        let conv = StopExplorerConversation::<SendingExplorerStop>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 5);
    }

    #[test]
    fn wait_correct_message() {
        let conv = make_wait_conv();
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::StopExplorerAIResult {
            explorer_id: EXPLORER_ID,
        });
        let result = conv.transition(Some(msg));
        assert!(
            result.is_none(),
            "Conversation should terminate upon receiving StopExplorerAIResult"
        );
    }

    #[test]
    fn wait_wrong_message() {
        let conv = make_wait_conv();
        let wrong_msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::StartExplorerAIResult {
                explorer_id: EXPLORER_ID,
            });
        let result = conv
            .transition(Some(wrong_msg))
            .expect("Should transition to ErrorState");
        assert_eq!(result.get_id(), CONV_ID);
        assert_eq!(
            result.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }

    #[test]
    fn wait_getters() {
        let conv = StopExplorerConversation::<WaitingExplorerStopResult>::new(CONV_ID, EXPLORER_ID);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::StopExplorerAIResult
            ))
        );
        assert_eq!(conv.get_priority(), 5);
    }
}
