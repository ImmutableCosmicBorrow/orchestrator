use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    Conversation, PossibleExpectedKinds, PossibleMessage, SendersToPlanet,
};
use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::StartPlanetAIResult;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
use std::marker::PhantomData;

struct WaitingPlanetStartResult;
struct SendingPlanetStart {
    to_planet_id: ID,
    planets_senders: SendersToPlanet,
}
///Start Planet Conversation FSM
struct StartPlanetConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for StartPlanetConversation<WaitingPlanetStartResult> {
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
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StartPlanetAIResult {
            planet_id,
        })) = msg_wrapped
        {
            println!("Stopped Planet: {:?}", planet_id);
            return Ok(None);
        }

        Err((
            Some(self),
            "Wrong message arrived, keeping same state".to_string(),
        ))
    }
}

impl StartPlanetConversation<WaitingPlanetStartResult> {
    pub(crate) fn new(id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(StartPlanetAIResult)),
            state: WaitingPlanetStartResult,
        }
    }
}

impl Conversation<ExplorerBag> for StartPlanetConversation<SendingPlanetStart> {
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
            match s.send(OrchestratorToPlanet::StartPlanetAI) {
                Ok(_) => {
                    let next_state =
                        StartPlanetConversation::<WaitingPlanetStartResult>::new(self.id);
                    Ok(Some(Box::new(next_state)))
                }
                Err(_) => Err((Some(self), "Channel Disconnected".to_string())),
            }
        } else {
            Err((Some(self), "Sender not Found!".to_string()))
        }
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
