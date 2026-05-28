use crate::convo_manager::OrchContextRef;
use crate::globals::{TIMEOUT, get_explorer_timeout};
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::params::ConvoKind;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, ExplorerCommunicator,
    PossibleExpectedKinds, PossibleMessage,
};
use crate::orchestrator::conversations::{EntitiesIDTuple, UiCommunicator};
use crate::ui::OrchestratorToUiUpdate;
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;
use std::ops::Mul;
use std::time::Duration;

//**Supported Combination Conversation**
//
// This module manages the conversation between the Orchestrator and an Explorer regarding the combinations
// supported by the explorer's current planet.
// It uses a Finite State Machine (FSM) to ensure that the request for combinations and the subsequent
// result list are handled in the correct order at compile time.
//
// The conversation flow starts by sending a request to the explorer to get the planet supported combinations and terminates once the
// [`ExplorerToOrchestrator::SupportedCombinationResult`] is received and processed.

// --- SUPPORTED COMBINATION CONVERSATION ---
define_conversation!(
    name: SupportedCombinationConversation
);

// --- SEND SUPPORTED COMBINATION REQUEST DEFINITION ---
create_request_state!(
    state_name: SendingSupportedCombinationRequest,
    convo_kind: ConvoKind::SupportedCombination,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        explorer_id: ID,
    },
    entities_id_fn: |this: &SupportedCombinationConversation<SendingSupportedCombinationRequest>| { (None, Some(this.state.explorer_id)) },
    transition_fn: send_supp_comb_req_transition,
    methods_settings: {

    },
);

/// Transition Function for [`SendingSupportedCombinationRequest`] state:
///
/// Returns:
///
/// [`ErrorState`] with [`CommonErrorTypes::MessageToExplorerFailed`] if the request failed to send.
///
/// [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`] if the communication channel is missing.
///
/// The next state: [`SupportedCombinationConversation<WaitingSupportedCombinationResult>`] if the request was sent successfully.
fn send_supp_comb_req_transition(
    this: Box<SupportedCombinationConversation<SendingSupportedCombinationRequest>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this.state.to_explorer(
        this.state.explorer_id,
        OrchestratorToExplorer::SupportedCombinationRequest,
    ) {
        Ok(()) => {
            let state_struct = WaitingSupportedCombinationResult::new(
                this.state.orch_context,
                this.state.explorer_id,
            );
            let next_conv =
                SupportedCombinationConversation::<WaitingSupportedCombinationResult>::new(
                    this.id,
                    state_struct,
                );
            Some(Box::new(next_conv))
        }

        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAITING SUPPORTED COMBINATION RESPONSE DEFINITION

create_response_state!(
    state: WaitingSupportedCombinationResult,
    convo_kind: ConvoKind::SupportedCombination,
    timeout: Some(get_explorer_timeout().mul(2)),
    expected_msg: ExplorerToOrchKind(ExplorerToOrchestratorKind::SupportedCombinationResult),
    fields: {
        explorer_id: ID,
    },
    entities_id_closure: |this: &SupportedCombinationConversation<WaitingSupportedCombinationResult>| { (None, Some(this.state.explorer_id)) },
    transition: wait_supp_comb_res_transition,
    methods_settings: {

    },
);

/// Transition Function for [`WaitingSupportedCombinationResult`] state:
///
/// Returns:
///
/// [None] if the [`ExplorerToOrchestrator::SupportedCombinationResult`] is successfully received, closing the conversation.
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected result kind.
/// [`ErrorState`] with [`CommonErrorTypes::MessageToUiFailed`] if update to the UI failed
fn wait_supp_comb_res_transition(
    this: Box<SupportedCombinationConversation<WaitingSupportedCombinationResult>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::ExplorerToOrch(
        ExplorerToOrchestrator::SupportedCombinationResult {
            explorer_id,
            combination_list,
        },
    )) = msg
    {
        let combinations_log = format!("{combination_list:?}");

        //Try sending update to the UI
        return match this
            .state
            .to_ui(OrchestratorToUiUpdate::SupportedCombinations(
                explorer_id,
                combination_list,
            )) {
            Ok(()) => {
                log_internal(
                    LogTarget::Conversations,
                    Channel::Debug,
                    payload!(
                            action : "Sent Supported Combination to UI",
                            explorer_id : explorer_id,
                            conversation_id : this.id,
                            comb_list: combinations_log,
                    ),
                );
                None
            }

            Err(err) => {
                log_internal(
                    LogTarget::Conversations,
                    Channel::Warning,
                    payload!(
                        action : "Failed to send Supported Combination to UI",
                        explorer_id : explorer_id,
                        conversation_id : this.id
                    ),
                );
                let error_state = ErrorState::new(Box::new(err), this.id);
                Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
            }
        };
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
    use common_game::components::resource::ComplexResourceType;
    use crossbeam_channel::unbounded;
    use std::collections::HashSet;

    const CONV_ID: ID = 100;
    const EXPLORER_ID: ID = 200;

    fn make_send_conv(
        orch_context: OrchContextRef,
    ) -> Box<SupportedCombinationConversation<SendingSupportedCombinationRequest>> {
        let state = SendingSupportedCombinationRequest::new(orch_context, EXPLORER_ID);
        Box::new(SupportedCombinationConversation::<
            SendingSupportedCombinationRequest,
        >::new(CONV_ID, state))
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
    ) -> Box<SupportedCombinationConversation<WaitingSupportedCombinationResult>> {
        let state = WaitingSupportedCombinationResult::new(orch_context, EXPLORER_ID);
        Box::new(SupportedCombinationConversation::<
            WaitingSupportedCombinationResult,
        >::new(CONV_ID, state))
    }

    fn make_combination_list() -> HashSet<ComplexResourceType> {
        let mut combination_list = HashSet::new();
        combination_list.insert(ComplexResourceType::Water);
        combination_list.insert(ComplexResourceType::Robot);
        combination_list
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
            .expect("Should transition to WaitingSupportedCombinationResult");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::SupportedCombinationResult
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
            ConvoKind::SupportedCombination.priority().as_i32()
        );
    }

    #[test]
    fn wait_correct_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::SupportedCombinationResult {
                explorer_id: EXPLORER_ID,
                combination_list: make_combination_list(),
            });
        let result = conv.transition(Some(msg));
        assert!(
            result.is_none(),
            "Conversation should terminate upon receiving SupportedCombinationResult"
        );
    }

    #[test]
    fn wait_wrong_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let wrong_msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::StopExplorerAIResult {
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
        let state = WaitingSupportedCombinationResult::new(test_ctx.clone(), EXPLORER_ID);
        let conv = SupportedCombinationConversation::<WaitingSupportedCombinationResult>::new(
            CONV_ID, state,
        );
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::SupportedCombinationResult
            ))
        );
        assert_eq!(
            conv.get_priority(),
            ConvoKind::SupportedCombination.priority().as_i32()
        );
    }
}
