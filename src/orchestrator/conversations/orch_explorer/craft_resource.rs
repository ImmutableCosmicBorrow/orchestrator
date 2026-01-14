use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use common_game::components::resource::BasicResourceType;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

///**Craft Resource Conversation**
///
/// This module manages the conversation between the Orchestrator and an Explorer regarding
/// the generation of basic resources.
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

/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingCraftResourceRequest`] state, which sends an
/// [`OrchestratorToExplorer::GenerateResourceRequest`] when the [`Conversation::transition`] method is called.
struct SendingCraftResourceRequest {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
    /// The basic resource type intended to be generated
    to_craft: BasicResourceType,
}

impl SendingCraftResourceRequest {
    /// Constructor for [`SendingCraftResourceRequest`] state struct
    fn new(to_explorer_struct: ToExplorerStruct, to_craft: BasicResourceType) -> Self {
        Self {
            to_explorer_struct,
            to_craft,
        }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingCraftResourceResult`] state, the conversation expects an
/// [`ExplorerToOrchestrator::GenerateResourceResponse`] message indicating whether
/// the generation process was successful.
struct WaitingCraftResourceResult {
    /// ID of the explorer we are waiting for
    explorer_id: ID,
    /// The resource type being generated (carried over for error reporting)
    to_craft: BasicResourceType,
}

impl WaitingCraftResourceResult {
    /// The constructor for [`WaitingCraftResourceResult`] state struct
    fn new(explorer_id: ID, to_craft: BasicResourceType) -> Self {
        Self {
            explorer_id,
            to_craft,
        }
    }
}

/// Craft Resource Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
struct CraftResourceConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING CRAFT RESOURCE REQUEST IMPLEMENTATION
impl Conversation<ExplorerBag> for CraftResourceConversation<SendingCraftResourceRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingCraftResourceRequest`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] if the request failed to send to the explorer.
    ///
    /// [`CraftResourceConversation<WaitingCraftResourceResult>`] if the request was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self.state.to_explorer_struct.to_explorer(
            OrchestratorToExplorer::GenerateResourceRequest {
                to_generate: self.state.to_craft,
            },
        ) {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let state_struct =
                    WaitingCraftResourceResult::new(explorer_id, self.state.to_craft);
                let next_state = CraftResourceConversation::<WaitingCraftResourceResult>::new(
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

impl CraftResourceConversation<SendingCraftResourceRequest> {
    /// The constructor for [`CraftResourceConversation`] in the [`SendingCraftResourceRequest`] state
    fn new(id: ID, state: SendingCraftResourceRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING CRAFT RESOURCE RESULT IMPLEMENTATION
impl Conversation<ExplorerBag> for CraftResourceConversation<WaitingCraftResourceResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingCraftResourceResult`] state:
    ///
    /// Returns:
    ///
    /// [None] if the [`ExplorerToOrchestrator::GenerateResourceResponse`] returns `Ok(())`, closing the conversation.
    ///
    /// [`ErrorState`] with [`CraftingFailed`] if the explorer returns an error.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if an unexpected message is received.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::GenerateResourceResponse {
                explorer_id,
                generated,
            },
        )) = msg_wrapped
        {
            return match generated {
                Ok(()) => {
                    println!("Explorer {explorer_id} generated the requested resource correctly");
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

impl CraftResourceConversation<WaitingCraftResourceResult> {
    /// The constructor for [`CraftResourceConversation`] in the [`WaitingCraftResourceResult`] state
    fn new(id: ID, state: WaitingCraftResourceResult) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::GenerateResourceResponse,
            )),
            state,
        }
    }
}
