use crate::convo_manager::OrchContextRef;
use crate::globals::get_convo_timeout;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::params::ConvoKind;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, ErrorType, ExplorerCommunicator,
    PossibleExpectedKinds, PossibleMessage,
};
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::components::resource::BasicResourceType;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;
use std::ops::Mul;
use std::time::Duration;

///**Craft Resource Conversation**
///
/// This module manages the conversation between the Orchestrator and an Explorer regarding
/// the manual generation of basic resources.
/// It uses a Finite State Machine (FSM) to ensure that the resource generation request
/// and the subsequent result are handled in the correct order at compile time.
///
/// The conversation flow starts by sending a generation request to the explorer and terminates
/// once the [`ExplorerToOrchestrator::GenerateResourceResponse`] is received.
/// Custom error type for when an explorer fails to generate the requested basic resource.
struct CraftingFailed {
    /// ID of the explorer who attempted the generation.
    explorer_id: ID,
    /// Detailed error message provided by the explorer.
    err: String,
    /// The type of basic resource that failed to be generated.
    resource: BasicResourceType,
}

impl ErrorType for CraftingFailed {
    fn stringify(&self) -> String {
        format!(
            "Explorer {}, failed to craft {:?}: {}",
            self.explorer_id, self.resource, self.err
        )
    }
}

// --- CRAFT RESOURCE CONVERSATION ---
define_conversation!(
    name: CraftResourceConversation
);

// --- SEND CRAFT RESOURCE REQUEST DEFINITION ---
create_request_state!(
    state_name: SendingCraftResourceRequest,
    conv_name: CraftResourceConversation,
    convo_kind: ConvoKind::CraftResource,
    timeout: None,
    expected_msg: None,
    fields: {
        explorer_id: ID,
        to_craft: BasicResourceType,
    },
    entities_id_fn: |this: &CraftResourceConversation<SendingCraftResourceRequest>| { (None, Some(this.state.explorer_id)) },
    transition_fn: send_craft_resource_req_transition,
    methods_settings: {

    },
);

/// Transition Function for [`SendingCraftResourceRequest`] state:
///
/// Returns:
///
/// [`ErrorState`] if the request failed to send to the explorer.
///
/// [`CraftResourceConversation<WaitingCraftResourceResult>`] if the request was sent successfully.
fn send_craft_resource_req_transition(
    this: Box<CraftResourceConversation<SendingCraftResourceRequest>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this.state.to_explorer(
        this.state.explorer_id,
        OrchestratorToExplorer::GenerateResourceRequest {
            to_generate: this.state.to_craft,
        },
    ) {
        Ok(()) => {
            let state_struct = WaitingCraftResourceResult::new(
                this.state.orch_context,
                this.state.explorer_id,
                this.state.to_craft,
            );
            let next_conv =
                CraftResourceConversation::<WaitingCraftResourceResult>::new(this.id, state_struct);
            Some(Box::new(next_conv))
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAITING CRAFT RESOURCE RESPONSE DEFINITION

create_response_state!(
    state: WaitingCraftResourceResult,
    conv: CraftResourceConversation,
    convo_kind: ConvoKind::CraftResource,
    timeout: Some(get_convo_timeout().mul(2)), //longer timeout since involves Explorer - Planet Conversation
    expected_msg: ExplorerToOrchKind(ExplorerToOrchestratorKind::GenerateResourceResponse),
    fields: {
        explorer_id: ID,
        to_craft: BasicResourceType,
    },
    entities_id_closure: |this: &CraftResourceConversation<WaitingCraftResourceResult>| { (None, Some(this.state.explorer_id)) },
    transition: wait_craft_resource_res_transition,
    methods_settings: {

    },
);

/// Transition Function for [`WaitingCraftResourceResult`] state:
///
/// Returns:
///
/// [None] if the [`ExplorerToOrchestrator::GenerateResourceResponse`] returns `Ok(())`, closing the conversation.
///
/// [`ErrorState`] with [`CraftingFailed`] if the explorer returns an error.
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if an unexpected message is received.
fn wait_craft_resource_res_transition(
    this: Box<CraftResourceConversation<WaitingCraftResourceResult>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::ExplorerToOrch(
        ExplorerToOrchestrator::GenerateResourceResponse {
            explorer_id,
            generated,
        },
    )) = msg
    {
        return match generated {
            Ok(()) => {
                log_internal(
                    LogTarget::Conversations,
                    Channel::Debug,
                    payload!(
                        action : "Explorer generated a resource correctly, closing conversation",
                        explorer_id : explorer_id,
                        resource : format!("{:?}",this.state.to_craft),
                        conversation_id : this.id
                    ),
                );
                None
            }

            Err(e) => {
                let error_struct = CraftingFailed {
                    explorer_id,
                    err: e,
                    resource: this.state.to_craft,
                };
                let error_state = ErrorState::new(Box::new(error_struct), this.id);
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
    use common_game::components::resource::BasicResourceType::Hydrogen;
    use crossbeam_channel::unbounded;

    const CONV_ID: ID = 1;
    const EXPLORER_ID: ID = 2;

    // --- Helper functions ---

    fn make_send_conv(
        orch_context: OrchContextRef,
    ) -> Box<CraftResourceConversation<SendingCraftResourceRequest>> {
        let state = SendingCraftResourceRequest::new(orch_context, EXPLORER_ID, Hydrogen);
        Box::new(CraftResourceConversation::<SendingCraftResourceRequest>::new(CONV_ID, state))
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
    ) -> Box<CraftResourceConversation<WaitingCraftResourceResult>> {
        let state = WaitingCraftResourceResult::new(orch_context, EXPLORER_ID, Hydrogen);
        Box::new(CraftResourceConversation::<WaitingCraftResourceResult>::new(CONV_ID, state))
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
                ExplorerToOrchestratorKind::GenerateResourceResponse
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
        assert_eq!(
            conv.get_priority(),
            ConvoKind::CraftResource.priority().as_i32()
        );
    }

    #[test]
    fn wait_correct_transition_generation_done() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::GenerateResourceResponse {
                explorer_id: EXPLORER_ID,
                generated: Ok(()),
            });
        let result = conv.transition(Some(msg));
        assert!(result.is_none(), "Conversation should terminate");
    }
    #[test]
    fn wait_correct_transition_generation_failed() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::CombineResourceResponse {
                explorer_id: EXPLORER_ID,
                generated: Err("Resource Not generated".to_string()),
            });
        let next_conv = conv
            .transition(Some(msg))
            .expect("Should transition to error state");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_error_details().is_some());
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
        let state = WaitingCraftResourceResult::new(test_ctx.clone(), EXPLORER_ID, Hydrogen);
        let conv = CraftResourceConversation::<WaitingCraftResourceResult>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::GenerateResourceResponse
            ))
        );
        assert_eq!(
            conv.get_priority(),
            ConvoKind::CraftResource.priority().as_i32()
        );
    }

    #[test]
    fn crafting_failed() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        // Setup a WaitingCraftResourceResult conversation
        let state = WaitingCraftResourceResult::new(test_ctx.clone(), 42, Hydrogen);
        let conv = CraftResourceConversation::<WaitingCraftResourceResult>::new(CONV_ID, state);
        // Simulate a GenerateResourceResponse with an error
        let msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::GenerateResourceResponse {
                explorer_id: 42,
                generated: Err("Not enough resources".to_string()),
            });
        let result = Box::new(conv)
            .transition(Some(msg))
            .expect("Should return an ErrorState");
        let expected = "Explorer 42, failed to craft Hydrogen: Not enough resources";
        assert_eq!(result.get_error_details().unwrap(), expected);
    }
}
