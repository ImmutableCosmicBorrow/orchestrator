use crate::logging_utils::log_internal;
use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use crate::payload;
use common_game::components::resource::ComplexResourceType;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

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
struct CraftingFailed {
    /// ID of the explorer who attempted the craft.
    explorer_id: ID,
    /// Detailed error message provided by the explorer.
    err: String,
    /// The type of complex resource that failed to be created.
    resource: ComplexResourceType,
}

impl ErrorType for CraftingFailed {
    fn stringify(&self) -> String {
        format!(
            "Explorer {}, failed to craft {:?}: {}",
            self.explorer_id, self.resource, self.err
        )
    }
}

/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingCombineResourceRequest`] state, which sends an
/// [`OrchestratorToExplorer::CombineResourceRequest`] when the [`Conversation::transition`] method is called.
struct SendingCombineResourceRequest {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
    /// The complex resource type intended to be crafted
    to_craft: ComplexResourceType,
}

impl SendingCombineResourceRequest {
    /// Constructor for [`SendingCombineResourceRequest`] state struct
    fn new(to_explorer_struct: ToExplorerStruct, to_craft: ComplexResourceType) -> Self {
        Self {
            to_explorer_struct,
            to_craft,
        }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingCombineResourceResult`] state, the conversation expects an
/// [`ExplorerToOrchestrator::CombineResourceResponse`] message indicating whether
/// the crafting process was successful.
struct WaitingCombineResourceResult {
    /// ID of the explorer we are waiting for
    explorer_id: ID,
    /// The resource type being crafted (carried over for error reporting)
    to_craft: ComplexResourceType,
}

impl WaitingCombineResourceResult {
    /// The constructor for [`WaitingCombineResourceResult`] state struct
    fn new(explorer_id: ID, to_craft: ComplexResourceType) -> Self {
        Self {
            explorer_id,
            to_craft,
        }
    }
}

/// Combine Resource Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
struct CombineResourceConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING COMBINE RESOURCE REQUEST IMPLEMENTATION
impl Conversation<ExplorerBag> for CombineResourceConversation<SendingCombineResourceRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingCombineResourceRequest`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] if the crafting request failed to send to the explorer.
    ///
    /// [`CombineResourceConversation<WaitingCombineResourceResult>`] if the request was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self.state.to_explorer_struct.to_explorer(
            OrchestratorToExplorer::CombineResourceRequest {
                to_generate: self.state.to_craft,
            },
        ) {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let state_struct =
                    WaitingCombineResourceResult::new(explorer_id, self.state.to_craft);
                let next_state = CombineResourceConversation::<WaitingCombineResourceResult>::new(
                    self.id,
                    state_struct,
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
                Some(Box::new(error_state))
            }
        }
    }

    fn get_priority(&self) -> i32 {
        2
    }
}

impl CombineResourceConversation<SendingCombineResourceRequest> {
    /// The constructor for [`CombineResourceConversation`] in the [`SendingCombineResourceRequest`] state
    fn new(id: ID, state: SendingCombineResourceRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING COMBINE RESOURCE RESULT IMPLEMENTATION
impl Conversation<ExplorerBag> for CombineResourceConversation<WaitingCombineResourceResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
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
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::CombineResourceResponse {
                explorer_id,
                generated,
            },
        )) = msg_wrapped
        {
            return match generated {
                Ok(()) => {
                    log_internal(
                        Channel::Debug,
                        payload!(
                            action : "Explorer generated a resource correctly, closing conversation",
                            explorer_id: explorer_id,
                            resource : format!{"{:?}", self.state.to_craft},
                            conversation_id: self.id,
                        ),
                    );
                    None
                }
                Err(e) => {
                    let error_struct = CraftingFailed {
                        explorer_id,
                        err: e,
                        resource: self.state.to_craft,
                    };
                    let error_state = ErrorState::new(Box::new(error_struct), self.id);
                    Some(Box::new(error_state))
                }
            };
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        2
    }
}

impl CombineResourceConversation<WaitingCombineResourceResult> {
    /// The constructor for [`CombineResourceConversation`] in the [`WaitingCombineResourceResult`] state
    fn new(id: ID, state: WaitingCombineResourceResult) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::CombineResourceResponse,
            )),
            state,
        }
    }
}
