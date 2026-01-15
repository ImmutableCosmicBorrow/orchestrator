use crate::logging_utils::log_internal;
use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError, ToPlanetStruct,
};
use crate::payload;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::StartPlanetAIResult;
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::utils::ID;

///**Start Planet Conversation**
///
/// This module manages the conversation between the Orchestrator and a Planet regarding the activation of its AI.
/// It uses a Finite State Machine (FSM) to ensure that the start command and the confirmation result
/// are handled in the correct order at compile time.
///
/// The conversation flow starts by sending a start request and terminates once the planet
/// confirms the AI has started.
/// Marker struct for FSM state
///
/// In the [`WaitingPlanetStartResult`] state, the conversation expects a
/// [`PlanetToOrchestrator::StartPlanetAIResult`] message to confirm the planet has successfully initialized its AI.
struct WaitingPlanetStartResult {
    /// ID of the planet we are starting
    planet_id: ID,
}

impl WaitingPlanetStartResult {
    /// The constructor for [`WaitingPlanetStartResult`] state struct
    fn new(planet_id: ID) -> Self {
        Self { planet_id }
    }
}

/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingPlanetStart`] state, which sends an
/// [`OrchestratorToPlanet::StartPlanetAI`] when the [`Conversation::transition`] method is called.
struct SendingPlanetStart {
    /// A struct containing fields to send messages to the indicated planet
    to_planet_struct: ToPlanetStruct,
}

impl SendingPlanetStart {
    /// Constructor for [`SendingPlanetStart`] state struct
    fn new(to_planet_struct: ToPlanetStruct) -> Self {
        Self { to_planet_struct }
    }
}

/// Start Planet Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
struct StartPlanetConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the conversation
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING PLANET START IMPLEMENTATION
impl Conversation<ExplorerBag> for StartPlanetConversation<SendingPlanetStart> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_planet_struct.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingPlanetStart`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToPlanetFailed`] if the message has not been correctly sent to the planet
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::PlanetSenderNotFound`] if the sender to the planet is not in the list
    ///
    /// The next state: [`StartPlanetConversation<WaitingPlanetStartResult>`] if the start command was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::StartPlanetAI)
        {
            Ok(()) => {
                let planet_id = self.state.to_planet_struct.planet_id;
                let next_state =
                    StartPlanetConversation::<WaitingPlanetStartResult>::new(self.id, planet_id);
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
        5
    }
}

impl StartPlanetConversation<SendingPlanetStart> {
    pub(crate) fn new(id: ID, state: SendingPlanetStart) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING RESULT IMPLEMENTATION
impl Conversation<ExplorerBag> for StartPlanetConversation<WaitingPlanetStartResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingPlanetStartResult`] state:
    ///
    /// Returns:
    ///
    /// [None] if the start result is successfully received, ending the conversation.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different from [`PlanetToOrchestrator::StartPlanetAIResult`]
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StartPlanetAIResult {
            planet_id,
        })) = msg_wrapped
        {
            log_internal(
                Channel::Info,
                payload!(
                    action : "Started Planet, closing conversation",
                    planet_id : planet_id,
                    conversation_id : self.id,
                ),
            );
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        5
    }
}

impl StartPlanetConversation<WaitingPlanetStartResult> {
    fn new(id: ID, planet_id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(StartPlanetAIResult)),
            state: WaitingPlanetStartResult::new(planet_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: u32 = 1;
    const PLANET_ID: u32 = 2;
    #[test]
    fn test_sending_state_correct() {
        let (tx, _rx) = unbounded::<OrchestratorToPlanet>();
        //adding the correct channel
        let senders_to_planets = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx.clone())])));

        // We wrap this in the SendingPlanetStart state
        // Note: You'll need to satisfy the ToPlanetStruct requirements
        let to_planet = ToPlanetStruct {
            planet_id: PLANET_ID,
            planets_senders: senders_to_planets,
        };
        let state = SendingPlanetStart::new(to_planet);
        let conv = Box::new(StartPlanetConversation::<SendingPlanetStart>::new(
            CONV_ID, state,
        ));

        // Act: Transition from the first state
        // The first transition sends the message and moves to 'Waiting'
        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");

        // Assert: Check if the expected message kind is now set correctly
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PlanetToOrchKind(StartPlanetAIResult))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
    }

    #[test]
    fn test_sending_state_wrong_planet_sender() {
        //adding the correct channel
        let senders_to_planets = Arc::new(Mutex::new(HashMap::new()));

        // We wrap this in the SendingPlanetStart state
        // Note: You'll need to satisfy the ToPlanetStruct requirements
        let to_planet = ToPlanetStruct {
            planet_id: PLANET_ID,
            planets_senders: senders_to_planets,
        };
        let state = SendingPlanetStart::new(to_planet);
        let conv = Box::new(StartPlanetConversation::<SendingPlanetStart>::new(
            CONV_ID, state,
        ));

        // Act: Transition from the first state
        // The first transition sends the message and moves to 'Waiting'
        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");

        // Assert: Error state no expected message
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(next_conv.get_id(), CONV_ID);
        //ASSERT: IT IS THE RIGHT ERROR
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("sender to planet {PLANET_ID} not found"))
        );
    }

    #[test]
    fn test_sending_state_message_failure() {
        // 1. Create the channel
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();

        // 2. Drop the receiver immediately
        // This makes the channel "disconnected". Any future send attempts will fail.
        drop(rx);

        let senders_to_planets = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx)])));

        let to_planet = ToPlanetStruct {
            planet_id: PLANET_ID,
            planets_senders: senders_to_planets,
        };

        let state = SendingPlanetStart::new(to_planet);
        let conv = Box::new(StartPlanetConversation::<SendingPlanetStart>::new(
            CONV_ID, state,
        ));

        // 3. Act: This should now trigger the Err(ToPlanetError::SendingMessageFailure) branch
        let next_conv = conv.transition(None).expect("Should return an ErrorState");

        // 4. Assert: Check for the specific MessageToPlanetFailed error
        // Note: The string here depends on how your CommonErrorTypes handles the "MessageToPlanetFailed" variant
        let error_msg = next_conv
            .get_error_details()
            .expect("Should return an Error Details String");
        assert_eq!(
            error_msg,
            format!("failed to send message to planet {PLANET_ID}")
        );
    }

    #[test]
    fn test_waiting_state_correct_transition() {
        // Start directly in the Waiting state to test the end of the FSM
        let conv = Box::new(StartPlanetConversation::<WaitingPlanetStartResult>::new(
            CONV_ID, PLANET_ID,
        ));

        // Mock the incoming message from the planet
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StartPlanetAIResult {
            planet_id: PLANET_ID,
        });

        // Act: Transition with the correct message
        let result = conv.transition(Some(msg));

        // Assert: Successful completion should return None
        assert!(
            result.is_none(),
            "Conversation should terminate successfully (None)"
        );
    }

    #[test]
    fn test_waiting_state_wrong_message() {
        let conv = Box::new(StartPlanetConversation::<WaitingPlanetStartResult>::new(
            CONV_ID, PLANET_ID,
        ));

        // Mock an irrelevant message (e.g., a different planet message if available)
        // Using a dummy/wrong message variant here
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::SunrayAck {
            planet_id: PLANET_ID,
        });

        // Act
        let result = conv
            .transition(Some(wrong_msg))
            .expect("Should return an ErrorState");

        // Assert: We expect an ErrorState, not None
        assert_eq!(result.get_id(), CONV_ID);
        // Assert: It is the right kind of error
        assert_eq!(
            result.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }
}
