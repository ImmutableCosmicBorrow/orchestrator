use crate::globals::get_convo_timeout;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::orch_planet::galaxy_events::adv_dead_explorer::{
    AdvDeadExplorer, SendingDeadExpAdv,
};
use crate::orchestrator::conversations::params::ConvoKind;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, ExplorerCommunicator,
    PossibleExpectedKinds, PossibleMessage,
};
use crate::orchestrator::{ChannelsManagerRef, OrchContextRef};
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;
use std::time::Duration;

//**Kill Explorer Conversation**
//
// This module manages the termination of an Explorer.
// It uses a Finite State Machine (FSM) to send the kill command to the explorer and wait
// for confirmation.
//
// Depending on the `handle_outgoing` flag, it can subsequently transition to an
// [`AdvDeadExplorer`] conversation to notify the planet that the explorer has left/died,
// or end the conversation returning None
//
// Marker struct for FSM state
//
// The conversation starts in the [`SendingKillExplorer`] state, which sends an
// [`OrchestratorToExplorer::KillExplorer`] request when the [`Conversation::transition`] method is called.

// --- KILL EXPLORER CONVERSATION ---

define_conversation!(
    name: KillExplorerConversation
);

// --- SEND KILL EXPLORER CONVERSATION ---

create_request_state!(
    state_name: SendingExplorerKill,
    conv_name: KillExplorerConversation,
    convo_kind: ConvoKind::KillExplorer,
    timeout: None,
    expected_msg: None,
    fields: {
        explorer_id: ID,
        curr_planet_id: ID,
        handle_outgoing: bool,
    },
    entities_id_fn: |this: &KillExplorerConversation<SendingExplorerKill>| {  (None, Some(this.state.explorer_id)) },
    transition_fn: send_explorer_kill_transition,
    methods_settings: {

    },
);

/// Transition Function for [`SendingKillExplorer`] state:
///
/// Returns:
///
/// [`ErrorState`] if the kill command fails to send or the sender is not found.
///
/// [`KillExplorerConversation<WaitingKillExplorerResult>`] if the request was sent successfully.
fn send_explorer_kill_transition(
    this: Box<KillExplorerConversation<SendingExplorerKill>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this
        .state
        .to_explorer(this.state.explorer_id, OrchestratorToExplorer::KillExplorer)
    {
        Ok(()) => {
            let next_state = WaitingKillExplorerResult::new(
                this.state.orch_context,
                this.state.explorer_id,
                this.state.curr_planet_id,
                this.state.handle_outgoing,
            );
            let next_conv =
                KillExplorerConversation::<WaitingKillExplorerResult>::new(this.id, next_state);
            Some(Box::new(next_conv))
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAIT KILL EXPLORER RESULT DEFINITION ---

create_response_state!(
    state: WaitingKillExplorerResult,
    conv: KillExplorerConversation,
    convo_kind: ConvoKind::KillExplorer,
    timeout: Some(get_convo_timeout()),
    expected_msg: ExplorerToOrchKind(ExplorerToOrchestratorKind::KillExplorerResult),
    fields: {
        explorer_id: ID,
        curr_planet_id: ID,
        handle_outgoing: bool,
    },
    entities_id_closure: |this: &KillExplorerConversation<WaitingKillExplorerResult>| { (None, Some(this.state.explorer_id)) },
    transition: wait_exp_kill_res_transition,
    methods_settings: {

    },
);

/// Transition Function for [`WaitingKillExplorerResult`] state:
///
/// Returns:
///
/// [`AdvDeadExplorer<SendingDeadExpAdv>`] if `handle_outgoing` is true, to notify the planet of the dead explorer.
///
/// None if `handle_outgoing` is false (we already took care of the dead explorer in its planet), closing the conversation
fn wait_exp_kill_res_transition(
    this: Box<KillExplorerConversation<WaitingKillExplorerResult>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::KillExplorerResult {
        explorer_id,
    })) = msg
    {
        //Delete killed explorer from the explorer location list
        this.state.delete_dead_explorer();
        this.state
            .get_channels_manager()
            .remove_explorer_channels(explorer_id);

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
                this.state.orch_context,
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
            self.orch_context
                .explorers_location
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
        add_broken_explorer_sender, add_working_explorer_sender, make_test_context,
    };
    use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
    use crossbeam_channel::unbounded;
    use dashmap::DashMap;

    const CONV_ID: ID = 1;
    const EXPLORER_ID: ID = 2;
    const PLANET_ID: ID = 5;

    fn make_send_conv(
        orch_context: OrchContextRef,
        handle_outgoing: bool,
    ) -> Box<KillExplorerConversation<SendingExplorerKill>> {
        let state = SendingExplorerKill::new(orch_context, EXPLORER_ID, PLANET_ID, handle_outgoing);
        Box::new(KillExplorerConversation::<SendingExplorerKill>::new(
            CONV_ID, state,
        ))
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
        handle_outgoing: bool,
    ) -> Box<KillExplorerConversation<WaitingKillExplorerResult>> {
        let state =
            WaitingKillExplorerResult::new(orch_context, EXPLORER_ID, PLANET_ID, handle_outgoing);
        Box::new(KillExplorerConversation::<WaitingKillExplorerResult>::new(
            CONV_ID, state,
        ))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);
        let conv = make_send_conv(test_ctx.clone(), false);
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
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_send_conv(test_ctx.clone(), false);
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
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        add_broken_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);
        let conv = make_send_conv(test_ctx.clone(), false);
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
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);
        let conv = make_send_conv(test_ctx.clone(), false);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(
            conv.get_priority(),
            ConvoKind::KillExplorer.priority().as_i32()
        );
    }

    #[test]
    fn wait_correct_transition_no_outgoing_handling() {
        let explorers_location = DashMap::new();

        explorers_location.insert(EXPLORER_ID, PLANET_ID);

        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, Some(explorers_location.clone()), ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone(), false);

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
            test_ctx.explorers_location.is_empty(),
            "Should have killed the only explorer saved in the map"
        );
    }

    #[test]
    fn wait_correct_transition_outgoing_handling() {
        let explorers_location = DashMap::new();
        explorers_location.insert(EXPLORER_ID, PLANET_ID);

        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, Some(explorers_location.clone()), ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone(), true);
        let orch_context = test_ctx.clone();

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
            orch_context.explorers_location.is_empty(),
            "Should have killed the only explorer saved in the map"
        );
    }

    #[test]
    fn wait_wrong_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone(), true);
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
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone(), false);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(ExplorerToOrchKind(
                ExplorerToOrchestratorKind::KillExplorerResult
            ))
        );
        assert_eq!(
            conv.get_priority(),
            ConvoKind::KillExplorer.priority().as_i32()
        );
    }
}
