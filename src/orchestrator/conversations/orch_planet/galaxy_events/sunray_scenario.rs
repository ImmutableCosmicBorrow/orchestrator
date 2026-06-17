use crate::convo_manager::OrchContextRef;
use crate::globals::get_convo_timeout;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
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
use std::time::Duration;

// --- CONVERSATION FSM WRAPPER DEFINITION ---

define_conversation!(
    name: SunrayConversation
);

// --- SEND SUNRAY STATE DEFINITION ---
create_request_state!(
    state_name: SendSunray,
    conv_name: SunrayConversation,
           convo_kind: ConvoKind::Sunray,
    timeout: None,
    expected_msg: None,
    fields: {
        planet_id: ID,
    },
    entities_id_fn: |this: &SunrayConversation<SendSunray>| { (Some(this.state.planet_id), None) },
    transition_fn: send_sunray_transition,
    methods_settings: {

    },
);

/// Transition Function for [`SendSunray`] state:
///
/// Returns:
///
/// [`ErrorState`] with [`CommonErrorTypes::MessageToPlanetFailed`] if the message has not been correctly sent to the planet
///
/// [`ErrorState`] with [`CommonErrorTypes::PlanetSenderNotFound`] if the sender to the planet is not in the list
///
/// The next state: [`SunrayConversation<WaitingSunrayAck>`] if the sunray was sent successfully.
fn send_sunray_transition(
    this: Box<SunrayConversation<SendSunray>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    let sunray = this.state.orch_context.forge.generate_sunray();
    match this
        .state
        .to_planet(this.state.planet_id, OrchestratorToPlanet::Sunray(sunray))
    {
        //Correctly message sending
        Ok(()) => {
            let planet_id = this.state.planet_id;
            let next_state = WaitingSunrayAck::new(this.state.orch_context, planet_id);
            let next_conv = SunrayConversation::<WaitingSunrayAck>::new(this.id, next_state);
            Some(Box::new(next_conv) as Box<dyn Conversation + Send + Sync>)
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAIT SUNRAY ACK STATE DEFINITION ---

create_response_state!(
    state: WaitingSunrayAck,
    conv: SunrayConversation,
        convo_kind: ConvoKind::Sunray,
        timeout: Some(crate::orchestrator::conversations::params::sunray_ack_timeout()),
    expected_msg: PlanetToOrchKind(PlanetToOrchestratorKind::SunrayAck),
    fields: {
        planet_id: ID,
    },
    entities_id_closure: |this: &SunrayConversation<WaitingSunrayAck>| { (Some(this.state.planet_id), None) },
    transition: wait_sunray_ack_transition,
    methods_settings: {
        on_timeout: on_timeout,
    },
);

/// Transition Function for [`WaitingSunrayAck`] state:
///
/// Returns:
///
/// [None] if the [`PlanetToOrchestrator::SunrayAck`] is successfully received, ending the conversation.
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different from the expected one.
fn wait_sunray_ack_transition(
    this: Box<SunrayConversation<WaitingSunrayAck>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::SunrayAck { planet_id })) = msg
    {
        log_internal(
            LogTarget::AsteroidsSunrays,
            Channel::Trace,
            payload!(
                action : "Planet received the Sunray, closing conversation",
                planet_id : planet_id,
                conversation_id : this.id
            ),
        );
        return None;
    }

    //Wrong Message, transition to error state
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}

//On timeout function, this state does not use the default implementation of the trait
/// Called when the conversation times out waiting for `SunrayAck`.
/// Logs a warning - the conversation is simply terminated.
fn on_timeout(this: Box<SunrayConversation<WaitingSunrayAck>>) {
    log_internal(
        LogTarget::AsteroidsSunrays,
        Channel::Warning,
        payload!(
            action : "Sunray conversation timed out waiting for planet acknowledgment",
            planet_id : this.state.planet_id,
            conversation_id : this.id,
            timeout_secs : get_convo_timeout().as_secs()
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::orch_planet::test_utils::{
        add_broken_planet_sender, add_working_planet_sender, make_test_context,
    };
    use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
    use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::SunrayAck;
    use crossbeam_channel::unbounded;

    const CONV_ID: ID = 1;
    const PLANET_ID: ID = 2;

    // --- Helper functions ---

    fn make_sunray_conversation_send(
        orch_context: OrchContextRef,
    ) -> Box<SunrayConversation<SendSunray>> {
        let state = SendSunray::new(orch_context, PLANET_ID);
        Box::new(SunrayConversation::<SendSunray>::new(CONV_ID, state))
    }

    fn make_sunray_conversation_wait(
        orch_context: OrchContextRef,
    ) -> Box<SunrayConversation<WaitingSunrayAck>> {
        let state = WaitingSunrayAck::new(orch_context, PLANET_ID);
        Box::new(SunrayConversation::<WaitingSunrayAck>::new(CONV_ID, state))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_planet_sender(test_ctx.channels_manager.as_ref(), PLANET_ID);
        let conv = make_sunray_conversation_send(test_ctx.clone());
        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PlanetToOrchKind(SunrayAck))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn send_missing_sender() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_sunray_conversation_send(test_ctx.clone());
        let next_conv = conv
            .transition(None)
            .expect("Should transition to error state");
        assert!(next_conv.get_expected_kind().is_none());
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
        let conv = make_sunray_conversation_send(test_ctx.clone());
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
    fn send_sunray_getters() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_sunray_conversation_send(test_ctx.clone());
        // get_id
        assert_eq!(conv.get_id(), CONV_ID);
        // get_entity_ids
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        // get_expected_kind (should be None in SendSunray state)
        assert_eq!(conv.get_expected_kind(), None);
        // send state should not have a timeout - it's not waiting for messages
        assert_eq!(conv.get_timeout(), None);
        // get_priority
        assert_eq!(conv.get_priority(), ConvoKind::Sunray.priority().as_i32());
    }

    #[test]
    fn wait_correct_transition() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_sunray_conversation_wait(test_ctx.clone());
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::SunrayAck {
            planet_id: PLANET_ID,
        });
        let result = conv.transition(Some(msg));
        assert!(result.is_none());
    }

    #[test]
    fn wait_wrong_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_sunray_conversation_wait(test_ctx.clone());
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StartPlanetAIResult {
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
    fn waiting_sunray_getters() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_sunray_conversation_wait(test_ctx.clone());
        // get_id
        assert_eq!(conv.get_id(), CONV_ID);
        // get_entity_ids
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        // get_expected_kind (should be Some(SunrayAck))
        assert_eq!(conv.get_expected_kind(), Some(PlanetToOrchKind(SunrayAck)));
        // get_priority
        assert_eq!(conv.get_priority(), ConvoKind::Sunray.priority().as_i32());
    }

    // --- Timeout Feature Tests ---

    #[test]
    fn waiting_sunray_has_timeout_config() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_sunray_conversation_wait(test_ctx.clone());

        // Verify timeout is configured
        assert!(conv.get_timeout().is_some());
        assert_eq!(
            conv.get_timeout(),
            Some(crate::orchestrator::conversations::params::sunray_ack_timeout())
        );
    }

    #[test]
    fn waiting_sunray_timeout_logs_and_terminates() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_sunray_conversation_wait(test_ctx.clone());

        // on_timeout should just log and return (not panic)
        // This test verifies it doesn't panic
        conv.on_timeout();
        // If we get here, the test passes - on_timeout completed without panic
    }
}
