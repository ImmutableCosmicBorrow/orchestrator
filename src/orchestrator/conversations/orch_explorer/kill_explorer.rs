use crate::logging_utils::log_internal;
use crate::orchestrator::conversations::orch_planet::adv_dead_explorer::{
    AdvDeadExplorer, SendingDeadExpAdv,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct, ToPlanetStruct,
};
use crate::orchestrator::{ExplorerBag, ExplorersLocationRef};
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
    /// Flag indicating if the conversation should proceed to handle the dead explorer notification
    handle_outgoing: bool,
    ///The reference to the explorers location used to track them in the orchestrator
    explorers_location_ref: ExplorersLocationRef,
}

impl SendingKillExplorer {
    /// Constructor for [`SendingKillExplorer`] state struct
    pub(crate) fn new(
        to_explorer_struct: ToExplorerStruct,
        to_planet_struct: ToPlanetStruct,
        handle_outgoing: bool,
        explorers_location_ref: ExplorersLocationRef,
    ) -> Self {
        Self {
            to_explorer_struct,
            to_planet_struct,
            handle_outgoing,
            explorers_location_ref,
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
    explorers_location_ref: ExplorersLocationRef,
}

impl WaitingKillExplorerResult {
    /// The constructor for [`WaitingKillExplorerResult`] state struct
    fn new(
        explorer_id: ID,
        to_planet_struct: ToPlanetStruct,
        handle_outgoing: bool,
        explorers_location_ref: ExplorersLocationRef,
    ) -> Self {
        Self {
            explorer_id,
            to_planet_struct,
            handle_outgoing,
            explorers_location_ref,
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

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, Some(self.state.to_explorer_struct.explorer_id))
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
                    self.state.explorers_location_ref,
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
                Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBag> + Send + Sync>)
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

    // Getter for the entities ID (planet_id, explorer_id)
    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, Some(self.state.explorer_id))
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
    /// None if `handle_outgoing` is false (we already took care of the dead explorer in its planet)
    /// panic if an unexpected message is received.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::KillExplorerResult {
            explorer_id,
        })) = msg_wrapped
        {
            self.delete_dead_explorer();

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
            //No need to adv dead explorer, ending conversation
            log_internal(
                Channel::Warning,
                payload!(
                    action : "Conversation already took care of this dead Explorer and is Ending",
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
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBag> + Send + Sync>)
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

    fn delete_dead_explorer(&self) {
        assert!(
            self.state
                .explorers_location_ref
                .lock()
                .unwrap()
                .remove(&self.state.explorer_id)
                .is_some(),
            "Trying to delete the dead explorer {} from the location map, but the entry is not found!",
            self.state.explorer_id
        );
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
        _handle_outgoing: bool,
    ) -> Box<KillExplorerConversation<SendingKillExplorer>> {
        let to_explorer = make_to_explorer_struct(EXPLORER_ID, exp_senders);
        let to_planet = make_to_planet_struct(5, pla_senders);

        let state = SendingKillExplorer::new(
            to_explorer,
            to_planet,
            false,
            Arc::new(Mutex::new(HashMap::new())),
        );
        Box::new(KillExplorerConversation::<SendingKillExplorer>::new(
            CONV_ID, state,
        ))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv(
        planet_senders: SendersToPlanet,
        handle_outgoing: bool,
        explorers_location_ref: ExplorersLocationRef,
    ) -> Box<KillExplorerConversation<WaitingKillExplorerResult>> {
        let to_planet_struct = make_to_planet_struct(5, planet_senders);
        let state = WaitingKillExplorerResult::new(
            EXPLORER_ID,
            to_planet_struct,
            handle_outgoing,
            explorers_location_ref,
        );
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
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 5);
    }

    #[test]
    fn wait_correct_transition_no_outgoing_handling() {
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));
        let explorers_locations = HashMap::from([(EXPLORER_ID, 5)]);
        let exp_loc_ref = Arc::new(Mutex::new(explorers_locations));
        let conv = make_wait_conv(planet_senders, false, exp_loc_ref.clone());

        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::KillExplorerResult {
            explorer_id: EXPLORER_ID,
        });
        let next_conv = conv.transition(Some(msg));
        // When handle_outgoing is false, the conversation should end (return None)
        assert!(
            next_conv.is_none(),
            "Conversation should end and return None"
        );
        assert!(
            exp_loc_ref.lock().unwrap().is_empty(),
            "Should have killed the only explorer saved in the map"
        );
    }

    #[test]
    fn wait_correct_transition_outgoing_handling() {
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));
        let explorers_locations = HashMap::from([(EXPLORER_ID, 5)]);
        let exp_loc_ref = Arc::new(Mutex::new(explorers_locations));
        let conv = make_wait_conv(planet_senders, true, exp_loc_ref.clone());
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::KillExplorerResult {
            explorer_id: EXPLORER_ID,
        });
        let next_conv = Box::new(conv)
            .transition(Some(msg))
            .expect("Should transition to next state");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(next_conv.get_priority(), 4);
        assert!(next_conv.get_error_details().is_none());

        assert!(
            exp_loc_ref.lock().unwrap().is_empty(),
            "Should have killed the only explorer saved in the map"
        );
    }

    #[test]
    fn wait_wrong_message() {
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));
        let exp_loc_ref = Arc::new(Mutex::new(HashMap::new()));

        let conv = make_wait_conv(planet_senders, true, exp_loc_ref);
        let wrong_msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::StartExplorerAIResult {
                explorer_id: EXPLORER_ID,
            });

        let next_conv = Box::new(conv)
            .transition(Some(wrong_msg))
            .expect("Should transition to next state");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(next_conv.get_priority(), 5);
        // Now, error details should be present for a wrong message
        assert_eq!(
            next_conv.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }

    #[test]
    fn wait_getters() {
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));
        let exp_loc_ref = Arc::new(Mutex::new(HashMap::new()));

        let conv = make_wait_conv(planet_senders, false, exp_loc_ref);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::KillExplorerResult
            ))
        );
        assert_eq!(conv.get_priority(), 5);
    }
}
