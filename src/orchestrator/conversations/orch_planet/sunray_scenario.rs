use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError, ToPlanetStruct,
};
use common_game::components::forge::Forge;
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
struct SendSunray {
    /// A struct containing fields to send messages to the indicated planet
    to_planet_struct: ToPlanetStruct,
    /// Atomic Reference to the forge used to generate the Sunray
    forge_ref: Arc<Forge>,
}

impl SendSunray {
    /// Constructor for [`SendSunray`] state struct
    fn new(to_planet_struct: ToPlanetStruct, forge_ref: Arc<Forge>) -> Self {
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
struct SunrayConversation<State> {
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
            println!("Planet {planet_id:?} received the sunray");
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