use crate::convo_manager::OrchContextRef;
use crate::globals::TIMEOUT;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::Duration;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::params::EventKind;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, KillExplorersList,
    PlanetCommunicator, PossibleExpectedKinds, PossibleMessage,
};
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::KillPlanetResult;
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::utils::ID;

//**Kill Planet Conversation**
//
// This module manages the complex process of destroying a planet.
// It uses an FSM to send the kill command, wait for confirmation, and then
// sends via its method [`Conversation::get_kill_explorers_vec`] the IDs of the explorers on the planet
// so that the Orchestrator can kill them
//
//
// The conversation starts in the [`SendPlanetKill`] state, which sends an
// [`OrchestratorToPlanet::KillPlanet`] message when the [`Conversation::transition`] method is called.
// --- INTERNAL STATE CONVERSATION ---

define_conversation!(
    name: KillPlanetConversation
);

// --- SEND KILL PLANET DEFINITION ---

create_request_state!(
    state_name: SendPlanetKill,
    conv_name: KillPlanetConversation,
    priority: EventKind::KillPlanet.priority_i32(),
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        planet_id: ID,
    },
    entities_id_fn: |this: &KillPlanetConversation<SendPlanetKill>| { (Some(this.state.planet_id), None) },
    transition_fn: send_kill_planet_transition,
    methods_settings: {

    },
);

