use crate::globals::get_explorer_timeout;
use crate::logging_utils::{LogTarget, log_internal};
use crate::orchestrator::ExplorerBagContent;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use crate::payload;
use crate::ui::OrchestratorToUiUpdate;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;
use crossbeam_channel::Sender;
use std::ops::Mul;
use std::time::Duration;

///**Supported Combination Conversation**
///
/// This module manages the conversation between the Orchestrator and an Explorer regarding the combinations
/// supported by the explorer's current planet.
/// It uses a Finite State Machine (FSM) to ensure that the request for combinations and the subsequent
/// result list are handled in the correct order at compile time.
///
/// The conversation flow starts by sending a request to the explorer and terminates once the
/// [`ExplorerToOrchestrator::SupportedCombinationResult`] is received and processed.
/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingSupportedCombinationRequest`] state, which sends an
/// [`OrchestratorToExplorer::SupportedCombinationRequest`] when the [`Conversation::transition`] method is called.
pub(crate) struct SendingSupportedCombinationRequest {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
    /// Optional sender to forward explorer snapshot to UI
    ui_sender: Option<Sender<OrchestratorToUiUpdate>>,
}

impl SendingSupportedCombinationRequest {
    /// Constructor for [`SendingSupportedCombinationRequest`] state struct
    pub(crate) fn new(
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
/// In the [`WaitingSupportedCombinationResult`] state, the conversation expects an
/// [`ExplorerToOrchestrator::SupportedCombinationResult`] message containing the list of valid
/// recipes or combinations available to the explorer.
struct WaitingSupportedCombinationResult {
    /// ID of the explorer we are waiting for
    explorer_id: ID,
    /// Optional sender to forward explorer snapshot to UI
    ui_sender: Option<Sender<OrchestratorToUiUpdate>>,
}

impl WaitingSupportedCombinationResult {
    /// The constructor for [`WaitingSupportedCombinationResult`] state struct
    fn new(explorer_id: ID, ui_sender: Option<Sender<OrchestratorToUiUpdate>>) -> Self {
        Self {
            explorer_id,
            ui_sender,
        }
    }
}

/// Supported Combination Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
pub(crate) struct SupportedCombinationConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING SUPPORTED COMBINATION REQUEST IMPLEMENTATION
impl Conversation<ExplorerBagContent>
    for SupportedCombinationConversation<SendingSupportedCombinationRequest>
{
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, Some(self.state.to_explorer_struct.explorer_id))
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingSupportedCombinationRequest`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToExplorerFailed`] if the request failed to send.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`] if the communication channel is missing.
    ///
    /// The next state: [`SupportedCombinationConversation<WaitingSupportedCombinationResult>`] if the request was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        match self
            .state
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::SupportedCombinationRequest)
        {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let next_state = SupportedCombinationConversation::<
                    WaitingSupportedCombinationResult,
                >::new(
                    self.id, explorer_id, self.state.ui_sender.clone()
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
        2
    }
}

