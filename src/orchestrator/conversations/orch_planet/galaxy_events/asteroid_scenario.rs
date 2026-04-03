use crate::convo_manager::OrchContextRef;
use crate::globals::TIMEOUT;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::orch_planet::lifecycle::kill_planet::{
    KillPlanetConversation, SendPlanetKill,
};
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

/// Default timeout duration for waiting for an Asteroid acknowledgment.
/// Asteroids are critical events, so the planet must respond promptly.
const ASTEROID_ACK_TIMEOUT: Duration = Duration::from_secs(10);

define_conversation!(
    name: AsteroidConversation
);

create_request_state!(
    state_name: SendingAsteroid,
    conv_name: AsteroidConversation,
    priority: 4,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        planet_id: ID,
    },
    entities_id_fn: |this: &AsteroidConversation<SendingAsteroid>| { (Some(this.state.planet_id), None) },
    transition_fn: sending_asteroid_transition,
    methods_settings: { },
);

///Transition Function for [`SendingAsteroid`] state:
///
/// Returns:
///
/// [`ErrorState`] with [`CommonErrorTypes::MessageToPlanetFailed`] if the message has not been correctly sent to the planet
///
/// [`ErrorState`] with [`CommonErrorTypes::PlanetSenderNotFound`] if the sender to the planet is not in the [`crate::channels_manager::OrchToPlanetSenders`] list in the [`crate::channels_manager::ChannelsManager`]
///
/// [`AsteroidConversation<WaitingAsteroidAck>`] if the asteroid has been correctly sent, going to the next state
// TODO: check if we can remove allows
#[allow(clippy::unnecessary_wraps)]
#[allow(clippy::boxed_local)]
fn sending_asteroid_transition(
    this: Box<AsteroidConversation<SendingAsteroid>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    let asteroid = this.state.orch_context.forge.generate_asteroid();
    match this.state.to_planet(
        this.state.planet_id,
        OrchestratorToPlanet::Asteroid(asteroid),
    ) {
        Ok(()) => {
            let state_struct =
                WaitingAsteroidAck::new(this.state.orch_context, this.state.planet_id);
            let next_state = AsteroidConversation::<WaitingAsteroidAck>::new(this.id, state_struct);
            Some(Box::new(next_state) as Box<dyn Conversation + Send + Sync>)
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAITING ASTEROID ACK ---

create_response_state!(
    state: WaitingAsteroidAck,
    conv: AsteroidConversation,
    priority: 4,
    timeout: Some(ASTEROID_ACK_TIMEOUT),
    expected_msg: PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck),
    fields: { planet_id: ID},
    entities_id_closure: |this: &AsteroidConversation<WaitingAsteroidAck>| { (Some(this.state.planet_id), None) },
    transition: waiting_asteroid_ack_transition,
    methods_settings: { },
);

///Transition Function for [`WaitingAsteroidAck`] state:
///
/// Returns:
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different from the expected one [`PlanetToOrchestrator::AsteroidAck`]
///
/// [None] if the planet defends itself with a rocket, ending the conversation
///
/// [`KillPlanetConversation<SendPlanetKill>`] if the planet cannot defend himself and has to be killed with a [`KillPlanetConversation`]
// TODO: check if we can remove allows
#[allow(clippy::boxed_local)]
#[allow(clippy::needless_pass_by_value)]
fn waiting_asteroid_ack_transition(
    this: Box<AsteroidConversation<WaitingAsteroidAck>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::AsteroidAck {
        planet_id,
        rocket,
    })) = msg
    {
        if rocket.is_some() {
            log_internal(
                LogTarget::AsteroidsSunrays,
                Channel::Debug,
                payload!(
                        action : "Planet received an asteroid and defends with a rocket, closing conversation",
                        planet_id : planet_id,
                        conversation_id : this.id
                ),
            );
            return None;
        }

        log_internal(
            LogTarget::AsteroidsSunrays,
            Channel::Info,
            payload!(
                action : "Planet received an asteroid and did not defend, so it will be killed",
                planet_id : planet_id,
                conversation_id : this.id
            ),
        );

        //Transition to KillStateConversation
        let new_state = KillPlanetConversation::<SendPlanetKill>::new(
            this.id,
            SendPlanetKill::new(this.state.orch_context, this.state.planet_id),
        );
        return Some(Box::new(new_state) as Box<dyn Conversation + Send + Sync>);
    }
    //Wrong message arrived, transitioning to error state
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::TIMEOUT;
    use crate::orchestrator::conversations::util::get_test_forge;
    use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: ID = 1;
    const PLANET_ID: ID = 2;

    type PlanetSenders = Arc<Mutex<HashMap<ID, crossbeam_channel::Sender<OrchestratorToPlanet>>>>;

    struct MakeSendersResult(
        PlanetSenders,
        crossbeam_channel::Receiver<OrchestratorToPlanet>,
    );

    // --- Helper functions ---
    fn make_senders_with(planet_id: ID) -> MakeSendersResult {
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();
        MakeSendersResult(Arc::new(Mutex::new(HashMap::from([(planet_id, tx)]))), rx)
    }

    fn make_empty_senders() -> PlanetSenders {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn make_to_planet_struct(planet_id: ID, senders: PlanetSenders) -> ToPlanetStruct {
        ToPlanetStruct {
            planet_id,
            planets_senders: senders,
        }
    }

    fn make_empty_explorer_refs() -> (ExplorersLocationRef, OrchToExplorerSenders) {
        (
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashMap::new())),
        )
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(senders: PlanetSenders) -> Box<AsteroidConversation<SendingAsteroid>> {
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let forge = get_test_forge();
        let (explorers_location, explorers_senders) = make_empty_explorer_refs();
        let state = SendingAsteroid::new(to_planet, forge, explorers_location, explorers_senders);
        Box::new(AsteroidConversation::<SendingAsteroid>::new(CONV_ID, state))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv() -> Box<AsteroidConversation<WaitingAsteroidAck>> {
        let senders = make_empty_senders();
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let (explorers_location, explorers_senders) = make_empty_explorer_refs();
        let state = WaitingAsteroidAck::new(to_planet, explorers_senders, explorers_location);
        Box::new(AsteroidConversation::<WaitingAsteroidAck>::new(
            CONV_ID, state,
        ))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let MakeSendersResult(senders, _rx) = make_senders_with(PLANET_ID);
        let conv = make_send_conv(senders);
        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PossibleExpectedKinds::PlanetToOrchKind(
                PlanetToOrchestratorKind::AsteroidAck
            ))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn send_missing_sender() {
        let senders = make_empty_senders();
        let conv = make_send_conv(senders);
        let next_conv = conv
            .transition(None)
            .expect("Should transition to error state");
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("sender to planet {PLANET_ID} not found"))
        );
        assert!(next_conv.transition(None).is_none());
    }

    #[test]
    fn send_message_failure() {
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();
        drop(rx);
        let senders = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx)])));
        let conv = make_send_conv(senders);
        let next_conv = conv.transition(None).expect("Should return an ErrorState");
        let error_msg = next_conv
            .get_error_details()
            .expect("Should have error details");
        assert_eq!(
            error_msg,
            format!("failed to send message to planet {PLANET_ID}")
        );
        assert!(next_conv.transition(None).is_none());
    }

    #[test]
    fn send_getters() {
        let MakeSendersResult(senders, _rx) = make_senders_with(PLANET_ID);
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let forge = get_test_forge();
        let (explorers_location, explorers_senders) = make_empty_explorer_refs();
        let state = SendingAsteroid::new(to_planet, forge, explorers_location, explorers_senders);
        let conv = AsteroidConversation::<SendingAsteroid>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 4);
    }

    #[test]
    fn wait_correct_no_rocket() {
        let conv = make_wait_conv();
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::AsteroidAck {
            planet_id: PLANET_ID,
            rocket: None,
        });
        let result = conv
            .transition(Some(msg))
            .expect("Should transition to KillPlanetConversation");
        assert_eq!(result.get_id(), CONV_ID);
    }

    #[test]
    fn wait_wrong_message() {
        let conv = make_wait_conv();
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StartPlanetAIResult {
            planet_id: PLANET_ID,
        });
        let result = conv
            .transition(Some(wrong_msg))
            .expect("Should return an ErrorState");
        assert_eq!(
            result.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
        assert!(result.transition(None).is_none());
    }

    #[test]
    fn wait_getters() {
        let senders = make_empty_senders();
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let (explorers_location, explorers_senders) = make_empty_explorer_refs();
        let state = WaitingAsteroidAck::new(to_planet, explorers_senders, explorers_location);
        let conv = AsteroidConversation::<WaitingAsteroidAck>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::PlanetToOrchKind(
                PlanetToOrchestratorKind::AsteroidAck
            ))
        );
        assert_eq!(conv.get_priority(), 4);
    }

    #[test]
    fn wait_defends_with_rocket() {
        let conv = make_wait_conv();
        // Create a dummy Rocket value for testing using unsafe since Rocket::new is pub(crate)
        // SAFETY: Rocket only contains a private unit field `_private: ()`, which is a ZST (zero-sized type)
        let dummy_rocket: common_game::components::rocket::Rocket = unsafe { std::mem::zeroed() };
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::AsteroidAck {
            planet_id: PLANET_ID,
            rocket: Some(dummy_rocket),
        });
        let result = conv.transition(Some(msg));
        assert!(
            result.is_none(),
            "Conversation should terminate when planet defends with rocket"
        );
    }

    // --- Timeout Feature Tests ---

    #[test]
    fn waiting_asteroid_has_timeout_config() {
        let conv = make_wait_conv();

        // Verify timeout is configured
        assert!(conv.get_timeout().is_some());
        assert_eq!(conv.get_timeout(), Some(ASTEROID_ACK_TIMEOUT));
    }

    #[test]
    #[should_panic(
        expected = "Conversation 1 timed out waiting for Some(PlanetToOrchKind(AsteroidAck))"
    )]
    fn waiting_asteroid_timeout_logs_and_terminates() {
        let conv = make_wait_conv();

        // on_timeout should panic
        // This test verifies it does
        conv.on_timeout();
    }

    #[test]
    fn sending_asteroid_has_default_timeout() {
        let MakeSendersResult(senders, _rx) = make_senders_with(PLANET_ID);
        let conv = make_send_conv(senders);

        // Sending states should not have timeout - they're not waiting for messages
        assert_eq!(conv.get_timeout(), Some(TIMEOUT));
    }
}
*/