/// Transition Function for [`SendPlanetKill`] state:
///
/// Returns:
///
/// [`ErrorState`] if the message to the planet fails or the sender is not found.
///
/// [`KillPlanetConversation<WaitingPlanetKillResult>`] if the kill command was sent successfully.
fn send_kill_planet_transition(
    this: Box<KillPlanetConversation<SendPlanetKill>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this
        .state
        .to_planet(this.state.planet_id, OrchestratorToPlanet::KillPlanet)
    {
        Ok(()) => {
            let state_struct =
                WaitingPlanetKillResult::new(this.state.orch_context, this.state.planet_id);
            let next_state =
                KillPlanetConversation::<WaitingPlanetKillResult>::new(this.id, state_struct);
            Some(Box::new(next_state))
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAITING KILL PLANET RESPONSE DEFINITION ---

create_response_state!(
    state: WaitingPlanetKillResult,
    conv: KillPlanetConversation,
    priority: EventKind::KillPlanet.priority_i32(),
    timeout: Some(TIMEOUT),
    expected_msg: PlanetToOrchKind(KillPlanetResult),
    fields: {
        planet_id: ID,
    },
    entities_id_closure: |this: &KillPlanetConversation<WaitingPlanetKillResult>| { (Some(this.state.planet_id), None) },
    transition: wait_planet_kill_res_transition,
    methods_settings: {
        get_kill_exp_vec: |this: &KillPlanetConversation<WaitingPlanetKillResult>| { Some((this.state.get_explorers_in_planet(this.state.planet_id), false)) }
    },
);

impl WaitingPlanetKillResult {
    //Helper function to find all explorers in the planet
    fn get_explorers_in_planet(&self, target_planet: ID) -> Vec<(ID, ID)> {
        self.orch_context
            .explorers_location
            .iter()
            .filter(|r| *r.value() == target_planet) // Access value via the guard
            .map(|r| (*r.key(), *r.value())) // Dereference/copy the data
            .collect()
    }
}

/// Transition Function for [`WaitingPlanetKillResult`] state:
///
/// Returns:
///
/// None to end the conversation if planet is killed correctly - NOTE: to kill explorers on this planet, we return the list of them
/// through the dedicated method of the trait and let the Orchestrator take care of that
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different then the expected [`PlanetToOrchestrator::KillPlanetResult`]
fn wait_planet_kill_res_transition(
    this: Box<KillPlanetConversation<WaitingPlanetKillResult>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::KillPlanetResult {
        planet_id,
    })) = msg
    {
        log_internal(
            LogTarget::Conversations,
            Channel::Info,
            payload!(
                action : "Killed Planet",
                planet_id : planet_id,
                conversation_id : this.id
            ),
        );

        return None;
    }

    //Wrong Message, close conversation
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::orch_planet::test_utils::{
        add_broken_planet_sender, add_working_planet_sender, make_test_context,
    };
    use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
    use crossbeam_channel::unbounded;

    const CONV_ID: ID = 100;
    const PLANET_ID: ID = 200;
    const EXPLORER_ID_1: ID = 301;
    const EXPLORER_ID_2: ID = 302;

    // --- Helper functions ---

    fn make_send_conv(orch_context: OrchContextRef) -> Box<KillPlanetConversation<SendPlanetKill>> {
        let state = SendPlanetKill::new(orch_context, PLANET_ID);
        Box::new(KillPlanetConversation::<SendPlanetKill>::new(
            CONV_ID, state,
        ))
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
    ) -> Box<KillPlanetConversation<WaitingPlanetKillResult>> {
        let state = WaitingPlanetKillResult::new(orch_context, PLANET_ID);
        Box::new(KillPlanetConversation::<WaitingPlanetKillResult>::new(
            CONV_ID, state,
        ))
    }

    fn make_wait_conv_with_explorers(
        orch_context: OrchContextRef,
    ) -> Box<KillPlanetConversation<WaitingPlanetKillResult>> {
        orch_context
            .explorers_location
            .insert(EXPLORER_ID_1, PLANET_ID);
        orch_context
            .explorers_location
            .insert(EXPLORER_ID_2, PLANET_ID);
        orch_context.explorers_location.insert(999, 888); // Explorer on a different planet (should be ignored)

        let state = WaitingPlanetKillResult::new(orch_context, PLANET_ID);
        Box::new(KillPlanetConversation::<WaitingPlanetKillResult>::new(
            CONV_ID, state,
        ))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_planet_sender(test_ctx.channels_manager.as_ref(), PLANET_ID);
        let conv = make_send_conv(test_ctx.clone());
        let next_conv = conv
            .transition(None)
            .expect("Should transition to WaitingPlanetKillResult");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PlanetToOrchKind(KillPlanetResult))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn send_missing_sender() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_send_conv(test_ctx.clone());
        let next_conv = conv
            .transition(None)
            .expect("Should transition to ErrorState");
        assert!(next_conv.get_error_details().is_some());
        assert_eq!(
            next_conv.get_error_details().unwrap(),
            format!("sender to planet {PLANET_ID} not found")
        );
    }

    #[test]
    fn send_message_failure() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        add_broken_planet_sender(test_ctx.channels_manager.as_ref(), PLANET_ID);
        let conv = make_send_conv(test_ctx.clone());
        let next_conv = conv.transition(None).expect("Should return an ErrorState");
        let error_msg = next_conv
            .get_error_details()
            .expect("Should return an Error Details String");
        assert_eq!(
            error_msg,
            format!("failed to send message to planet {PLANET_ID}")
        );
    }

    #[test]
    fn send_getters() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_send_conv(test_ctx.clone());
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), EventKind::KillPlanet.priority_i32());
    }

    #[test]
    fn wait_success_and_cleanup() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv_with_explorers(test_ctx.clone());

        let explorers = conv.get_kill_explorers_vec();
        assert!(explorers.is_some());
        let (explorers_list, _) = explorers.unwrap();
        assert_eq!(explorers_list.len(), 2);

        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::KillPlanetResult {
            planet_id: PLANET_ID,
        });
        let next_conv = conv.transition(Some(msg));
        // After planet kill, the conversation should end (return None)
        assert!(
            next_conv.is_none(),
            "Conversation should end and return None"
        );
    }

    #[test]
    fn wait_wrong_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::AsteroidAck {
            planet_id: PLANET_ID,
            rocket: None,
        });
        let next_conv = conv
            .transition(Some(wrong_msg))
            .expect("Should return an ErrorState");
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
        let conv = make_wait_conv(test_ctx.clone());
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PlanetToOrchKind(KillPlanetResult))
        );
        assert_eq!(conv.get_priority(), EventKind::KillPlanet.priority_i32());
    }
}
