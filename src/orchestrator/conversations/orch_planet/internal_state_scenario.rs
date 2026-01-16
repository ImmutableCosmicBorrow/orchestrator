use crate::logging_utils::log_internal;
use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError, ToPlanetStruct,
};
use crate::payload;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
///**Internal State Conversation**
///
/// This module manages the conversation between the Orchestrator and a Planet regarding its internal state.
/// It uses a Finite State Machine (FSM) to ensure that requests and responses are handled in the correct
/// order at compile time.
///
/// The conversation flow starts by sending a request and terminates once the planet's state
/// is received (intended for UI reporting).
///
/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingInternalStateRequest`] state, which sends an
/// [`OrchestratorToPlanet::InternalStateRequest`] when the [`Conversation::transition`] method is called.
struct SendingInternalStateRequest {
    /// A struct containing fields to send messages to the indicated planet
    to_planet_struct: ToPlanetStruct,
}

impl SendingInternalStateRequest {
    /// Constructor for [`SendingInternalStateRequest`] state struct
    fn new(to_planet_struct: ToPlanetStruct) -> Self {
        Self { to_planet_struct }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingInternalStateResponse`] state, the conversation expects a
/// [`PlanetToOrchestrator::InternalStateResponse`] message to complete the state retrieval.
struct WaitingInternalStateResponse {
    /// ID of the planet we are waiting for
    planet_id: ID,
}

impl WaitingInternalStateResponse {
    /// The constructor for [`WaitingInternalStateResponse`] state struct
    fn new(planet_id: ID) -> Self {
        Self { planet_id }
    }
}

/// Generic FSM struct for Internal State requests
struct InternalStateConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the conversation
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING INTERNAL STATE REQUEST IMPLEMENTATION
impl Conversation<ExplorerBag> for InternalStateConversation<SendingInternalStateRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_planet_struct.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingInternalStateRequest`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToPlanetFailed`] if the message has not been correctly sent to the planet
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::PlanetSenderNotFound`] if the sender to the planet is not in the [`SendersToPlanet`] list
    ///
    /// The next state: [`InternalStateConversation<WaitingInternalStateResponse>`] if the request was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::InternalStateRequest)
        {
            Ok(()) => {
                let next_state = InternalStateConversation::<WaitingInternalStateResponse>::new(
                    self.id,
                    self.state.to_planet_struct.planet_id,
                );
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
        3
    }
}

impl InternalStateConversation<SendingInternalStateRequest> {
    pub(crate) fn new(id: ID, state: SendingInternalStateRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBag> for InternalStateConversation<WaitingInternalStateResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingInternalStateResponse`] state:
    ///
    /// Returns:
    ///
    /// [None] if the state is successfully received and sent to the UI, closing the conversation
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different from the expected one [`PlanetToOrchestrator::InternalStateResponse`]
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::InternalStateResponse {
            planet_id,
            planet_state,
        })) = msg_wrapped
        {
            //TODO: SEND PLANET STATE TO UI
            log_internal(
                Channel::Debug,
                payload!(
                    action : "Planet sent its internal state",
                    planet_id : planet_id,
                    planet_state : format!("{planet_state:?}"),
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
        3
    }
}

impl InternalStateConversation<WaitingInternalStateResponse> {
    pub(crate) fn new(id: ID, planet_id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::InternalStateResponse,
            )),
            state: WaitingInternalStateResponse::new(planet_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_game::components::planet::DummyPlanetState;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    // IDs for testing
    const CONV_ID: ID = 1;
    const PLANET_ID: ID = 2;

    #[test]
    fn send_success() {
        let (tx, _rx) = unbounded::<OrchestratorToPlanet>();
        let senders_to_planets = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx)])));

        let to_planet = ToPlanetStruct {
            planet_id: PLANET_ID,
            planets_senders: senders_to_planets,
        };
        let state = SendingInternalStateRequest::new(to_planet);
        let conv =
            Box::new(InternalStateConversation::<SendingInternalStateRequest>::new(CONV_ID, state));

        // Act: Transition sends the Request and moves to Waiting state
        let next_conv = conv
            .transition(None)
            .expect("Should transition to Waiting state");

        // Assert correct Expected kind, conversation id and no error_details
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::InternalStateResponse
            ))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn send_missing_sender() {
        //No senders in map
        let senders_to_planets = Arc::new(Mutex::new(HashMap::new()));

        let to_planet = ToPlanetStruct {
            planet_id: PLANET_ID,
            planets_senders: senders_to_planets,
        };
        let state = SendingInternalStateRequest::new(to_planet);
        let conv =
            Box::new(InternalStateConversation::<SendingInternalStateRequest>::new(CONV_ID, state));

        //Transition should lead to an error
        let next_conv = conv.transition(None).expect("Should return ErrorState");

        // Assert correct expected kind and error_details
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("sender to planet {PLANET_ID} not found"))
        );
        //Assert: Error transitions to None, finishing the Conversation
        assert!(next_conv.transition(None).is_none());
    }

    #[test]
    fn send_message_failure() {
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();
        drop(rx); // Break the channel

        let senders_to_planets = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx)])));
        let to_planet = ToPlanetStruct {
            planet_id: PLANET_ID,
            planets_senders: senders_to_planets,
        };

        let state = SendingInternalStateRequest::new(to_planet);
        let conv =
            Box::new(InternalStateConversation::<SendingInternalStateRequest>::new(CONV_ID, state));

        //Transition should lead to an Error
        let next_conv = conv.transition(None).expect("Should return ErrorState");

        // Assert correct error_details
        let error_msg = next_conv
            .get_error_details()
            .expect("Should have error details");
        assert_eq!(
            error_msg,
            format!("failed to send message to planet {PLANET_ID}")
        );
        //Assert: Error transitions to None, finishing the Conversation
        assert!(next_conv.transition(None).is_none());
    }

    #[test]
    fn wait_correct_response() {
        let conv = Box::new(
            InternalStateConversation::<WaitingInternalStateResponse>::new(CONV_ID, PLANET_ID),
        );

        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::InternalStateResponse {
            planet_id: PLANET_ID,
            planet_state: DummyPlanetState {
                energy_cells: vec![],
                charged_cells_count: 0,
                has_rocket: false,
            },
        });

        // Act: Valid message should end the conversation
        let result = conv.transition(Some(msg));

        // Assert: Conversation should finish correctly
        assert!(result.is_none(), "Conversation should finish successfully");
    }

    #[test]
    fn wait_wrong_message() {
        let conv = Box::new(
            InternalStateConversation::<WaitingInternalStateResponse>::new(CONV_ID, PLANET_ID),
        );

        // Sending an unrelated message variant
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StartPlanetAIResult {
            planet_id: PLANET_ID,
        });

        // Transition should lead to an Error state
        let result = conv
            .transition(Some(wrong_msg))
            .expect("Should return ErrorState");

        // Assert correct error_details
        assert_eq!(
            result.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
        //Assert: Error transitions to None, finishing the Conversation
        assert!(result.transition(None).is_none());
    }
}
