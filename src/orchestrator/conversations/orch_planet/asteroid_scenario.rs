use crate::orchestrator::conversations::orch_planet::kill_planet::{
    KillPlanetConversation, SendPlanetKill,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    SendersToExplorer, ToPlanetError, ToPlanetStruct,
};
use crate::orchestrator::{ExplorerBag, ExplorersLocationRef};
#[cfg(doc)]
use common_game::components::asteroid::Asteroid;
use common_game::components::forge::Forge;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
use std::sync::Arc;

///**Asteroid Conversation**
///
/// This Module deals with the conversation between the Orchestrator and a Planet regarding asteroids, using FSM to ensure the correctness
/// of messages orders at compile time

///Marker Struct for FSM state
///
/// The conversation starts in the [SendingAsteroid] state, this state does not expect any message as it sends a [OrchestratorToPlanet::Sunray]
/// when the [Conversation::transition] method is called
struct SendingAsteroid {
    to_planet_struct: ToPlanetStruct,
    ///Atomic Reference to the forge to create [Asteroid]
    forge: Arc<Forge>,
    explorers_senders: SendersToExplorer,
    explorers_location_ref: ExplorersLocationRef,
}

impl SendingAsteroid {
    fn new(
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
/// In the [WaitingAsteroidAck] state, the conversation expects a [PlanetToOrchestrator::AsteroidAck] message to decide
/// whether to kill the planet using the [KillPlanetConversation] or closing the conversations if the planet defends himself
struct WaitingAsteroidAck {
    to_planet_struct: ToPlanetStruct,
    explorers_senders: SendersToExplorer,
    explorers_location_ref: ExplorersLocationRef,
}

impl WaitingAsteroidAck {
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

///Actual FSM struct, takes [State] to get the FSM state
struct AsteroidConversation<State> {
    ///Conversation ID
    id: ID,
    ///Optional expected message of the conversation
    expected_message: Option<PossibleExpectedKinds>,
    ///State of the FSM
    state: State,
}

impl Conversation<ExplorerBag> for AsteroidConversation<SendingAsteroid> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        let asteroid = self.state.forge.generate_asteroid();
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::Asteroid(asteroid))
        {
            Ok(_) => {
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
                Some(Box::new(error_state))
            }
        }
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

impl Conversation<ExplorerBag> for AsteroidConversation<WaitingAsteroidAck> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::AsteroidAck {
            planet_id,
            rocket,
        })) = msg_wrapped
        {
            if let Some(r) = rocket {
                println!(
                    "Planet {planet_id} received an asteroid and defends with the rocket {r:?}"
                );
                return None;
            }

            println!("Planet {planet_id} received an asteroid and will be killed");
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

        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
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
