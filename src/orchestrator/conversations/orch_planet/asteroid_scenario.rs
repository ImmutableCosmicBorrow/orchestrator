use crate::logging_utils::log_internal;
use crate::orchestrator::conversations::orch_planet::kill_planet::{
    KillPlanetConversation, SendPlanetKill,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    SendersToExplorer, ToPlanetError, ToPlanetStruct,
};
use crate::orchestrator::{ExplorerBagContent, ExplorersLocationRef};
use crate::payload;
use common_game::components::forge::Forge;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
use std::sync::Arc;
use std::time::Duration;

/// Default timeout duration for waiting for an Asteroid acknowledgment.
/// Asteroids are critical events, so the planet must respond promptly.
const ASTEROID_ACK_TIMEOUT: Duration = Duration::from_secs(10);

///**Asteroid Conversation**
///
/// This Module deals with the conversation between the Orchestrator and a Planet regarding asteroids, using FSM to ensure the correctness
/// of messages orders at compile time
///Marker Struct for FSM state
///
/// The conversation starts in the [`SendingAsteroid`] state, this state does not expect any message as it sends a [`OrchestratorToPlanet::Asteroid`]
/// when the [`Conversation::transition`] method is called
pub(crate) struct SendingAsteroid {
    ///A struct containing fields to send messages to a planet, used if a planet cannot defend and has to be killed
    to_planet_struct: ToPlanetStruct,
    ///Atomic Reference to the forge to create [Asteroid]
    forge: Arc<Forge>,
    ///Struct to send messages to explorer, used by subsequent states
    explorers_senders: SendersToExplorer,
    ///Reference to the list of explorers locations, used by subsequent states
    explorers_location_ref: ExplorersLocationRef,
}

impl SendingAsteroid {
    ///Constructor for [`SendingAsteroid`] state struct
    pub(crate) fn new(
        to_planet_struct: ToPlanetStruct,
        forge: Arc<Forge>,
        explorers_location_ref: ExplorersLocationRef,
        explorers_senders: SendersToExplorer,
    ) -> Self {
        Self {
            to_planet_struct,
            forge,
            explorers_senders,
            explorers_location_ref,
        }
    }
}

///Marker Struct for FSM state
///
/// In the [`WaitingAsteroidAck`] state, the conversation expects a [`PlanetToOrchestrator::AsteroidAck`] message to decide
/// whether to kill the planet using the [`KillPlanetConversation`] or closing the conversations if the planet defends himself
struct WaitingAsteroidAck {
    ///A struct containing fields to send messages to a planet, used if a planet cannot defend and has to be killed
    to_planet_struct: ToPlanetStruct,
    ///Struct to send messages to explorer, used by subsequent states
    explorers_senders: SendersToExplorer,
    ///Reference to the list of explorers locations, used by subsequent states
    explorers_location_ref: ExplorersLocationRef,
}

impl WaitingAsteroidAck {
    ///The constructor for [`WaitingAsteroidAck`] state struct
    fn new(
        to_planet_struct: ToPlanetStruct,
        explorers_senders: SendersToExplorer,
        explorers_location_ref: ExplorersLocationRef,
    ) -> Self {
        Self {
            to_planet_struct,
            explorers_senders,
            explorers_location_ref,
        }
    }
}

///This is the generic FSM struct, it takes the generic type State to ensure only methods of that state can be called
pub(crate) struct AsteroidConversation<State> {
    ///Conversation ID
    id: ID,
    ///Optional expected message to trigger the conversation
    expected_message: Option<PossibleExpectedKinds>,
    ///State of the FSM
    state: State,
}

//SENDING ASTEROID IMPLEMENTATION
impl Conversation<ExplorerBagContent> for AsteroidConversation<SendingAsteroid> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (Some(self.state.to_planet_struct.planet_id), None)
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    ///Transition Function for [`SendingAsteroid`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToPlanetFailed`] if the message has not been correctly sent to the planet
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::PlanetSenderNotFound`] if the sender to the planet is not in the [`SendersToPlanet`] list
    ///
    /// [`AsteroidConversation<WaitingAsteroidAck>`] if the asteroid has been correctly sent, going to the next state  
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        let asteroid = self.state.forge.generate_asteroid();
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::Asteroid(asteroid))
        {
            Ok(()) => {
                let state_struct = WaitingAsteroidAck::new(
                    self.state.to_planet_struct,
                    self.state.explorers_senders,
                    self.state.explorers_location_ref,
                );
                let next_state =
                    AsteroidConversation::<WaitingAsteroidAck>::new(self.id, state_struct);
                Some(Box::new(next_state))
            }
            Err(err) => {
                let error = match err {
                    ToPlanetError::SendingMessageFailure(id) => {
                        CommonErrorTypes::MessageToPlanetFailed(id)
                    }
                    ToPlanetError::SenderNotFound(id) => CommonErrorTypes::PlanetSenderNotFound(id),
                };
                let error_state = ErrorState::new(Box::new(error), self.id);
                Some(Box::new(error_state)
                    as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
            }
        }
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl AsteroidConversation<SendingAsteroid> {
    pub(crate) fn new(id: ID, state: SendingAsteroid) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

//WAITING ACK IMPLEMENTATION
impl Conversation<ExplorerBagContent> for AsteroidConversation<WaitingAsteroidAck> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (Some(self.state.to_planet_struct.planet_id), None)
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    ///Transition Function for [`SendingAsteroid`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different from the expected one [`PlanetToOrchestrator::AsteroidAck`]
    ///
    /// [None] if the planet defends itself with a rocket, ending the conversation
    ///
    /// [`KillPlanetConversation<SendPlanetKill>`] if the planet cannot defend himself and has to be killed with a [`KillPlanetConversation`]  
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::AsteroidAck {
            planet_id,
            rocket,
        })) = msg_wrapped
        {
            if rocket.is_some() {
                log_internal(
                    Channel::Debug,
                    payload!(
                        action : "Planet received an asteroid and defends with a rocket, closing conversation",
                        planet_id : planet_id,
                        conversation_id : self.id
                    ),
                );
                return None;
            }

            log_internal(
                Channel::Debug,
                payload!(
                    action : "Planet received an asteroid and did not defend, so it will be killed",
                    planet_id : planet_id,
                    conversation_id : self.id
                ),
            );

            //Transition to KillStateConversation
            let new_state = KillPlanetConversation::<SendPlanetKill>::new(
                self.id,
                SendPlanetKill::new(
                    self.state.to_planet_struct,
                    self.state.explorers_location_ref,
                    self.state.explorers_senders,
                ),
            );
            return Some(Box::new(new_state));
        }
        //wrong message arrived, transitioning to ErrorState
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
    }

    fn get_priority(&self) -> i32 {
        4
    }

    /// Returns the timeout duration for waiting for `AsteroidAck`.
    /// Asteroids are critical events - if the planet doesn't respond, it's considered destroyed.
    fn get_timeout(&self) -> Option<Duration> {
        Some(ASTEROID_ACK_TIMEOUT)
    }
}

impl AsteroidConversation<WaitingAsteroidAck> {
    fn new(id: ID, state: WaitingAsteroidAck) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::PlanetToOrchKind(
                PlanetToOrchestratorKind::AsteroidAck,
            )),
            state,
        }
    }
}

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

    fn make_empty_explorer_refs() -> (ExplorersLocationRef, SendersToExplorer) {
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
        assert_eq!(conv.get_timeout(), Some(Duration::from_millis(TIMEOUT)));
    }
}
