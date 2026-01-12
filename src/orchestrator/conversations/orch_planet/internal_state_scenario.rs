use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError, ToPlanetStruct,
};
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
            println!("Planet {planet_id} sent its internal state {planet_state:?}");
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