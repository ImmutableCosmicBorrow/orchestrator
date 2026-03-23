use std::ops::Mul;
use crate::globals::{get_explorer_timeout, TIMEOUT};
use crate::logging_utils::{log_internal, LogTarget};
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::{ChannelsContext, CommonErrorTypes, Conversation, ErrorState, ErrorType, ExplorerContext, PossibleExpectedKinds, PossibleMessage};
use crate::orchestrator::conversations::{EntitiesIDTuple, ExplorerCommunicator};
use crate::orchestrator::ChannelsManagerRef;
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::components::resource::ComplexResourceType;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;
use std::time::Duration;

///**Combine Resource Conversation**
///
/// This module manages the conversation between the Orchestrator and an Explorer regarding
/// the crafting of complex resources.
/// It uses a Finite State Machine (FSM) to ensure that the request to combine resources
/// and the subsequent result (success or failure) are handled in the correct order at compile time.
///
/// The conversation flow starts by sending a [`OrchestratorToExplorer::CombineResourceRequest`] to the explorer and terminates
/// once the [`ExplorerToOrchestrator::CombineResourceResponse`] is received.
/// Custom error type for when an explorer fails to craft the requested complex resource.

//TODO: SEND RESULT TO UI?

struct CombineFailed {
    /// ID of the explorer who attempted the craft.
    explorer_id: ID,
    /// Detailed error message provided by the explorer.
    err: String,
    /// The type of complex resource that failed to be created.
    resource: ComplexResourceType,
}

impl ErrorType for CombineFailed {
    fn stringify(&self) -> String {
        format!(
            "Explorer {}, failed to craft {:?}: {}",
            self.explorer_id, self.resource, self.err
        )
    }
}

// --- COMBINE RESOURCE CONVERSATION ---
define_conversation!(
    name: CombineResourceConversation
);

// --- SEND COMBINE RESOURCE REQUEST DEFINITION ---
create_request_state!(
    state_name: SendingCombineResourceRequest,
    conv_name: CombineResourceConversation,
    priority: 2,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        channels_manager: ChannelsManagerRef,
        explorer_id: ID,
        to_combine: ComplexResourceType
    },
    entities_id_fn: |this: &CombineResourceConversation<SendingCombineResourceRequest>| { (Some(this.state.explorer_id), None) },
    transition_fn: send_comb_resource_req_transition,
    methods_settings: {

    },
);

impl ExplorerContext for SendingCombineResourceRequest {
    fn get_explorer_id(&self) -> ID {
        self.explorer_id
    }
}

impl ChannelsContext for SendingCombineResourceRequest {
    fn get_channels_manager(&self) -> &ChannelsManagerRef {
        &self.channels_manager
    }
}

