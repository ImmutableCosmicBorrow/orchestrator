use crate::logging_utils::log_internal;
use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_planet::adv_dead_explorer::{
    AdvDeadExplorer, SendingDeadExpAdv,
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
/// or end the conversation returnin None
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
}

impl SendingKillExplorer {
    /// Constructor for [`SendingKillExplorer`] state struct
    pub(crate) fn new(
        to_explorer_struct: ToExplorerStruct,
        to_planet_struct: ToPlanetStruct,
        handle_outgoing: bool,
    ) -> Self {
        Self {
            to_explorer_struct,
            to_planet_struct,
            handle_outgoing,
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
}

impl WaitingKillExplorerResult {
    /// The constructor for [`WaitingKillExplorerResult`] state struct
    fn new(explorer_id: ID, to_planet_struct: ToPlanetStruct, handle_outgoing: bool) -> Self {
        Self {
            explorer_id,
            to_planet_struct,
            handle_outgoing,
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
    /// [`AdvDeadExplorer<SendingDeadExpAdv>`] if `handle_outgoing` is true, to notify the planet of the dead explorer.
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
                    action : "Killed explorer",
                    explorer_id : explorer_id,
                    conversation_id : self.id
                ),
            );
            if self.state.handle_outgoing {
                let state_struct = SendingDeadExpAdv::new(self.state.to_planet_struct, explorer_id);
                let next_state = AdvDeadExplorer::<SendingDeadExpAdv>::new(self.id, state_struct);
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
            return None;
        }

        // Wrong Message
        log_internal(
            Channel::Warning,
            payload!(
                action : "Wrong Message arrived, sending error state",
                conversation_id : self.id,
            ),
        );
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::orch_explorer::test_utils::{
        MakeSendersResult, make_empty_senders, make_senders_with, make_to_explorer_struct,
        make_to_planet_struct,
    };
    use crate::orchestrator::conversations::{SendersToExplorer, SendersToPlanet};
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    const CONV_ID: u32 = 1;
    const EXPLORER_ID: u32 = 2;

    // --- Helper functions ---

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(
        exp_senders: SendersToExplorer,
        pla_senders: SendersToPlanet,
        handle_outgoing: bool,
    ) -> Box<KillExplorerConversation<SendingKillExplorer>> {
        let to_explorer = make_to_explorer_struct(EXPLORER_ID, exp_senders);
        let to_planet = make_to_planet_struct(5, pla_senders);

        let state = SendingKillExplorer::new(to_explorer, to_planet, false);
        Box::new(KillExplorerConversation::<SendingKillExplorer>::new(
            CONV_ID, state,
        ))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv(
        planet_senders: SendersToPlanet,
        handle_outgoing: bool,
    ) -> Box<KillExplorerConversation<WaitingKillExplorerResult>> {
        let to_planet_struct = make_to_planet_struct(5, planet_senders);
        let state = WaitingKillExplorerResult::new(EXPLORER_ID, to_planet_struct, handle_outgoing);
        Box::new(KillExplorerConversation::<WaitingKillExplorerResult>::new(
            CONV_ID, state,
        ))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let MakeSendersResult(senders, _rx) = make_senders_with(EXPLORER_ID);
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));
        let conv = make_send_conv(senders, planet_senders, false);
        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::KillExplorerResult
            ))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
    }

    #[test]
    fn send_missing_sender() {
        let senders = make_empty_senders();
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));
        let conv = make_send_conv(senders, planet_senders, false);
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
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));

        let conv = make_send_conv(senders, planet_senders, false);
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
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));

        let conv = make_send_conv(senders, planet_senders, false);

        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entity_id(), EXPLORER_ID);
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 5);
    }

    #[test]
    fn wait_correct_transition_no_outgoing_handling() {
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));

        let conv = make_wait_conv(planet_senders, false);
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::KillExplorerResult {
            explorer_id: EXPLORER_ID,
        });
        let next_conv = conv
            .transition(Some(msg))
            .expect("Should transition to next state");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(next_conv.get_priority(), 5);
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn wait_correct_transition_outgoing_handling() {
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));

        let conv = make_wait_conv(planet_senders, true);
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::KillExplorerResult {
            explorer_id: EXPLORER_ID,
        });
        let next_conv = conv
            .transition(Some(msg))
            .expect("Should transition to next state");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(next_conv.get_priority(), 4);
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn wait_wrong_message() {
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));
        let conv = make_wait_conv(planet_senders, true);

        let wrong_msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::StartExplorerAIResult {
                explorer_id: EXPLORER_ID,
            });

        let next_conv = conv
            .transition(Some(wrong_msg))
            .expect("Should transition to next state");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(next_conv.get_priority(), 5);
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn wait_getters() {
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));

        let conv = make_wait_conv(planet_senders, false);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entity_id(), EXPLORER_ID);
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::KillExplorerResult
            ))
        );
        assert_eq!(conv.get_priority(), 5);
    }
}
