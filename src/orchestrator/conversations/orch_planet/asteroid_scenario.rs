use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_planet::kill_planet::{
    KillPlanetConversation, SendPlanetKill,
};
use crate::orchestrator::conversations::{
    Conversation, PossibleExpectedKinds, PossibleMessage, SendersToPlanet,
};
use common_game::components::forge::Forge;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
use std::sync::Arc;

#[cfg(doc)]
use common_game::components::asteroid::Asteroid;

///**Asteroid Conversation**
///
/// This Module deals with the conversation between the Orchestrator and a Planet regarding asteroids, using FSM to ensure the correctness
/// of messages orders at compile time

///Marker Struct for FSM state
///
/// In the [WaitingAsteroidAck] state, the conversation expects a [PlanetToOrchestrator::AsteroidAck] message to decide
/// whether to kill the planet using the [KillPlanetConversation] or closing the conversations if the planet defends himself
struct WaitingAsteroidAck {
    ///ID of planet to send the message
    to_planet_id: ID,
    ///Atomic Reference to planet senders hashmap
    planets_senders: SendersToPlanet,
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
    ) -> Result<
        Option<Box<dyn Conversation<ExplorerBag>>>,
        (Option<Box<dyn Conversation<ExplorerBag>>>, String),
    > {
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::AsteroidAck {
            planet_id,
            rocket,
        })) = msg_wrapped
        {
            if let Some(r) = rocket {
                println!(
                    "Planet {planet_id} received an asteroid and defends with the rocket {r:?}"
                );
                return Ok(None);
            }

            println!("Planet {planet_id} received an asteroid and will be killed");
            //Transition to KillStateConversation
            let new_state = KillPlanetConversation::<SendPlanetKill>::new(
                self.id,
                SendPlanetKill::new(self.state.to_planet_id, self.state.planets_senders.clone()),
            );
            return Ok(Some(Box::new(new_state)));
        }

        Err((
            Some(self),
            "Wrong message arrived, keeping same state".to_string(),
        ))
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

///Marker Struct for FSM state
///
/// The conversation starts in the [SendingAsteroid] state, this state does not expect any message as it sends a [OrchestratorToPlanet::Sunray]
/// when the [Conversation::transition] method is called
struct SendingAsteroid {
    ///ID of planet to send the message
    to_planet_id: ID,
    ///
    planets_senders: SendersToPlanet,
    ///Atomic Reference to the forge to create [Asteroid]
    forge: Arc<Forge>,
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
    ) -> Result<
        Option<Box<dyn Conversation<ExplorerBag>>>,
        (Option<Box<dyn Conversation<ExplorerBag>>>, String),
    > {
        //to release immediately the lock on the hashmap
        let sender = {
            let lock = self.state.planets_senders.lock().unwrap();
            lock.get(&self.state.to_planet_id).cloned() // Clone the Sender handle
        };

        if let Some(s) = sender {
            let asteroid = self.state.forge.as_ref().generate_asteroid();
            match s.send(OrchestratorToPlanet::Asteroid(asteroid)) {
                Ok(_) => {
                    let next_state = AsteroidConversation::<WaitingAsteroidAck>::new(
                        self.id,
                        WaitingAsteroidAck {
                            to_planet_id: self.state.to_planet_id,
                            planets_senders: self.state.planets_senders.clone(),
                        },
                    );
                    Ok(Some(Box::new(next_state)))
                }
                Err(_) => Err((Some(self), "Channel Disconnected".to_string())),
            }
        } else {
            Err((Some(self), "Sender not Found!".to_string()))
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
