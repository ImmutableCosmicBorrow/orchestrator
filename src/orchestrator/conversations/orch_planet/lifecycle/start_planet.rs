use crate::convo_manager::OrchContextRef;
use crate::globals::TIMEOUT;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::Duration;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::params::ConvoKind;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, PlanetCommunicator,
    PossibleExpectedKinds, PossibleMessage,
};
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;

//**Start Planet Conversation**
//
// This module manages the conversation between the Orchestrator and a Planet regarding the activation of its AI.
// It uses a Finite State Machine (FSM) to ensure that the start command and the confirmation result
// are handled in the correct order at compile time.
//
// The conversation flow starts by sending a start request and terminates once the planet
// confirms the AI has started.
// Marker struct for FSM state
//
// In the [`WaitingPlanetStartResult`] state, the conversation expects a
// [`PlanetToOrchestrator::StartPlanetAIResult`] message to confirm the planet has successfully initialized its AI.
// --- START PLANET CONVERSATION ---

define_conversation!(
    name: StartPlanetConversation
);

// --- SENDING PLANET START STATE DEFINITION ---

create_request_state!(
    state_name: SendingPlanetStart,
    conv_name: StartPlanetConversation,
    convo_kind: ConvoKind::StartPlanet,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        planet_id: ID,
    },
    entities_id_fn: |this: &StartPlanetConversation<SendingPlanetStart>| { (Some(this.state.planet_id), None) },
    transition_fn: send_planet_start_transition,
    methods_settings: {

    },
);

fn send_planet_start_transition(
    this: Box<StartPlanetConversation<SendingPlanetStart>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this
        .state
        .to_planet(this.state.planet_id, OrchestratorToPlanet::StartPlanetAI)
    {
        Ok(()) => {
            let next_state =
                WaitingPlanetStartResult::new(this.state.orch_context, this.state.planet_id);
            let next_conv =
                StartPlanetConversation::<WaitingPlanetStartResult>::new(this.id, next_state);
            Some(Box::new(next_conv))
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAIT START PLANET RESULT STATE DEFINITION ---

create_response_state!(
    state: WaitingPlanetStartResult,
    conv: StartPlanetConversation,
    convo_kind: ConvoKind::StartPlanet,
    timeout: Some(TIMEOUT),
    expected_msg: PlanetToOrchKind(PlanetToOrchestratorKind::StartPlanetAIResult),
    fields: {
        planet_id: ID
    },
    entities_id_closure: |this: &StartPlanetConversation<WaitingPlanetStartResult>| { (Some(this.state.planet_id), None) },
    transition: wait_planet_start_res_transition,
    methods_settings: {

    },
);

/// Transition Function for [`WaitingPlanetStartResult`] state:
///
/// Returns:
///
/// [None] if the start result is successfully received, ending the conversation.
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different from [`PlanetToOrchestrator::StartPlanetAIResult`]
fn wait_planet_start_res_transition(
    this: Box<StartPlanetConversation<WaitingPlanetStartResult>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StartPlanetAIResult {
        planet_id,
    })) = msg
    {
        log_internal(
            LogTarget::Conversations,
            Channel::Info,
            payload!(
                action : "Started Planet, closing conversation",
                planet_id : planet_id,
                conversation_id : this.id,
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
    use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::StartPlanetAIResult;
    use crossbeam_channel::unbounded;

    const CONV_ID: ID = 1;
    const PLANET_ID: ID = 2;

    // --- Helper functions ---

    fn make_send_conv(
        orch_context: OrchContextRef,
    ) -> Box<StartPlanetConversation<SendingPlanetStart>> {
        let state = SendingPlanetStart::new(orch_context, PLANET_ID);
        Box::new(StartPlanetConversation::<SendingPlanetStart>::new(
            CONV_ID, state,
        ))
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
    ) -> Box<StartPlanetConversation<WaitingPlanetStartResult>> {
        let state = WaitingPlanetStartResult::new(orch_context, PLANET_ID);
        Box::new(StartPlanetConversation::<WaitingPlanetStartResult>::new(
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
            .expect("Should transition to next state");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PlanetToOrchKind(StartPlanetAIResult))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
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
            .expect("Should transition to next state");
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("sender to planet {PLANET_ID} not found"))
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
        assert_eq!(
            conv.get_priority(),
            ConvoKind::StartPlanet.priority().as_i32()
        );
    }

    #[test]
    fn wait_correct_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StartPlanetAIResult {
            planet_id: PLANET_ID,
        });
        let result = conv.transition(Some(msg));
        assert!(
            result.is_none(),
            "Conversation should terminate successfully (None)"
        );
    }

    #[test]
    fn wait_wrong_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::SunrayAck {
            planet_id: PLANET_ID,
        });
        let result = conv
            .transition(Some(wrong_msg))
            .expect("Should return an ErrorState");
        assert_eq!(result.get_id(), CONV_ID);
        assert_eq!(
            result.get_error_details(),
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
            Some(PlanetToOrchKind(StartPlanetAIResult))
        );
        assert_eq!(
            conv.get_priority(),
            ConvoKind::StartPlanet.priority().as_i32()
        );
    }
}
