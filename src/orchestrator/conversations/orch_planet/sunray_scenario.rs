use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    Conversation, PossibleExpectedKinds, PossibleMessage, SendersToPlanet,
};
use common_game::components::forge::Forge;
use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::{
    StartPlanetAIResult, SunrayAck,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
use std::marker::PhantomData;
use std::sync::Arc;

struct WaitingSunrayAck;
struct SendSunray {
    to_planet_id: ID,
    planets_senders: SendersToPlanet,
    forge: Arc<Forge>,
}

struct SunrayConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for SunrayConversation<WaitingSunrayAck> {
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
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::SunrayAck { planet_id })) =
            msg_wrapped
        {
            println!("Planet {:?} received the sunray", planet_id);
            return Ok(None);
        }

        Err((
            Some(self),
            "Wrong message arrived, keeping same state".to_string(),
        ))
    }
}

impl SunrayConversation<WaitingSunrayAck> {
    pub(crate) fn new(id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(SunrayAck)),
            state: WaitingSunrayAck,
        }
    }
}

impl Conversation<ExplorerBag> for SunrayConversation<SendSunray> {
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
            let sunray = self.state.forge.as_ref().generate_sunray();
            match s.send(OrchestratorToPlanet::Sunray(sunray)) {
                Ok(_) => {
                    let next_state = SunrayConversation::<WaitingSunrayAck>::new(self.id);
                    Ok(Some(Box::new(next_state)))
                }
                Err(_) => Err((Some(self), "Channel Disconnected".to_string())),
            }
        } else {
            Err((Some(self), "Sender not Found!".to_string()))
        }
    }
}

impl SunrayConversation<SendSunray> {
    pub(crate) fn new(id: ID, state: SendSunray) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}
