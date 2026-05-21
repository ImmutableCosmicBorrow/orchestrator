use crate::convo_manager::OrchContextRef;
use crate::globals::{TIMEOUT, get_explorer_timeout};
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds,
    PossibleMessage,
};
use crate::orchestrator::conversations::{EntitiesIDTuple, ExplorerCommunicator, UiCommunicator};
use crate::ui::OrchestratorToUiUpdate;
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;
use std::time::Duration;

//**Bag Content Conversation**
//
// This module manages the conversation between the Orchestrator and an Explorer regarding the contents of their bag.
// It uses a Finite State Machine (FSM) to ensure that the inventory request and response are handled
// in the correct order at compile time.
//
// The conversation flow starts by sending a request to the explorer and terminates once the
// bag content is received (intended for UI reporting).

// --- EXPLORER BAG CONTENT CONVERSATION ---
define_conversation!(
    name: BagContentConversation
);

// --- SEND BAG CONTENT REQUEST DEFINITION ---
create_request_state!(
    state_name: SendingBagContentRequest,
    conv_name: BagContentConversation,
    priority: 3,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        explorer_id: ID,
    },
    entities_id_fn: |this: &BagContentConversation<SendingBagContentRequest>| { (None, Some(this.state.explorer_id)) },
    transition_fn: send_bag_content_req_transition,
    methods_settings: {

    },
);

/// Transition Function for [`SendingBagContentRequest`] state:
///
/// Returns:
///
/// [`ErrorState`] with [`CommonErrorTypes::MessageToExplorerFailed`] if the request failed to send.
///
/// [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`] if the communication channel is missing.
///
/// The next state: [`BagContentConversation<WaitingBagContentResponse>`] if the request was sent successfully.
fn send_bag_content_req_transition(
    this: Box<BagContentConversation<SendingBagContentRequest>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this.state.to_explorer(
        this.state.explorer_id,
        OrchestratorToExplorer::BagContentRequest,
    ) {
        Ok(()) => {
            let next_state =
                WaitingBagContentResponse::new(this.state.orch_context, this.state.explorer_id);
            let next_conv =
                BagContentConversation::<WaitingBagContentResponse>::new(this.id, next_state);
            Some(Box::new(next_conv))
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAIT BAG CONTENT RESPONSE DEFINITION ---

create_response_state!(
    state: WaitingBagContentResponse,
    conv: BagContentConversation,
    priority: 3,
    timeout: Some(get_explorer_timeout()),
    expected_msg: ExplorerToOrchKind(ExplorerToOrchestratorKind::BagContentResponse),
    fields: {
        explorer_id: ID,
    },
    entities_id_closure: |this: &BagContentConversation<WaitingBagContentResponse>| { (None, Some(this.state.explorer_id)) },
    transition: wait_bag_content_res_transition,
    methods_settings: {

    },
);

/// Transition Function for [`WaitingBagContentResponse`] state:
///
/// Returns:
///
/// [None] if the [`ExplorerToOrchestrator::BagContentResponse`] is successfully received and processed, closing the conversation.
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected response kind.
/// [`ErrorState`] with [`CommonErrorTypes::MessageToUiFailed`] if the update to the UI fails.
fn wait_bag_content_res_transition(
    this: Box<BagContentConversation<WaitingBagContentResponse>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::BagContentResponse {
        explorer_id,
        bag_content,
    })) = msg
    {
        let bag_content_log = format!("{bag_content:?}");

        return match this.state.to_ui(OrchestratorToUiUpdate::ExplorerSnapshot(
            explorer_id,
            bag_content,
        )) {
            Ok(()) => {
                log_internal(
                    LogTarget::Conversations,
                    Channel::Debug,
                    payload!(
                        action : "Sent ExplorerBagContent to UI",
                        explorer_id : explorer_id,
                        conversation_id : this.id,
                        bag_content : bag_content_log
                    ),
                );
                None
            }

            Err(err) => {
                log_internal(
                    LogTarget::Conversations,
                    Channel::Warning,
                    payload!(
                        action : "Failed to send ExplorerBagContent to UI",
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
    use std::collections::HashMap;
    use crossbeam_channel::{unbounded};
    use crate::ui::UiToOrchestratorCommand;

    const CONV_ID: ID = 1;
    const EXPLORER_ID: ID = 2;

    fn make_send_conv(
        orch_context: OrchContextRef,
    ) -> Box<BagContentConversation<SendingBagContentRequest>> {
        let state = SendingBagContentRequest::new(orch_context, EXPLORER_ID);
        Box::new(BagContentConversation::<SendingBagContentRequest>::new(
            CONV_ID, state,
        ))
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
    ) -> Box<BagContentConversation<WaitingBagContentResponse>> {
        let state = WaitingBagContentResponse::new(orch_context, EXPLORER_ID);
        Box::new(BagContentConversation::<WaitingBagContentResponse>::new(
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
            .expect("Should transition to next state");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::BagContentResponse
            ))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
    }

    #[test]
    fn send_missing_sender() {
        
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();

        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_send_conv(test_ctx.clone());
        
        let next_conv = conv.transition(None).expect("Should return an ErrorState");
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(next_conv.get_id(), CONV_ID);
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
        assert_eq!(conv.get_priority(), 3);
    }

    #[test]
    fn wait_correct_transition() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::BagContentResponse {
            explorer_id: EXPLORER_ID,
            bag_content: common_explorer::ExplorerBagContent {
                resources_amounts: HashMap::default(),
            },
        });


        let result = conv.transition(Some(msg));
        assert!(
            result.is_none(),
            "Conversation should terminate upon receiving ResetExplorerAIResult"
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
            .expect("Should return an ErrorState");
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
        let state = WaitingBagContentResponse::new(test_ctx, EXPLORER_ID);
        let conv = BagContentConversation::<WaitingBagContentResponse>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::BagContentResponse
            ))
        );
        assert_eq!(conv.get_priority(), 3);
    }
}