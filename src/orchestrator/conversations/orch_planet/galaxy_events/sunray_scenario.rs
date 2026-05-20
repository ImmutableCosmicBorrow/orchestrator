use crate::convo_manager::OrchContextRef;
use crate::globals::TIMEOUT;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
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

/// Default timeout duration for waiting for a Sunray acknowledgment.
/// The planet should respond quickly to sunray events.
const SUNRAY_ACK_TIMEOUT: Duration = Duration::from_secs(5);

// --- CONVERSATION FSM WRAPPER DEFINITION ---

define_conversation!(
    name: SunrayConversation
);

// --- SEND SUNRAY STATE DEFINITION ---
create_request_state!(
    state_name: SendSunray,
    conv_name: SunrayConversation,
    priority: 1,
    timeout: Some(TIMEOUT),
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
    priority: 1,
    timeout: Some(SUNRAY_ACK_TIMEOUT),
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
            timeout_secs : SUNRAY_ACK_TIMEOUT.as_secs()
        ),
    );
}

/*

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::util::get_test_forge;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: ID = 1;
    const PLANET_ID: ID = 2;

    struct MakeSendersWithResult(
        Arc<Mutex<HashMap<ID, crossbeam_channel::Sender<OrchestratorToPlanet>>>>,
        crossbeam_channel::Receiver<OrchestratorToPlanet>,
    );

    // --- Helper functions ---
    fn make_senders_with(planet_id: ID) -> MakeSendersWithResult {
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();
        MakeSendersWithResult(Arc::new(Mutex::new(HashMap::from([(planet_id, tx)]))), rx)
    }

    fn make_empty_senders()
    -> Arc<Mutex<HashMap<ID, crossbeam_channel::Sender<OrchestratorToPlanet>>>> {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn make_to_planet_struct(
        planet_id: ID,
        senders: Arc<Mutex<HashMap<ID, crossbeam_channel::Sender<OrchestratorToPlanet>>>>,
    ) -> ToPlanetStruct {
        ToPlanetStruct {
            planet_id,
            planets_senders: senders,
        }
    }

    fn make_sunray_conversation_send(
        forge_ref: Arc<Forge>,
        senders: Arc<Mutex<HashMap<ID, crossbeam_channel::Sender<OrchestratorToPlanet>>>>,
    ) -> Box<SunrayConversation<SendSunray>> {
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let state = SendSunray::new(to_planet, forge_ref);
        Box::new(SunrayConversation::<SendSunray>::new(CONV_ID, state))
    }

    fn make_sunray_conversation_wait() -> Box<SunrayConversation<WaitingSunrayAck>> {
        Box::new(SunrayConversation::<WaitingSunrayAck>::new(
            CONV_ID, PLANET_ID,
        ))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let forge_ref = get_test_forge();
        let MakeSendersWithResult(senders, _rx) = make_senders_with(PLANET_ID);
        let conv = make_sunray_conversation_send(forge_ref, senders);
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
        let forge_ref = get_test_forge();
        let senders = make_empty_senders();
        let conv = make_sunray_conversation_send(forge_ref, senders);
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
        let forge_ref = get_test_forge();
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();
        drop(rx);
        let senders = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx)])));
        let conv = make_sunray_conversation_send(forge_ref, senders);
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
        let forge_ref = get_test_forge();
        let MakeSendersWithResult(senders, _rx) = make_senders_with(PLANET_ID);
        let to_planet = make_to_planet_struct(PLANET_ID, senders.clone());
        let state = SendSunray::new(to_planet, forge_ref);
        let conv = SunrayConversation::<SendSunray>::new(CONV_ID, state);
        // get_id
        assert_eq!(conv.get_id(), CONV_ID);
        // get_entity_ids
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        // get_expected_kind (should be None in SendSunray state)
        assert_eq!(conv.get_expected_kind(), None);
        // get_priority
        assert_eq!(conv.get_priority(), 1);
    }

    #[test]
    fn wait_correct_transition() {
        let conv = make_sunray_conversation_wait();
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::SunrayAck {
            planet_id: PLANET_ID,
        });
        let result = conv.transition(Some(msg));
        assert!(result.is_none());
    }

    #[test]
    fn wait_wrong_message() {
        let conv = make_sunray_conversation_wait();
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
        let conv = SunrayConversation::<WaitingSunrayAck>::new(CONV_ID, PLANET_ID);
        // get_id
        assert_eq!(conv.get_id(), CONV_ID);
        // get_entity_ids
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        // get_expected_kind (should be Some(SunrayAck))
        assert_eq!(conv.get_expected_kind(), Some(PlanetToOrchKind(SunrayAck)));
        // get_priority
        assert_eq!(conv.get_priority(), 1);
    }

    // --- Timeout Feature Tests ---

    #[test]
    fn waiting_sunray_has_timeout_config() {
        let conv = make_sunray_conversation_wait();

        // Verify timeout is configured
        assert!(conv.get_timeout().is_some());
        assert_eq!(conv.get_timeout(), Some(SUNRAY_ACK_TIMEOUT));
    }

    #[test]
    fn waiting_sunray_timeout_logs_and_terminates() {
        let conv = make_sunray_conversation_wait();

        // on_timeout should just log and return (not panic)
        // This test verifies it doesn't panic
        conv.on_timeout();
        // If we get here, the test passes - on_timeout completed without panic
    }
}
*/
