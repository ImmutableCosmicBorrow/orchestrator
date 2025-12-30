use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    Conversation, PossibleExpectedKinds, PossibleMessage, SendersToPlanet,
};
use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::{
    KillPlanetResult, StartPlanetAIResult,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
use std::marker::PhantomData;

struct WaitingPlanetKillResult;
pub(crate) struct SendPlanetKill {
    to_planet_id: ID,
    planets_senders: SendersToPlanet,
}

impl SendPlanetKill {
    pub(crate) fn new(to_planet_id: ID, planets_senders: SendersToPlanet) -> Self {
        Self {
            to_planet_id,
            planets_senders,
        }
    }
}

pub(crate) struct KillPlanetConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for KillPlanetConversation<WaitingPlanetKillResult> {
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
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::KillPlanetResult {
            planet_id,
        })) = msg_wrapped
        {
            println!("Killed Planet: {:?}", planet_id);
            return Ok(None);
        }

        Err((
            Some(self),
            "Wrong message arrived, keeping same state".to_string(),
        ))
    }
}

impl KillPlanetConversation<WaitingPlanetKillResult> {
    pub(crate) fn new(id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(KillPlanetResult)),
            state: WaitingPlanetKillResult,
        }
    }
}

impl Conversation<ExplorerBag> for KillPlanetConversation<SendPlanetKill> {
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
            match s.send(OrchestratorToPlanet::KillPlanet) {
                Ok(_) => {
                    let next_state =
                        KillPlanetConversation::<WaitingPlanetKillResult>::new(self.id);
                    Ok(Some(Box::new(next_state)))
                }
                Err(_) => Err((Some(self), "Channel Disconnected".to_string())),
            }
        } else {
            Err((Some(self), "Sender not Found!".to_string()))
        }
    }
}

impl KillPlanetConversation<SendPlanetKill> {
    pub(crate) fn new(id: ID, state: SendPlanetKill) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}
