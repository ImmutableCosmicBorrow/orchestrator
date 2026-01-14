use crate::logging_utils::log_internal;
use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_explorer::kill_explorers_manager::KillExplorersManager;
use crate::orchestrator::conversations::orch_planet::outgoing_explorer::{
    OutgoingExplorer, SendingOutgoingRequest,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct, ToPlanetStruct,
};
use crate::payload;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

///**Kill Explorer Conversation**
///
/// This module manages the termination of an Explorer.
/// It uses a Finite State Machine (FSM) to send the kill command to the explorer and wait
/// for confirmation.
///
/// Depending on the `handle_outgoing` flag, it can subsequently transition to an
/// [`OutgoingExplorer`] conversation to notify the planet that the explorer has left/died,
/// or return directly to the [`KillExplorersManager`] to finish the killing of other explorers.
///
/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingKillExplorer`] state, which sends an
/// [`OrchestratorToExplorer::KillExplorer`] request when the [`Conversation::transition`] method is called.
pub(crate) struct SendingKillExplorer {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
    /// A struct containing fields to send messages to the planet (for the outgoing notification phase)
    to_planet_struct: ToPlanetStruct,
    /// Flag indicating if the conversation should proceed to handle the outgoing explorer notification
    handle_outgoing: bool,
    /// The manager to return to once the termination sequence is complete
    manager: Box<KillExplorersManager>,
}

impl SendingKillExplorer {
    /// Constructor for [`SendingKillExplorer`] state struct
    pub(crate) fn new(
        to_explorer_struct: ToExplorerStruct,
        to_planet_struct: ToPlanetStruct,
        handle_outgoing: bool,
        manager: Box<KillExplorersManager>,
    ) -> Self {
        Self {
            to_explorer_struct,
            to_planet_struct,
            handle_outgoing,
            manager,
        }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingKillExplorerResult`] state, the conversation expects an
/// [`ExplorerToOrchestrator::KillExplorerResult`] message to confirm the explorer has been successfully terminated.
struct WaitingKillExplorerResult {
    /// ID of the explorer being terminated
    explorer_id: ID,
    /// A struct containing fields to send messages to the planet
    to_planet_struct: ToPlanetStruct,
    /// Flag indicating if the outgoing notification phase is required
    handle_outgoing: bool,
    /// The manager to return to after completion
    manager: Box<KillExplorersManager>,
}

impl WaitingKillExplorerResult {
    /// The constructor for [`WaitingKillExplorerResult`] state struct
    fn new(
        explorer_id: ID,
        to_planet_struct: ToPlanetStruct,
        handle_outgoing: bool,
        manager: Box<KillExplorersManager>,
    ) -> Self {
        Self {
            explorer_id,
            to_planet_struct,
            handle_outgoing,
            manager,
        }
    }
}

/// Kill Explorer Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
pub(crate) struct KillExplorerConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING KILL EXPLORER IMPLEMENTATION
impl Conversation<ExplorerBag> for KillExplorerConversation<SendingKillExplorer> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingKillExplorer`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] if the kill command fails to send or the sender is not found.
    ///
    /// [`KillExplorerConversation<WaitingKillExplorerResult>`] if the request was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::KillExplorer)
        {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let state_struct = WaitingKillExplorerResult::new(
                    explorer_id,
                    self.state.to_planet_struct,
                    self.state.handle_outgoing,
                    self.state.manager,
                );
                let next_state = KillExplorerConversation::<WaitingKillExplorerResult>::new(
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
        5
    }
}

impl KillExplorerConversation<SendingKillExplorer> {
    /// The constructor for [`KillExplorerConversation`] in the [`SendingKillExplorer`] state
    pub(crate) fn new(id: ID, state: SendingKillExplorer) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING KILL EXPLORER RESULT IMPLEMENTATION
impl Conversation<ExplorerBag> for KillExplorerConversation<WaitingKillExplorerResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingKillExplorerResult`] state:
    ///
    /// Returns:
    ///
    /// [`OutgoingExplorer<SendingOutgoingRequest>`] if `handle_outgoing` is true, to notify the planet of the dead explorer.
    ///
    /// The original [`KillExplorersManager`] if `handle_outgoing` is false (we already took care of the dead explorer in its planet) or if an unexpected message is received.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::KillExplorerResult {
            explorer_id,
        })) = msg_wrapped
        {
            log_internal(
                Channel::Info,
                payload!(
                    action : "Killed explorer, closing conversation",
                    explorer_id : explorer_id,
                    conversation_id : self.id
                ),
            );
            if self.state.handle_outgoing {
                let state_struct = SendingOutgoingRequest::new(
                    self.state.to_planet_struct,
                    explorer_id,
                    self.state.manager,
                );
                let next_state =
                    OutgoingExplorer::<SendingOutgoingRequest>::new(self.id, state_struct);
                return Some(Box::new(next_state));
            }
            log_internal(
                Channel::Warning,
                payload!(
                    action : "Conversation already took care of this outgoing Explorer and is going back to manager.",
                    explorer_id : explorer_id,
                    conversation_id : self.id,
                ),
            );
            return Some(self.state.manager);
        }

        // Wrong Message, return to manager as fallback
        Some(self.state.manager)
    }

    fn get_priority(&self) -> i32 {
        5
    }
}

impl KillExplorerConversation<WaitingKillExplorerResult> {
    /// The constructor for [`KillExplorerConversation`] in the [`WaitingKillExplorerResult`] state
    fn new(id: ID, state: WaitingKillExplorerResult) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::KillExplorerResult,
            )),
            state,
        }
    }
}
