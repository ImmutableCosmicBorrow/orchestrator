use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::globals::{get_explorer_timeout, TIMEOUT};
use crate::logging_utils::{log_internal, LogTarget};
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::{ChannelsContext, CommonErrorTypes, Conversation, ErrorState, ExplorerCommunicator, ExplorerContext, PossibleExpectedKinds, PossibleMessage};
use crate::orchestrator::ChannelsManagerRef;
use crate::{create_request_state, create_response_state, define_conversation, payload};
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

// --- START EXPLORER CONVERSATION ---
define_conversation!(
    name: StartExplorerConversation
);

// --- SEND EXPLORER START DEFINITION ---
create_request_state!(
    state_name: SendingExplorerStart,
    conv_name: StartExplorerConversation,
    priority: 5,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        channels_manager: ChannelsManagerRef,
        explorer_id: ID,
    },
    entities_id_fn: |this: &StartExplorerConversation<SendingExplorerStart>| { (None, Some(this.state.explorer_id)) },
    transition_fn: send_explorer_start_transition,
    methods_settings: {

    },
);

impl ExplorerContext for SendingExplorerStart {
    fn get_explorer_id(&self) -> ID {
        self.explorer_id
    }
}

impl ChannelsContext for SendingExplorerStart {
    fn get_channels_manager(&self) -> &ChannelsManagerRef {
        &self.channels_manager
    }
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
fn send_explorer_start_transition(this: Box<StartExplorerConversation<SendingExplorerStart>>) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this
        .state
        .to_explorer(OrchestratorToExplorer::StartExplorerAI)
    {
        Ok(()) => {
            let next_state = WaitingExplorerStartResult::new(
                this.state.explorer_id
            );
            let next_conv = StartExplorerConversation::<WaitingExplorerStartResult>::new(
                this.id,
                next_state,
            );
            Some(Box::new(next_conv))
        }
        Err(err) => {
            let err_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(err_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}


// --- WAITING EXPLORER START DEFINITION ---

create_response_state!(
    state: WaitingExplorerStartResult,
    conv: StartExplorerConversation,
    priority: 5,
    timeout: Some(get_explorer_timeout()),
    expected_msg: ExplorerToOrchKind(ExplorerToOrchestratorKind::StartExplorerAIResult),
    fields: {
        explorer_id: ID
    },
    entities_id_closure: |this: &StartExplorerConversation<WaitingExplorerStartResult>| { (None, Some(this.state.explorer_id)) },
    transition: wait_exp_start_res_transition,
    methods_settings: {

    },
);



impl ExplorerContext for WaitingExplorerStartResult {
    fn get_explorer_id(&self) -> ID {
        self.explorer_id
    }
}

/// Transition Function for [`WaitingExplorerStartResult`] state:
///
/// Returns:
///
/// [None] if the [`ExplorerToOrchestrator::StartExplorerAIResult`] is successfully received, closing the conversation.
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected result kind.
fn wait_exp_start_res_transition(this: Box<StartExplorerConversation<WaitingExplorerStartResult>>, msg: Option<PossibleMessage>) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::ExplorerToOrch(
                    ExplorerToOrchestrator::StartExplorerAIResult { explorer_id },
    )) = msg
    {
        log_internal(
            LogTarget::Conversations,
            Channel::Info,
            payload!(
                    action : "Started Explorer, closing conversation",
                    explorer_id : explorer_id,
                    conversation_id : this.id
                ),
        );
        return None;
    }

    //Wrong Message, close conversation
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::orch_explorer::test_utils::{
        make_empty_senders, make_senders_with, make_to_explorer_struct, MakeSendersResult,
    };
    use crate::orchestrator::conversations::OrchToExplorerSenders;
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
*/