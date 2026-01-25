use crate::logging_utils::log_internal;
use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError, ToPlanetStruct,
};
use crate::payload;
use common_game::components::forge::Forge;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::SunrayAck;
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::utils::ID;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default timeout duration for waiting for a Sunray acknowledgment.
/// The planet should respond quickly to sunray events.
const SUNRAY_ACK_TIMEOUT: Duration = Duration::from_secs(5);

///**Sunray Conversation**
///
/// This module manages the conversation between the Orchestrator and a Planet regarding Sunrays.
/// It uses a Finite State Machine (FSM) to ensure that the Sunray is sent and acknowledged
/// in the correct order at compile time.
///
/// The conversation starts by generating and sending a Sunray, then waits for a confirmation
/// from the target planet.
/// Marker struct for FSM state
///
/// In the [`WaitingSunrayAck`] state, the conversation expects a [`PlanetToOrchestrator::SunrayAck`]
/// message from the planet to confirm receipt of the Sunray.
struct WaitingSunrayAck {
    /// ID of the planet we are sending the sunray to
    planet_id: ID,
    /// Instant when we started waiting for the acknowledgment (for timeout tracking)
    wait_start: Instant,
}

impl WaitingSunrayAck {
    /// The constructor for [`WaitingSunrayAck`] state struct
    fn new(planet_id: ID) -> Self {
        Self {
            planet_id,
            wait_start: Instant::now(),
        }
    }
}

/// Marker struct for FSM state
///
/// The conversation starts in the [`SendSunray`] state, which generates a Sunray via the [`Forge`]
/// and sends an [`OrchestratorToPlanet::Sunray`] when the [`Conversation::transition`] method is called.
pub(crate) struct SendSunray {
    /// A struct containing fields to send messages to the indicated planet
    to_planet_struct: ToPlanetStruct,
    /// Atomic Reference to the forge used to generate the Sunray
    forge_ref: Arc<Forge>,
}

impl SendSunray {
    /// Constructor for [`SendSunray`] state struct
    pub(crate) fn new(to_planet_struct: ToPlanetStruct, forge_ref: Arc<Forge>) -> Self {
        Self {
            to_planet_struct,
            forge_ref,
        }
    }
}

/// Sunray Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
pub(crate) struct SunrayConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the conversation
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SEND SUNRAY IMPLEMENTATION
impl Conversation<ExplorerBag> for SunrayConversation<SendSunray> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (Some(self.state.to_planet_struct.planet_id), None)
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendSunray`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToPlanetFailed`] if the message has not been correctly sent to the planet
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::PlanetSenderNotFound`] if the sender to the planet is not in the list
    ///
    /// The next state: [`SunrayConversation<WaitingSunrayAck>`] if the sunray was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        let sunray = self.state.forge_ref.generate_sunray();
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::Sunray(sunray))
        {
            Ok(()) => {
                let planet_id = self.state.to_planet_struct.planet_id;
                let next_state = SunrayConversation::<WaitingSunrayAck>::new(self.id, planet_id);
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
                Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBag> + Send + Sync>)
            }
        }
    }

    fn get_priority(&self) -> i32 {
        1
    }
}

impl SunrayConversation<SendSunray> {
    /// The constructor for [`SunrayConversation`] in the [`SendSunray`] state
    pub(crate) fn new(id: ID, state: SendSunray) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING SUNRAY ACK IMPLEMENTATION
impl Conversation<ExplorerBag> for SunrayConversation<WaitingSunrayAck> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (Some(self.state.planet_id), None)
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingSunrayAck`] state:
    ///
    /// Returns:
    ///
    /// [None] if the [`PlanetToOrchestrator::SunrayAck`] is successfully received, ending the conversation.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different from the expected one.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::SunrayAck { planet_id })) =
            msg_wrapped
        {
            log_internal(
                Channel::Debug,
                payload!(
                    action : "Planet received the Sunray, closing conversation",
                    planet_id : planet_id,
                    conversation_id : self.id
                ),
            );
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBag> + Send + Sync>)
    }

    fn get_priority(&self) -> i32 {
        1
    }

    /// Returns when this conversation started waiting for the `SunrayAck` message.
    fn get_wait_start(&self) -> Option<Instant> {
        Some(self.state.wait_start)
    }

    /// Returns the timeout duration for waiting for `SunrayAck`.
    /// After this duration, `on_timeout` will be called.
    fn get_timeout(&self) -> Option<Duration> {
        Some(SUNRAY_ACK_TIMEOUT)
    }

    /// Called when the conversation times out waiting for `SunrayAck`.
    /// Logs a warning - the conversation is simply terminated.
    fn on_timeout(self: Box<Self>) {
        log_internal(
            Channel::Warning,
            payload!(
                action : "Sunray conversation timed out waiting for planet acknowledgment",
                planet_id : self.state.planet_id,
                conversation_id : self.id,
                timeout_secs : SUNRAY_ACK_TIMEOUT.as_secs()
            ),
        );
        // Conversation ends here - no further action needed for sunray timeout
    }
}

impl SunrayConversation<WaitingSunrayAck> {
    /// The constructor for [`SunrayConversation`] in the [`WaitingSunrayAck`] state
    fn new(id: ID, planet_id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(SunrayAck)),
            state: WaitingSunrayAck::new(planet_id),
        }
    }
}

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

    #[allow(clippy::unnecessary_box_returns)]
    fn make_sunray_conversation_send(
        forge_ref: Arc<Forge>,
        senders: Arc<Mutex<HashMap<ID, crossbeam_channel::Sender<OrchestratorToPlanet>>>>,
    ) -> Box<SunrayConversation<SendSunray>> {
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let state = SendSunray::new(to_planet, forge_ref);
        Box::new(SunrayConversation::<SendSunray>::new(CONV_ID, state))
    }

    #[allow(clippy::unnecessary_box_returns)]
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

        // Verify wait_start is set
        assert!(conv.get_wait_start().is_some());
    }

    #[test]
    fn waiting_sunray_timeout_logs_and_terminates() {
        let conv = make_sunray_conversation_wait();

        // on_timeout should just log and return (not panic)
        // This test verifies it doesn't panic
        conv.on_timeout();
        // If we get here, the test passes - on_timeout completed without panic
    }

    #[test]
    fn waiting_sunray_wait_start_is_recent() {
        use std::time::Instant;

        let before = Instant::now();
        let conv = make_sunray_conversation_wait();
        let after = Instant::now();

        let wait_start = conv.get_wait_start().expect("wait_start should be set");

        // Verify that wait_start is between before and after creation
        assert!(wait_start >= before && wait_start <= after);
    }
}
