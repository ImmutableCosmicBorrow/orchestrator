use crate::convo_manager::OrchContextRef;
use crate::globals::get_convo_timeout;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::conversations::ChannelsContext;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::params::ConvoKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ExplorerCommunicator, PossibleMessage,
};
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;
use std::time::Duration;

//**Stop Explorer Conversation**
//
// This module manages the conversation between the Orchestrator and an Explorer regarding the deactivation of its AI.
// It uses a Finite State Machine (FSM) to ensure that the stop command and the subsequent result
// are handled in the correct order at compile time.
//
// The conversation flow starts by sending a stop request to the explorer and terminates once the
// [`ExplorerToOrchestrator::StopExplorerAIResult`] is received.
// Marker struct for FSM state
//
// The conversation starts in the [`SendingExplorerStop`] state, which sends an
// [`OrchestratorToExplorer::StopExplorerAI`] when the [`Conversation::transition`] method is called.
// --- STOP EXPLORER CONVERSATION ---
define_conversation!(
    name: StopExplorerConversation
);

// --- SEND EXPLORER STOP DEFINITION ---
create_request_state!(
    state_name: SendingExplorerStop,
    conv_name: StopExplorerConversation,
    convo_kind: ConvoKind::StopExplorer,
    timeout: Some(get_convo_timeout()),
    expected_msg: None,
    fields: {
        explorer_id: ID,
    },
    entities_id_fn: |this: &StopExplorerConversation<SendingExplorerStop>| { (None, Some(this.state.explorer_id)) },
    transition_fn: send_explorer_stop_transition,
    methods_settings: {

    },
);

/// Transition Function for [`SendingExplorerStop`] state:
///
/// Returns:
///
/// [`ErrorState`] with [`CommonErrorTypes::MessageToExplorerFailed`] if the request failed to send.
///
/// [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`] if the communication channel is missing.
///
/// The next state: [`StopExplorerConversation<WaitingExplorerStopResult>`] if the stop command was sent successfully.
fn send_explorer_stop_transition(
    this: Box<StopExplorerConversation<SendingExplorerStop>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this.state.to_explorer(
        this.state.explorer_id,
        OrchestratorToExplorer::StopExplorerAI,
    ) {
        Ok(()) => {
            let next_state =
                WaitingExplorerStopResult::new(this.state.orch_context, this.state.explorer_id);
            let next_conv =
                StopExplorerConversation::<WaitingExplorerStopResult>::new(this.id, next_state);
            Some(Box::new(next_conv))
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAITING EXPLORER STOP DEFINITION ---

create_response_state!(
    state: WaitingExplorerStopResult,
    conv: StopExplorerConversation,
    convo_kind: ConvoKind::StopExplorer,
    timeout: Some(get_convo_timeout()),
    expected_msg: ExplorerToOrchKind(ExplorerToOrchestratorKind::StopExplorerAIResult),
    fields: {
        explorer_id: ID
    },
    entities_id_closure: |this: &StopExplorerConversation<WaitingExplorerStopResult>| { (None, Some(this.state.explorer_id)) },
    transition: wait_exp_stop_res_transition,
    methods_settings: {

    },
);

/// Transition Function for [`WaitingExplorerStopResult`] state:
///
/// Returns:
///
/// [None] if the [`ExplorerToOrchestrator::StopExplorerAIResult`] is successfully received, closing the conversation.
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected result kind.
fn wait_exp_stop_res_transition(
    this: Box<StopExplorerConversation<WaitingExplorerStopResult>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::StopExplorerAIResult {
        explorer_id,
    })) = msg
    {
        log_internal(
            LogTarget::Conversations,
            Channel::Info,
            payload!(
                action : "Stopped Explorer, closing conversation",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::orch_explorer::test_utils::{
        add_broken_explorer_sender, add_working_explorer_sender, make_test_context,
    };
    use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
    use crossbeam_channel::unbounded;

    const CONV_ID: ID = 100;
    const EXPLORER_ID: ID = 200;

    fn make_send_conv(
        orch_context: OrchContextRef,
    ) -> Box<StopExplorerConversation<SendingExplorerStop>> {
        let state = SendingExplorerStop::new(orch_context, EXPLORER_ID);
        Box::new(StopExplorerConversation::<SendingExplorerStop>::new(
            CONV_ID, state,
        ))
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
    ) -> Box<StopExplorerConversation<WaitingExplorerStopResult>> {
        let state = WaitingExplorerStopResult::new(orch_context, EXPLORER_ID);
        Box::new(StopExplorerConversation::<WaitingExplorerStopResult>::new(
            CONV_ID, state,
        ))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);
        let conv = make_send_conv(test_ctx.clone());
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
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_send_conv(test_ctx.clone());
        let next_conv = conv.transition(None).expect("Should return an ErrorState");
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("sender to explorer {EXPLORER_ID} not found"))
        );
    }

    #[test]
    fn send_message_failure() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        add_broken_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);
        let conv = make_send_conv(test_ctx.clone());
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
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);
        let conv = make_send_conv(test_ctx.clone());
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(
            conv.get_priority(),
            ConvoKind::StopExplorer.priority().as_i32()
        );
    }

    #[test]
    fn wait_correct_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
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
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
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
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let state = WaitingExplorerStopResult::new(test_ctx.clone(), EXPLORER_ID);
        let conv = StopExplorerConversation::<WaitingExplorerStopResult>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::StopExplorerAIResult
            ))
        );
        assert_eq!(
            conv.get_priority(),
            ConvoKind::StopExplorer.priority().as_i32()
        );
    }
}
