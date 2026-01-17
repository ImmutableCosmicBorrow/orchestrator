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
struct SendingBagContentRequest {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
}

impl SendingBagContentRequest {
    /// Constructor for [`SendingBagContentRequest`] state struct
    fn new(to_explorer_struct: ToExplorerStruct) -> Self {
        Self { to_explorer_struct }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingBagContentResponse`] state, the conversation expects an
/// [`ExplorerToOrchestrator::BagContentResponse`] message containing the items currently held by the explorer.
struct WaitingBagContentResponse {
    /// ID of the explorer we are waiting for
    explorer_id: ID,
}

impl WaitingBagContentResponse {
    /// The constructor for [`WaitingBagContentResponse`] state struct
    fn new(explorer_id: ID) -> Self {
        Self { explorer_id }
    }
}

/// Bag Content Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
struct BagContentConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING BAG CONTENT REQUEST IMPLEMENTATION
impl Conversation<ExplorerBag> for BagContentConversation<SendingBagContentRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_explorer_struct.explorer_id
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
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::BagContentRequest)
        {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let next_state =
                    BagContentConversation::<WaitingBagContentResponse>::new(self.id, explorer_id);
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
                Some(Box::new(error_state))
            }
        }
    }

    fn get_priority(&self) -> i32 {
        3
    }
}

impl BagContentConversation<SendingBagContentRequest> {
    /// The constructor for [`BagContentConversation`] in the [`SendingBagContentRequest`] state
    fn new(id: ID, state: SendingBagContentRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING BAG CONTENT RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBag> for BagContentConversation<WaitingBagContentResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_id
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
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::BagContentResponse {
            explorer_id,
            bag_content,
        })) = msg_wrapped
        {
            //TODO: SEND THIS TO UI
            log_internal(
                Channel::Debug,
                payload!(
                    action : "Explorer sent its bag content, closing conversation",
                    explorer_id : explorer_id,
                    bag_content : format!{"{bag_content:?}"},
                    conversation_id : self.id
                ),
            );
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        3
    }
}

impl BagContentConversation<WaitingBagContentResponse> {
    /// The constructor for [`BagContentConversation`] in the [`WaitingBagContentResponse`] state
    fn new(id: ID, explorer_id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::BagContentResponse,
            )),
            state: WaitingBagContentResponse::new(explorer_id),
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

    struct DummyExplorerBag;

    // --- Helper functions ---

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(
        senders: SendersToExplorer,
    ) -> Box<BagContentConversation<SendingBagContentRequest>> {
        let to_explorer = make_to_explorer_struct(EXPLORER_ID, senders);
        let state = SendingBagContentRequest::new(to_explorer);
        Box::new(BagContentConversation::<SendingBagContentRequest>::new(
            CONV_ID, state,
        ))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv() -> Box<BagContentConversation<WaitingBagContentResponse>> {
        Box::new(BagContentConversation::<WaitingBagContentResponse>::new(
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
        let state = SendingBagContentRequest::new(to_explorer);
        let conv = BagContentConversation::<SendingBagContentRequest>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entity_id(), EXPLORER_ID);
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 3);
    }

    #[test]
    fn wait_correct_transition() {
        let conv = make_wait_conv();
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::BagContentResponse {
            explorer_id: EXPLORER_ID,
            bag_content: ExplorerBag,
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

    #[test]
    fn wait_getters() {
        let conv = BagContentConversation::<WaitingBagContentResponse>::new(CONV_ID, EXPLORER_ID);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entity_id(), EXPLORER_ID);
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::BagContentResponse
            ))
        );
        assert_eq!(conv.get_priority(), 3);
    }
}
