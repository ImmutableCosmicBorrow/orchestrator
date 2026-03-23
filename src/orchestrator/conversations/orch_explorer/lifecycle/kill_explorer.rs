use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::globals::{get_explorer_timeout, TIMEOUT};
use crate::logging_utils::{log_internal, LogTarget};
use crate::orchestrator::conversations::orch_planet::galaxy_events::adv_dead_explorer::{
    AdvDeadExplorer, SendingDeadExpAdv,
};
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::{ChannelsContext, CommonErrorTypes, Conversation, ErrorState, ExplorerCommunicator, ExplorerContext, PossibleExpectedKinds, PossibleMessage};
use crate::orchestrator::{ChannelsManagerRef, ExplorersLocationRef};
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;
use std::time::Duration;

///**Kill Explorer Conversation**
///
/// This module manages the termination of an Explorer.
/// It uses a Finite State Machine (FSM) to send the kill command to the explorer and wait
/// for confirmation.
///
/// Depending on the `handle_outgoing` flag, it can subsequently transition to an
/// [`AdvDeadExplorer`] conversation to notify the planet that the explorer has left/died,
/// or end the conversation returning None
///
/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingKillExplorer`] state, which sends an
/// [`OrchestratorToExplorer::KillExplorer`] request when the [`Conversation::transition`] method is called.

// --- KILL EXPLORER CONVERSATION ---

define_conversation!(
    name: KillExplorerConversation
);

// --- SEND KILL EXPLORER CONVERSATION ---

create_request_state!(
    state_name: SendingExplorerKill,
    conv_name: KillExplorerConversation,
    priority: 5,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        channels_manager: ChannelsManagerRef,
        explorer_id: ID,
        explorers_location_ref: ExplorersLocationRef,
        handle_outgoing: bool,
    },
    entities_id_fn: |this: &KillExplorerConversation<SendingExplorerKill>| { (Some(this.state.explorer_id), None) },
    transition_fn: send_explorer_kill_transition,
    methods_settings: {

    },
);

impl ExplorerContext for SendingExplorerKill {
    fn get_explorer_id(&self) -> ID {
        self.explorer_id
    }
}

impl ChannelsContext for SendingExplorerKill {
    fn get_channels_manager(&self) -> &ChannelsManagerRef {
        &self.channels_manager
    }
}

/// Transition Function for [`SendingKillExplorer`] state:
///
/// Returns:
///
/// [`ErrorState`] if the kill command fails to send or the sender is not found.
///
/// [`KillExplorerConversation<WaitingKillExplorerResult>`] if the request was sent successfully.
fn send_explorer_kill_transition(this: Box<KillExplorerConversation<SendingExplorerKill>>) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this
        .state
        .to_explorer(OrchestratorToExplorer::KillExplorer)
    {
        Ok(()) => {

            let curr_planet_id = this.state.get_planet_of_explorer().expect("Explorer should be located in a planet");

            let next_state = WaitingKillExplorerResult::new(
                this.state.channels_manager.clone(),
                this.state.explorer_id,
                curr_planet_id,
                this.state.handle_outgoing,
                this.state.explorers_location_ref.clone(),
            );
            let next_conv = KillExplorerConversation::<WaitingKillExplorerResult>::new(
                this.id,
                next_state,
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

impl SendingExplorerKill {
    /// Helper Function to retrieve the planet ID of the current explorer getting killed
    /// in order to send the advertisement
    ///
    /// Return: The optional ID of the planet
    fn get_planet_of_explorer(&self) -> Option<ID> {
        self.explorers_location_ref.lock().unwrap().get(&self.explorer_id).copied()
    }
}

// --- WAIT KILL EXPLORER RESULT DEFINITION ---

create_response_state!(
    state: WaitingKillExplorerResult,
    conv: KillExplorerConversation,
    priority: 5,
    timeout: Some(get_explorer_timeout()),
    expected_msg: ExplorerToOrchKind(ExplorerToOrchestratorKind::KillExplorerResult),
    fields: {
        channels_manager: ChannelsManagerRef,
        explorer_id: ID,
        curr_planet_id: ID,
        handle_outgoing: bool,
        explorers_location_ref: ExplorersLocationRef,
    },
    entities_id_closure: |this: &KillExplorerConversation<WaitingKillExplorerResult>| { (Some(this.state.explorer_id), None) },
    transition: wait_exp_kill_res_transition,
    methods_settings: {

    },
);

impl ExplorerContext for WaitingKillExplorerResult {
    fn get_explorer_id(&self) -> ID {
        self.explorer_id
    }
}

/// Transition Function for [`WaitingKillExplorerResult`] state:
///
/// Returns:
///
/// [`AdvDeadExplorer<SendingDeadExpAdv>`] if `handle_outgoing` is true, to notify the planet of the dead explorer.
///
/// None if `handle_outgoing` is false (we already took care of the dead explorer in its planet), closing the conversation
fn wait_exp_kill_res_transition(this: Box<KillExplorerConversation<WaitingKillExplorerResult>>, msg: Option<PossibleMessage>) -> Option<Box<dyn Conversation + Send + Sync>> {

    if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::KillExplorerResult {
        explorer_id,
     })) = msg
    {
        //Delete killed explorer from the explorer location list
        this.state.delete_dead_explorer();

        log_internal(
            LogTarget::Conversations,
            Channel::Info,
            payload!(
                    action : "Killed explorer",
                    explorer_id : explorer_id,
                    conversation_id : this.id
                ),
        );

        // handle outgoing --> Notify planet of dead explorer
        if this.state.handle_outgoing {
            let state_struct = SendingDeadExpAdv::new(
                this.state.channels_manager,
                this.state.curr_planet_id,
                this.state.explorer_id,
            );
            let next_state = AdvDeadExplorer::<SendingDeadExpAdv>::new(this.id, state_struct);
            return Some(Box::new(next_state));
        }

        //No need to adv dead explorer, ending conversation
        log_internal(
            LogTarget::Conversations,
            Channel::Warning,
            payload!(
                    action : "Conversation already took care of this dead Explorer and is Ending",
                    explorer_id : explorer_id,
                    conversation_id : this.id,
                ),
        );
        return None;
    }

    // Wrong Message
    log_internal(
        LogTarget::Conversations,
        Channel::Warning,
        payload!(
                action : "Wrong Message arrived, sending error state",
                conversation_id : this.id,
            ),
    );
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}

impl WaitingKillExplorerResult {
    /// Helper function to remove an explorer from the location list
    fn delete_dead_explorer(&self) {
        assert!(
            self
                .explorers_location_ref
                .lock()
                .unwrap()
                .remove(&self.explorer_id)
                .is_some(),
            "Trying to delete the dead explorer {} from the location map, but the entry is not found!",
            self.explorer_id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::orch_explorer::test_utils::{
        make_empty_senders, make_senders_with, make_to_explorer_struct, make_to_planet_struct,
        MakeSendersResult,
    };
    use crate::orchestrator::conversations::{OrchToExplorerSenders, OrchToPlanetSenders};
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    const CONV_ID: u32 = 1;
    const EXPLORER_ID: u32 = 2;

    // --- Helper functions ---

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(
        exp_senders: OrchToExplorerSenders,
        pla_senders: OrchToPlanetSenders,
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
        planet_senders: OrchToPlanetSenders,
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