impl SupportedCombinationConversation<SendingSupportedCombinationRequest> {
    /// The constructor for [`SupportedCombinationConversation`] in the [`SendingSupportedCombinationRequest`] state
    pub(crate) fn new(id: ID, state: SendingSupportedCombinationRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING SUPPORTED COMBINATION RESULT IMPLEMENTATION
impl Conversation<ExplorerBagContent>
    for SupportedCombinationConversation<WaitingSupportedCombinationResult>
{
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, Some(self.state.explorer_id))
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingSupportedCombinationResult`] state:
    ///
    /// Returns:
    ///
    /// [None] if the [`ExplorerToOrchestrator::SupportedCombinationResult`] is successfully received, closing the conversation.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected result kind.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::SupportedCombinationResult {
                explorer_id,
                combination_list,
            },
        )) = msg_wrapped
        {
            let combinations_log = format!("{combination_list:?}");

            // Send explorer snapshot to UI if sender is available
            if let Some(ref sender) = self.state.ui_sender {
                let _ = sender.send(OrchestratorToUiUpdate::SupportedCombinations(
                    explorer_id,
                    combination_list,
                ));
            }

            log_internal(
                LogTarget::Conversations,
                Channel::Debug,
                payload!(
                    action : "Explorer sent supported combinations in its current Planet, closing conversation",
                    explorer_id : explorer_id,
                    supported_combinnations : combinations_log,
                    conversation_id : self.id
                ),
            );
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
    }

    fn get_priority(&self) -> i32 {
        2
    }

    // Longer timeout, since it involves an Explorer - Planet communication
    fn get_timeout(&self) -> Option<Duration> {
        Some(get_explorer_timeout().mul(2))
    }
}

impl SupportedCombinationConversation<WaitingSupportedCombinationResult> {
    /// The constructor for [`SupportedCombinationConversation`] in the [`WaitingSupportedCombinationResult`] state
    fn new(id: ID, explorer_id: ID, ui_sender: Option<Sender<OrchestratorToUiUpdate>>) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::SupportedCombinationResult,
            )),
            state: WaitingSupportedCombinationResult::new(explorer_id, ui_sender),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::OrchToExplorerSenders;
    use crate::orchestrator::conversations::orch_explorer::test_utils::{
        MakeSendersResult, make_empty_senders, make_senders_with, make_to_explorer_struct,
    };
    use common_game::components::resource::ComplexResourceType;
    use common_game::protocols::orchestrator_explorer::ExplorerToOrchestratorKind;
    use crossbeam_channel::unbounded;
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex};

    const CONV_ID: ID = 100;
    const EXPLORER_ID: ID = 200;

    // --- Helper functions ---

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(
        senders: OrchToExplorerSenders,
    ) -> Box<SupportedCombinationConversation<SendingSupportedCombinationRequest>> {
        let to_explorer = make_to_explorer_struct(EXPLORER_ID, senders);
        let state = SendingSupportedCombinationRequest::new(to_explorer, None);
        Box::new(SupportedCombinationConversation::<
            SendingSupportedCombinationRequest,
        >::new(CONV_ID, state))
    }

    /*#[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv() -> Box<SupportedCombinationConversation<WaitingSupportedCombinationResult>>
    {
        Box::new(SupportedCombinationConversation::<
            WaitingSupportedCombinationResult,
        >::new(CONV_ID, EXPLORER_ID))
    }*/

    fn make_combination_list() -> HashSet<ComplexResourceType> {
        let mut combination_list = HashSet::new();
        combination_list.insert(ComplexResourceType::Water);
        combination_list.insert(ComplexResourceType::Robot);
        combination_list
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let MakeSendersResult(senders, _rx) = make_senders_with(EXPLORER_ID);
        let conv = make_send_conv(senders);
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
        let senders = make_empty_senders();
        let conv = make_send_conv(senders);
        let next_conv = conv.transition(None).expect("Should return an ErrorState");
        assert!(next_conv.get_expected_kind().is_none());
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

    /*#[test]
    fn send_getters() {
        let MakeSendersResult(senders, _rx) = make_senders_with(EXPLORER_ID);
        let to_explorer = make_to_explorer_struct(EXPLORER_ID, senders);
        let state = SendingSupportedCombinationRequest::new(to_explorer);
        let conv = SupportedCombinationConversation::<SendingSupportedCombinationRequest>::new(
            CONV_ID, state,
        );
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 2);
    }*/

    /*#[test]
    fn wait_correct_message() {
        let conv = make_wait_conv();
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
    }*/

    /*#[test]
    fn wait_wrong_message() {
        let conv = make_wait_conv();
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
    }*/

    /*#[test]
    fn wait_getters() {
        let conv = SupportedCombinationConversation::<WaitingSupportedCombinationResult>::new(
            CONV_ID,
            EXPLORER_ID,
        );
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::SupportedCombinationResult
            ))
        );
        assert_eq!(conv.get_priority(), 2);
    }*/
}
