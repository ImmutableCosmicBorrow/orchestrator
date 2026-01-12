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

///**Stop Planet Conversation**
///
/// This module manages the conversation between the Orchestrator and a Planet regarding the stopping of its AI.
/// It uses a Finite State Machine (FSM) to ensure that requests and responses are handled in the correct
/// order at compile time.
///
/// The conversation flow starts by sending a stop request and terminates once the planet
/// confirms the AI has successfully stopped.

/// Marker struct for FSM state
///
/// In the [`WaitingPlanetStopResult`] state, the conversation expects a
/// [`PlanetToOrchestrator::StopPlanetAIResult`] message to confirm the planet has successfully halted its AI processes.
struct WaitingPlanetStopResult {
    /// ID of the planet we are stopping
    planet_id: ID,
}

impl WaitingPlanetStopResult {
    /// The constructor for [`WaitingPlanetStopResult`] state struct
    fn new(planet_id: ID) -> Self {
        Self { planet_id }
    }
}

/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingPlanetStop`] state, which sends an
/// [`OrchestratorToPlanet::StopPlanetAI`] when the [`Conversation::transition`] method is called.
struct SendingPlanetStop {
    /// A struct containing fields to send messages to the indicated planet
    to_planet_struct: ToPlanetStruct,
}

impl SendingPlanetStop {
    /// Constructor for [`SendingPlanetStop`] state struct
    fn new(to_planet_struct: ToPlanetStruct) -> Self {
        Self { to_planet_struct }
    }
}

/// Stop Planet Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
struct StopPlanetConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the conversation
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING PLANET STOP IMPLEMENTATION
impl Conversation<ExplorerBag> for StopPlanetConversation<SendingPlanetStop> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_planet_struct.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingPlanetStop`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToPlanetFailed`] if the message has not been correctly sent to the planet
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::PlanetSenderNotFound`] if the sender to the planet is not in the list
    ///
    /// The next state: [`StopPlanetConversation<WaitingPlanetStopResult>`] if the stop command was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::StopPlanetAI)
        {
            Ok(()) => {
                let planet_id = self.state.to_planet_struct.planet_id;
                let next_state =
                    StopPlanetConversation::<WaitingPlanetStopResult>::new(self.id, planet_id);
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

impl StopPlanetConversation<SendingPlanetStop> {
    /// The constructor for [`StopPlanetConversation`] in the [`SendingPlanetStop`] state
    pub(crate) fn new(id: ID, state: SendingPlanetStop) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING RESULT IMPLEMENTATION
impl Conversation<ExplorerBag> for StopPlanetConversation<WaitingPlanetStopResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingPlanetStopResult`] state:
    ///
    /// Returns:
    ///
    /// [None] if the stop result is successfully received and processed, closing the conversation.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different from the expected one [`PlanetToOrchestrator::StopPlanetAIResult`]
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StopPlanetAIResult {
                                                      planet_id,
                                                  })) = msg_wrapped
        {
            println!("Stopped Planet: {planet_id:?}");
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

impl StopPlanetConversation<WaitingPlanetStopResult> {
    /// The constructor for [`StopPlanetConversation`] in the [`WaitingPlanetStopResult`] state
    fn new(id: ID, planet_id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::StopPlanetAIResult,
            )),
            state: WaitingPlanetStopResult::new(planet_id),
        }
    }
}