/// Transition Function for [`SendingCombineResourceRequest`] state:
///
/// Returns:
///
/// [`ErrorState`] if the crafting request failed to send to the explorer.
///
/// [`CombineResourceConversation<WaitingCombineResourceResult>`] if the request was sent successfully.
fn send_comb_resource_req_transition(this: Box<CombineResourceConversation<SendingCombineResourceRequest>>) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this.state.to_explorer(
        OrchestratorToExplorer::CombineResourceRequest {
            to_generate: this.state.to_combine,
        },
    ) {
        Ok(()) => {
            let state_struct =
                WaitingCombineResourceResult::new(this.state.explorer_id, this.state.to_combine);
            let next_conv = CombineResourceConversation::<WaitingCombineResourceResult>::new(
                this.id,
                state_struct,
            );
            Some(Box::new(next_conv))
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state)
                as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAITING COMBINE RESOURCE RESPONSE DEFINITION

create_response_state!(
    state: WaitingCombineResourceResult,
    conv: CombineResourceConversation,
    priority: 2,
    timeout: Some(get_explorer_timeout().mul(2)),
    expected_msg: ExplorerToOrchKind(ExplorerToOrchestratorKind::CombineResourceResponse),
    fields: {
        explorer_id: ID,
        to_combine: ComplexResourceType
    },
    entities_id_closure: |this: &CombineResourceConversation<WaitingCombineResourceResult>| { (Some(this.state.explorer_id), None) },
    transition: wait_comb_resource_res_transition,
    methods_settings: {

    },
);

impl ExplorerContext for WaitingCombineResourceResult {
    fn get_explorer_id(&self) -> ID {
        self.explorer_id
    }
}

/// Transition Function for [`WaitingCombineResourceResult`] state:
///
/// Returns:
///
/// [None] if the [`ExplorerToOrchestrator::CombineResourceResponse`] returns `Ok(())`, closing the conversation.
///
/// [`ErrorState`] with [`CraftingFailed`] if the explorer returns an error.
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if an unexpected message is received.
fn wait_comb_resource_res_transition(this: Box<CombineResourceConversation<WaitingCombineResourceResult>>, msg: Option<PossibleMessage>) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::ExplorerToOrch(
                    ExplorerToOrchestrator::CombineResourceResponse {
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
                            explorer_id: explorer_id,
                            resource : format!{"{:?}", this.state.to_combine},
                            conversation_id: this.id,
                        ),
                );
                None
            }

            Err(e) => {
                let error_struct = CombineFailed {
                    explorer_id,
                    err: e,
                    resource: this.state.to_combine,
                };
                let error_state = ErrorState::new(Box::new(error_struct), this.id);
                Some(Box::new(error_state)
                    as Box<dyn Conversation + Send + Sync>)
            }
        };
    }

    //Wrong Message, return error state
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::orch_explorer::test_utils::{
        make_empty_senders, make_senders_with, make_to_explorer_struct, MakeSendersResult,
    };
    use crate::orchestrator::conversations::OrchToExplorerSenders;
    use common_game::components::resource::ComplexResourceType::AIPartner;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: u32 = 1;
    const EXPLORER_ID: u32 = 2;

    // --- Helper functions ---

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(
        senders: OrchToExplorerSenders,
    ) -> Box<CombineResourceConversation<SendingCombineResourceRequest>> {
        let to_explorer = make_to_explorer_struct(EXPLORER_ID, senders);
        let state = SendingCombineResourceRequest::new(to_explorer, AIPartner);
        Box::new(CombineResourceConversation::<SendingCombineResourceRequest>::new(CONV_ID, state))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv() -> Box<CombineResourceConversation<WaitingCombineResourceResult>> {
        let state = WaitingCombineResourceResult::new(EXPLORER_ID, AIPartner);
        Box::new(CombineResourceConversation::<WaitingCombineResourceResult>::new(CONV_ID, state))
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
                ExplorerToOrchestratorKind::CombineResourceResponse
            ))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
    }

    #[test]
    fn send_missing_sender() {
        let senders = make_empty_senders();
        let conv = make_send_conv(senders);
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
        let state = SendingCombineResourceRequest::new(to_explorer, AIPartner);
        let conv =
            CombineResourceConversation::<SendingCombineResourceRequest>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 2);
    }

    #[test]
    fn wait_correct_transition_combination_done() {
        let conv = make_wait_conv();
        let msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::CombineResourceResponse {
                explorer_id: EXPLORER_ID,
                generated: Ok(()),
            });
        let result = conv.transition(Some(msg));
        assert!(
            result.is_none(),
            "Conversation should terminate upon receiving ResetExplorerAIResult"
        );
    }
    #[test]
    fn wait_correct_transition_combination_failed() {
        let conv = make_wait_conv();
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
        let conv = make_wait_conv();
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
        let state = WaitingCombineResourceResult::new(EXPLORER_ID, AIPartner);
        let conv = CombineResourceConversation::<WaitingCombineResourceResult>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::CombineResourceResponse
            ))
        );
        assert_eq!(conv.get_priority(), 2);
    }
}
