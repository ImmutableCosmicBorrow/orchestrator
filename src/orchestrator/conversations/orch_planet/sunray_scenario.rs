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
}

impl WaitingSunrayAck {
    /// The constructor for [`WaitingSunrayAck`] state struct
    fn new(planet_id: ID) -> Self {
        Self { planet_id }
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

    fn get_entity_id(&self) -> ID {
        self.state.to_planet_struct.planet_id
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
                Some(Box::new(error_state))
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

    fn get_entity_id(&self) -> ID {
        self.state.planet_id
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
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        1
    }
}

impl SunrayConversation<WaitingSunrayAck> {
    /// The constructor for [`SunrayConversation`] in the [`WaitingSunrayAck`] state
    pub(crate) fn new(id: ID, planet_id: ID) -> Self {
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
    use crate::orchestrator;
    use common_game::components::forge::Forge;
    use common_game::logging::ActorType::Orchestrator;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};

    const CONV_ID: ID = 1;
    const PLANET_ID: ID = 2;

    fn get_forge_for_testing() -> Arc<Forge> {
        static TEST_FORGE: OnceLock<Arc<Forge>> = OnceLock::new();
        TEST_FORGE
            .get_or_init(|| {
                // This block only runs the very first time any test calls this function
                Arc::new(Forge::new().expect("Forge singleton failed to initialize"))
            })
            .clone() // Returns a new pointer to the same instance
    }
    #[test]
    fn test_sending_sunray_state_correct() {
        let (tx, _rx) = unbounded::<OrchestratorToPlanet>();
        let senders_to_planets = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx.clone())])));

        // We need a forge for this state
        let forge_ref = get_forge_for_testing();

        let to_planet = ToPlanetStruct {
            planet_id: PLANET_ID,
            planets_senders: senders_to_planets,
        };
        let state = SendSunray::new(to_planet, forge_ref);
        let conv = Box::new(SunrayConversation::<SendSunray>::new(CONV_ID, state));

        // Act: Transition from the first state
        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");

        // Assert: Check if the expected message kind is now SunrayAck
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PlanetToOrchKind(SunrayAck))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn test_sending_sunray_state_wrong_planet_sender() {
        //Void senders_to_planet
        let senders_to_planets = Arc::new(Mutex::new(HashMap::new()));
        let forge_ref = get_forge_for_testing();

        let to_planet = ToPlanetStruct {
            planet_id: PLANET_ID,
            planets_senders: senders_to_planets,
        };
        let state = SendSunray::new(to_planet, forge_ref);
        let conv = Box::new(SunrayConversation::<SendSunray>::new(CONV_ID, state));

        // Transition should lead to an error
        let next_conv = conv
            .transition(None)
            .expect("Should transition to error state");

        // Assert correct Error Type
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("sender to planet {PLANET_ID} not found"))
        );
    }

    #[test]
    fn test_sending_sunray_state_message_failure() {
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();
        // Drop the receiver to force a SendError
        drop(rx);

        let senders_to_planets = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx)])));
        let forge_ref = get_forge_for_testing();

        let to_planet = ToPlanetStruct {
            planet_id: PLANET_ID,
            planets_senders: senders_to_planets,
        };

        let state = SendSunray::new(to_planet, forge_ref);
        let conv = Box::new(SunrayConversation::<SendSunray>::new(CONV_ID, state));

        // Transition should lead to an error
        let next_conv = conv.transition(None).expect("Should return an ErrorState");

        // Assert correct Error
        let error_msg = next_conv
            .get_error_details()
            .expect("Should return an Error Details String");
        assert_eq!(
            error_msg,
            format!("failed to send message to planet {PLANET_ID}")
        );
    }

    #[test]
    fn test_waiting_sunray_ack_correct_transition() {
        // Start directly in the Waiting state
        let conv = Box::new(SunrayConversation::<WaitingSunrayAck>::new(
            CONV_ID, PLANET_ID,
        ));

        // Mock the incoming SunrayAck
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::SunrayAck {
            planet_id: PLANET_ID,
        });

        // Transition should lead to None, concluding the conversation
        let result = conv.transition(Some(msg));

        // Assert: Successful completion
        assert!(result.is_none());
    }

    #[test]
    fn test_waiting_sunray_ack_wrong_message() {
        let conv = Box::new(SunrayConversation::<WaitingSunrayAck>::new(
            CONV_ID, PLANET_ID,
        ));

        // Mock a message that isn't the Ack (e.g., StartPlanetAIResult)
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StartPlanetAIResult {
            planet_id: PLANET_ID,
        });

        // Transition should lead to an error
        let result = conv
            .transition(Some(wrong_msg))
            .expect("Should return an ErrorState");

        // Assert correct error
        assert_eq!(result.get_id(), CONV_ID);
        assert_eq!(
            result.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }
}
