use crate::orchestrator::conversations::{EntitiesIDTuple, ExplorerCommunicator, UiCommunicator};
use crate::globals::{get_explorer_timeout, TIMEOUT};
use crate::logging_utils::{LogTarget, log_internal};
use crate::orchestrator::conversations::{ChannelsContext, CommonErrorTypes, Conversation, ErrorState, ExplorerContext, PossibleExpectedKinds, PossibleMessage, ToExplorerError};
use crate::{create_request_state, create_response_state, define_conversation, payload};
use crate::ui::OrchestratorToUiUpdate;
use common_explorer::ExplorerBagContent;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;
use crossbeam_channel::Sender;
use std::time::Duration;
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::conversations::orch_explorer::lifecycle::start_explorer::{SendingExplorerStart, WaitingExplorerStartResult};
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;

///**Bag Content Conversation**
///
/// This module manages the conversation between the Orchestrator and an Explorer regarding the contents of their bag.
/// It uses a Finite State Machine (FSM) to ensure that the inventory request and response are handled
/// in the correct order at compile time.
///
/// The conversation flow starts by sending a request to the explorer and terminates once the
/// bag content is received (intended for UI reporting).

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
        channels_manager: ChannelsManagerRef,
        explorer_id: ID,
    },
    entities_id_fn: |this: &BagContentConversation<SendingBagContentRequest>| { (Some(this.state.explorer_id), None) },
    transition_fn: send_bag_content_req_transition,
    methods_settings: {

    },
);

impl ExplorerContext for SendingBagContentRequest {
    fn get_explorer_id(&self) -> ID {
        self.explorer_id
    }
}

impl ChannelsContext for SendingBagContentRequest {
    fn get_channels_manager(&self) -> &ChannelsManagerRef {
        &self.channels_manager
    }
}

/// Transition Function for [`SendingBagContentRequest`] state:
///
/// Returns:
///
/// [`ErrorState`] with [`CommonErrorTypes::MessageToExplorerFailed`] if the request failed to send.
///
/// [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`] if the communication channel is missing.
///
/// The next state: [`BagContentConversation<WaitingBagContentResponse>`] if the request was sent successfully.
fn send_bag_content_req_transition(this: Box<BagContentConversation<SendingBagContentRequest>>) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this
        .state
        .to_explorer(OrchestratorToExplorer::BagContentRequest)
    {
        Ok(()) => {
            let next_state = WaitingBagContentResponse::new(this.state.explorer_id, this.state.channels_manager.clone());
            let next_conv = BagContentConversation::<WaitingBagContentResponse>::new(
                this.id,
                next_state
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

// --- WAIT BAG CONTENT RESPONSE DEFINITION ---

create_response_state!(
    state: WaitingBagContentResponse,
    conv: BagContentConversation,
    priority: 3,
    timeout: Some(get_explorer_timeout()),
    expected_msg: ExplorerToOrchKind(ExplorerToOrchestratorKind::BagContentResponse),
    fields: {
        explorer_id: ID,
        channels_manager: ChannelsManagerRef
    },
    entities_id_closure: |this: &BagContentConversation<WaitingBagContentResponse>| { (Some(this.state.explorer_id), None) },
    transition: wait_bag_content_res_transition,
    methods_settings: {

    },
);



impl ExplorerContext for WaitingBagContentResponse {
    fn get_explorer_id(&self) -> ID {
        self.explorer_id
    }
}

impl ChannelsContext for WaitingBagContentResponse {
    fn get_channels_manager(&self) -> &ChannelsManagerRef {
        &self.channels_manager
    }
}

// Use default behavior to send messages to UI
impl UiCommunicator for WaitingBagContentResponse {}

/// Transition Function for [`WaitingBagContentResponse`] state:
///
/// Returns:
///
/// [None] if the [`ExplorerToOrchestrator::BagContentResponse`] is successfully received and processed, closing the conversation.
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected response kind.
/// [`ErrorState`] with [`CommonErrorTypes::MessageToUiFailed`] if the update to the UI fails.
fn wait_bag_content_res_transition(this: Box<BagContentConversation<WaitingBagContentResponse>>, msg: Option<PossibleMessage>) -> Option<Box<dyn Conversation + Send + Sync>> {

    if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::BagContentResponse {
                                                    explorer_id,
                                                    bag_content,
                                                })) = msg
    {
        let bag_content_log = format!("{bag_content:?}");

        return match this.state.to_ui(OrchestratorToUiUpdate::ExplorerSnapshot(explorer_id, bag_content)) {
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
            },

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
                Some(Box::new(error_state)
                    as Box<dyn Conversation + Send + Sync>)
            }
        }
    }

    //Wrong Message, close conversation
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
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

    struct DummyExplorerBagContent;

    // --- Helper functions ---

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(
        senders: OrchToExplorerSenders,
    ) -> Box<BagContentConversation<SendingBagContentRequest>> {
        let to_explorer = make_to_explorer_struct(EXPLORER_ID, senders);
        let state = SendingBagContentRequest::new(to_explorer, None);
        Box::new(BagContentConversation::<SendingBagContentRequest>::new(
            CONV_ID, state,
        ))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv() -> Box<BagContentConversation<WaitingBagContentResponse>> {
        Box::new(BagContentConversation::<WaitingBagContentResponse>::new(
            CONV_ID,
            EXPLORER_ID,
            None,
        ))
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
                ExplorerToOrchestratorKind::BagContentResponse
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
        let state = SendingBagContentRequest::new(to_explorer, None);
        let conv = BagContentConversation::<SendingBagContentRequest>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 3);
    }

    #[test]
    fn wait_correct_transition() {
        let conv = make_wait_conv();
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

    /*#[test]
    fn wait_getters() {
        let conv = BagContentConversation::<WaitingBagContentResponse>::new(CONV_ID, EXPLORER_ID);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::BagContentResponse
            ))
        );
        assert_eq!(conv.get_priority(), 3);
    }*/
}
