use crate::convo_manager::OrchContextRef;
use crate::globals::{TIMEOUT, get_explorer_timeout};
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
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

//**Supported Resources Conversation**
//
// This module manages the conversation between the Orchestrator and an Explorer regarding
// the resources supported by the explorer's current location.
// It uses a Finite State Machine (FSM) to ensure that the resource query and the resulting
// list are handled in the correct order at compile time.
//
// The conversation flow starts by sending a request to the explorer and terminates once the
// [`ExplorerToOrchestrator::SupportedResourceResult`] is received and processed.

// --- SUPPORTED RESOURCES CONVERSATION ---
define_conversation!(
    name: SupportedResourcesConversation
);

// --- SEND SUPPORTED RESOURCES REQUEST DEFINITION ---
create_request_state!(
    state_name: SendingSupportedResourcesRequest,
    conv_name: SupportedResourcesConversation,
    priority: 2,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        explorer_id: ID,
    },
    entities_id_fn: |this: &SupportedResourcesConversation<SendingSupportedResourcesRequest>| { (None, Some(this.state.explorer_id)) },
    transition_fn: send_supp_resources_req_transition,
    methods_settings: {

    },
);

/// Transition Function for [`SendingSupportedResourcesRequest`] state:
///
/// Returns:
///
/// [`ErrorState`] with [`CommonErrorTypes::MessageToExplorerFailed`] if the request failed to send.
///
/// [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`] if the communication channel is missing.
///
/// The next state: [`SupportedResourcesConversation<WaitingSupportedResourcesResult>`] if the request was sent successfully.
fn send_supp_resources_req_transition(
    this: Box<SupportedResourcesConversation<SendingSupportedResourcesRequest>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this.state.to_explorer(
        this.state.explorer_id,
        OrchestratorToExplorer::SupportedResourceRequest,
    ) {
        Ok(()) => {
            let state_struct = WaitingSupportedResourcesResult::new(
                this.state.orch_context,
                this.state.explorer_id,
            );
            let next_conv = SupportedResourcesConversation::<WaitingSupportedResourcesResult>::new(
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

// --- WAITING SUPPORTED RESOURCES RESPONSE DEFINITION

create_response_state!(
    state: WaitingSupportedResourcesResult,
    conv: SupportedResourcesConversation,
    priority: 2,
    timeout: Some(get_explorer_timeout().mul(2)),
    expected_msg: ExplorerToOrchKind(ExplorerToOrchestratorKind::SupportedResourceResult),
    fields: {
        explorer_id: ID,
    },
    entities_id_closure: |this: &SupportedResourcesConversation<WaitingSupportedResourcesResult>| { (None, Some(this.state.explorer_id)) },
    transition: wait_supp_resources_res_transition,
    methods_settings: {

    },
);

/// Transition Function for [`WaitingSupportedResourcesResult`] state:
///
/// Returns:
///
/// [None] if the [`ExplorerToOrchestrator::SupportedResourceResult`] is successfully received, closing the conversation.
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected result kind.
/// [`ErrorState`] with [`CommonErrorTypes::MessageToUiFailed`] if update to the UI failed
fn wait_supp_resources_res_transition(
    this: Box<SupportedResourcesConversation<WaitingSupportedResourcesResult>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::ExplorerToOrch(
        ExplorerToOrchestrator::SupportedResourceResult {
            explorer_id,
            supported_resources,
        },
    )) = msg
    {
        let resources_log = format!("{supported_resources:?}");
        // Send explorer snapshot to UI if sender is available

        //Try sending update to the UI
        return match this.state.to_ui(OrchestratorToUiUpdate::SupportedResources(
            explorer_id,
            supported_resources,
        )) {
            Ok(()) => {
                log_internal(
                    LogTarget::Conversations,
                    Channel::Debug,
                    payload!(
                            action : "Sent Supported Resources to UI",
                            explorer_id : explorer_id,
                            conversation_id : this.id,
                            comb_list: resources_log,
                    ),
                );
                None
            }

            Err(err) => {
                log_internal(
                    LogTarget::Conversations,
                    Channel::Warning,
                    payload!(
                        action : "Failed to send Supported Resources to UI",
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
    use crate::orchestrator::conversations::orch_explorer::test_utils::{ make_test_context, add_broken_explorer_sender, add_working_explorer_sender
    };
    use common_game::components::resource::BasicResourceType;
    use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
    use crossbeam_channel::unbounded;
    use std::collections::HashSet;

    const CONV_ID: ID = 100;
    const EXPLORER_ID: ID = 200;

    fn make_send_conv(
        orch_context: OrchContextRef,
    ) -> Box<SupportedResourcesConversation<SendingSupportedResourcesRequest>> {
        let state = SendingSupportedResourcesRequest::new(orch_context, EXPLORER_ID);
        Box::new(SupportedResourcesConversation::<
            SendingSupportedResourcesRequest,
        >::new(CONV_ID, state))
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
    ) -> Box<SupportedResourcesConversation<WaitingSupportedResourcesResult>> {
        let state = WaitingSupportedResourcesResult::new(orch_context, EXPLORER_ID);
        Box::new(SupportedResourcesConversation::<
            WaitingSupportedResourcesResult,
        >::new(CONV_ID, state))
    }

    #[test]
    fn send_success() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);
        let conv = make_send_conv(test_ctx.clone());
        let next_conv = conv
            .transition(None)
            .expect("Should transition to WaitingSupportedResourcesResult");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::SupportedResourceResult
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
    fn wait_correct_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let mut supported_resources = HashSet::new();
        supported_resources.insert(BasicResourceType::Carbon);
        supported_resources.insert(BasicResourceType::Oxygen);
        let msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::SupportedResourceResult {
                explorer_id: EXPLORER_ID,
                supported_resources,
            });
        let result = conv.transition(Some(msg));
        assert!(
            result.is_none(),
            "Conversation should terminate upon receiving SupportedResourceResult"
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
    fn send_getters() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);
        let conv = make_send_conv(test_ctx.clone());
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 2);
    }

    #[test]
    fn wait_getters() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let state =
            WaitingSupportedResourcesResult::new(test_ctx.clone(), EXPLORER_ID);
        let conv =
            SupportedResourcesConversation::<WaitingSupportedResourcesResult>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::SupportedResourceResult
            ))
        );
        assert_eq!(conv.get_priority(), 2);
    }
}