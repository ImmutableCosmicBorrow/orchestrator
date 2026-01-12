use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError, ToPlanetStruct,
};
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
            println!("Started Planet: {planet_id:?}");
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
