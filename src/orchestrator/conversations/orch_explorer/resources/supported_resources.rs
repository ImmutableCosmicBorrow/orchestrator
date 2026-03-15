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

///**Supported Resources Conversation**
///
/// This module manages the conversation between the Orchestrator and an Explorer regarding
/// the resources supported by the explorer's current location.
/// It uses a Finite State Machine (FSM) to ensure that the resource query and the resulting
/// list are handled in the correct order at compile time.
///
/// The conversation flow starts by sending a request to the explorer and terminates once the
/// [`ExplorerToOrchestrator::SupportedResourceResult`] is received and processed.
/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingSupportedResourcesRequest`] state, which sends an
/// [`OrchestratorToExplorer::SupportedResourceRequest`] when the [`Conversation::transition`] method is called.
pub(crate) struct SendingSupportedResourcesRequest {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
    /// Optional sender to forward explorer snapshot to UI
    ui_sender: Option<Sender<OrchestratorToUiUpdate>>,
}

impl SendingSupportedResourcesRequest {
    /// Constructor for [`SendingSupportedResourcesRequest`] state struct
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
/// In the [`WaitingSupportedResourcesResult`] state, the conversation expects an
/// [`ExplorerToOrchestrator::SupportedResourceResult`] message containing the specific resources
/// that can be gathered or interacted with by the explorer.
struct WaitingSupportedResourcesResult {
    /// ID of the explorer we are waiting for
    explorer_id: ID,
    /// Optional sender to forward explorer snapshot to UI
    ui_sender: Option<Sender<OrchestratorToUiUpdate>>,
}

impl WaitingSupportedResourcesResult {
    /// The constructor for [`WaitingSupportedResourcesResult`] state struct
    fn new(explorer_id: ID, ui_sender: Option<Sender<OrchestratorToUiUpdate>>) -> Self {
        Self {
            explorer_id,
            ui_sender,
        }
    }
}

/// Supported Resources Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
pub(crate) struct SupportedResourcesConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING SUPPORTED RESOURCES REQUEST IMPLEMENTATION
impl Conversation
    for SupportedResourcesConversation<SendingSupportedResourcesRequest>
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

    /// Transition Function for [`SendingSupportedResourcesRequest`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToExplorerFailed`] if the request failed to send.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`] if the communication channel is missing.
    ///
    /// The next state: [`SupportedResourcesConversation<WaitingSupportedResourcesResult>`] if the request was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation + Send + Sync>> {
        match self
            .state
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::SupportedResourceRequest)
        {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let next_state =
                    SupportedResourcesConversation::<WaitingSupportedResourcesResult>::new(
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
                    as Box<dyn Conversation + Send + Sync>)
            }
        }
    }

    fn get_priority(&self) -> i32 {
        2
    }
}

impl SupportedResourcesConversation<SendingSupportedResourcesRequest> {
    /// The constructor for [`SupportedResourcesConversation`] in the [`SendingSupportedResourcesRequest`] state
    pub(crate) fn new(id: ID, state: SendingSupportedResourcesRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING SUPPORTED RESOURCES RESULT IMPLEMENTATION
impl Conversation
    for SupportedResourcesConversation<WaitingSupportedResourcesResult>
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

    /// Transition Function for [`WaitingSupportedResourcesResult`] state:
    ///
    /// Returns:
    ///
    /// [None] if the [`ExplorerToOrchestrator::SupportedResourceResult`] is successfully received, closing the conversation.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the received message does not match the expected result kind.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::SupportedResourceResult {
                explorer_id,
                supported_resources,
            },
        )) = msg_wrapped
        {
            let resources_log = format!("{supported_resources:?}");
            // Send explorer snapshot to UI if sender is available
            if let Some(ref sender) = self.state.ui_sender {
                let _ = sender.send(OrchestratorToUiUpdate::SupportedResources(
                    explorer_id,
                    supported_resources,
                ));
            }

            log_internal(
                LogTarget::Conversations,
                Channel::Debug,
                payload!(
                    action : "Explorer sent supported resources in its current Planet, closing conversation",
                    explorer_id : explorer_id,
                    supported_resources : resources_log,
                    conversation_id : self.id
                ),
            );
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
    }

    fn get_priority(&self) -> i32 {
        2
    }

    // Longer timeout, since it involves an Explorer - Planet communication
    fn get_timeout(&self) -> Option<Duration> {
        Some(get_explorer_timeout().mul(2))
    }
}

impl SupportedResourcesConversation<WaitingSupportedResourcesResult> {
    /// The constructor for [`SupportedResourcesConversation`] in the [`WaitingSupportedResourcesResult`] state
    fn new(id: ID, explorer_id: ID, ui_sender: Option<Sender<OrchestratorToUiUpdate>>) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::SupportedResourceResult,
            )),
            state: WaitingSupportedResourcesResult::new(explorer_id, ui_sender),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_game::components::resource::BasicResourceType;
    use common_game::protocols::orchestrator_explorer::ExplorerToOrchestratorKind;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    // Using u32 as IDs assuming ID can be constructed from them or replaced by ID::generate()
    const CONV_ID: ID = 100;
    const EXPLORER_ID: ID = 200;

    #[test]
    fn send_success() {
        let (tx, _rx) = unbounded::<OrchestratorToExplorer>();
        let senders_to_explorers = Arc::new(Mutex::new(HashMap::from([(EXPLORER_ID, tx)])));
        let to_explorer = ToExplorerStruct {
            explorer_id: EXPLORER_ID,
            explorers_senders: senders_to_explorers,
        };
        let state = SendingSupportedResourcesRequest::new(to_explorer, None);
        let conv = Box::new(SupportedResourcesConversation::<
            SendingSupportedResourcesRequest,
        >::new(CONV_ID, state));
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
        let senders_to_explorers = Arc::new(Mutex::new(HashMap::new()));
        let to_explorer = ToExplorerStruct {
            explorer_id: EXPLORER_ID,
            explorers_senders: senders_to_explorers,
        };
        let state = SendingSupportedResourcesRequest::new(to_explorer, None);
        let conv = Box::new(SupportedResourcesConversation::<
            SendingSupportedResourcesRequest,
        >::new(CONV_ID, state));
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
        let senders_to_explorers = Arc::new(Mutex::new(HashMap::from([(EXPLORER_ID, tx)])));
        let to_explorer = ToExplorerStruct {
            explorer_id: EXPLORER_ID,
            explorers_senders: senders_to_explorers,
        };
        let state = SendingSupportedResourcesRequest::new(to_explorer, None);
        let conv = Box::new(SupportedResourcesConversation::<
            SendingSupportedResourcesRequest,
        >::new(CONV_ID, state));
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
        let conv = Box::new(SupportedResourcesConversation::<
            WaitingSupportedResourcesResult,
        >::new(CONV_ID, EXPLORER_ID, None));

        let mut supported_resources = HashSet::new();
        // Replace these with actual valid BasicResourceType variants as appropriate
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
        let conv = Box::new(SupportedResourcesConversation::<
            WaitingSupportedResourcesResult,
        >::new(CONV_ID, EXPLORER_ID, None));
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
        let (tx, _rx) = unbounded::<OrchestratorToExplorer>();
        let senders_to_explorers = Arc::new(Mutex::new(HashMap::from([(EXPLORER_ID, tx)])));
        let to_explorer = ToExplorerStruct {
            explorer_id: EXPLORER_ID,
            explorers_senders: senders_to_explorers,
        };
        let state = SendingSupportedResourcesRequest::new(to_explorer, None);
        let conv =
            SupportedResourcesConversation::<SendingSupportedResourcesRequest>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 2);
    }

    #[test]
    fn wait_getters() {
        let conv = SupportedResourcesConversation::<WaitingSupportedResourcesResult>::new(
            CONV_ID,
            EXPLORER_ID,
            None,
        );
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
