use crate::globals::get_explorer_timeout;
use crate::logging_utils::log_internal;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use crate::payload;
use crate::ui::OrchestratorToUiUpdate;
use common_explorer::ExplorerBagContent;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;
use crossbeam_channel::Sender;
use std::time::Duration;

///**Bag Content Conversation**
///
/// This module manages the conversation between the Orchestrator and an Explorer regarding the contents of their bag.
/// It uses a Finite State Machine (FSM) to ensure that the inventory request and response are handled
/// in the correct order at compile time.
///
/// The conversation flow starts by sending a request to the explorer and terminates once the
/// bag content is received (intended for UI reporting).
/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingBagContentRequest`] state, which sends an
/// [`OrchestratorToExplorer::BagContentRequest`] when the [`Conversation::transition`] method is called.
pub struct SendingBagContentRequest {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
    /// Optional sender to forward explorer snapshot to UI
    ui_sender: Option<Sender<OrchestratorToUiUpdate>>,
}

impl SendingBagContentRequest {
    /// Constructor for [`SendingBagContentRequest`] state struct
    pub fn new(
        to_explorer_struct: ToExplorerStruct,
        ui_sender: Option<Sender<OrchestratorToUiUpdate>>,
    ) -> Self {
        Self {
            to_explorer_struct,
            ui_sender,
        }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingBagContentResponse`] state, the conversation expects an
/// [`ExplorerToOrchestrator::BagContentResponse`] message containing the items currently held by the explorer.
struct WaitingBagContentResponse {
    /// ID of the explorer we are waiting for
    explorer_id: ID,
    /// Optional sender to forward explorer snapshot to UI
    ui_sender: Option<Sender<OrchestratorToUiUpdate>>,
}

impl WaitingBagContentResponse {
    /// The constructor for [`WaitingBagContentResponse`] state struct
    fn new(explorer_id: ID, ui_sender: Option<Sender<OrchestratorToUiUpdate>>) -> Self {
        Self {
            explorer_id,
            ui_sender,
        }
    }
}

/// Bag Content Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
pub struct BagContentConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING BAG CONTENT REQUEST IMPLEMENTATION
impl Conversation<ExplorerBagContent> for BagContentConversation<SendingBagContentRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, Some(self.state.to_explorer_struct.explorer_id))
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
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
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        match self
            .state
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::BagContentRequest)
        {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let next_state = BagContentConversation::<WaitingBagContentResponse>::new(
                    self.id,
                    explorer_id,
                    self.state.ui_sender.clone(),
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
                    as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
            }
        }
    }

    fn get_priority(&self) -> i32 {
        3
    }
}

impl BagContentConversation<SendingBagContentRequest> {
    /// The constructor for [`BagContentConversation`] in the [`SendingBagContentRequest`] state
    pub fn new(id: ID, state: SendingBagContentRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING BAG CONTENT RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBagContent> for BagContentConversation<WaitingBagContentResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, Some(self.state.explorer_id))
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingBagContentResponse`] state:
    ///
    /// Returns:
    ///
    /// [None] if the [`ExplorerToOrchestrator::BagContentResponse`] is successfully received and processed, closing the conversation.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected response kind.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::BagContentResponse {
            explorer_id,
            bag_content,
        })) = msg_wrapped
        {
            let bag_content_log = format!("{bag_content:?}");

            // Send explorer snapshot to UI if sender is available
            if let Some(ref sender) = self.state.ui_sender {
                let res = sender.send(OrchestratorToUiUpdate::ExplorerSnapshot(
                    explorer_id,
                    bag_content,
                ));

                if res.is_err() {
                    log_internal(
                        Channel::Warning,
                        payload!(
                            action : "Failed to send ExplorerBagContent to UI",
                            explorer_id : explorer_id,
                            conversation_id : self.id
                        ),
                    );
                } else {
                    log_internal(
                        Channel::Info,
                        payload!(
                            action : "Sent ExplorerBagContent to UI",
                            explorer_id : explorer_id,
                            conversation_id : self.id,
                            bag_content : bag_content_log
                        ),
                    );
                }
            }
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
    }

    fn get_priority(&self) -> i32 {
        3
    }

    // Longer timeout, since it involves a communication with an Explorer
    fn get_timeout(&self) -> Option<Duration> {
        Some(get_explorer_timeout())
    }
}

impl BagContentConversation<WaitingBagContentResponse> {
    /// The constructor for [`BagContentConversation`] in the [`WaitingBagContentResponse`] state
    fn new(id: ID, explorer_id: ID, ui_sender: Option<Sender<OrchestratorToUiUpdate>>) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::BagContentResponse,
            )),
            state: WaitingBagContentResponse::new(explorer_id, ui_sender),
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
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: u32 = 1;
    const EXPLORER_ID: u32 = 2;

    struct DummyExplorerBagContent;

    // --- Helper functions ---

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(
        senders: SendersToExplorer,
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
