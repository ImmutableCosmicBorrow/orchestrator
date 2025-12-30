use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    Conversation, PossibleExpectedKinds, PossibleMessage, SendersToPlanet,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;

struct WaitingInternalStateResponse;

struct InternalStateConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for InternalStateConversation<WaitingInternalStateResponse> {
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
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::InternalStateResponse {
            planet_id,
            planet_state,
        })) = msg_wrapped
        {
            println!("Planet {planet_id} sent its internal state {planet_state:?}");
            return Ok(None);
        }

        Err((
            Some(self),
            "Wrong message arrived, keeping same state".to_string(),
        ))
    }
}

impl InternalStateConversation<WaitingInternalStateResponse> {
    pub(crate) fn new(id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::InternalStateResponse,
            )),
            state: WaitingInternalStateResponse,
        }
    }
}

struct SendingInternalStateRequest {
    to_planet_id: ID,
    planets_senders: SendersToPlanet,
}

impl Conversation<ExplorerBag> for InternalStateConversation<SendingInternalStateRequest> {
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
            match s.send(OrchestratorToPlanet::InternalStateRequest) {
                Ok(_) => {
                    let next_state =
                        InternalStateConversation::<WaitingInternalStateResponse>::new(self.id);
                    Ok(Some(Box::new(next_state)))
                }
                Err(_) => Err((Some(self), "Channel Disconnected".to_string())),
            }
        } else {
            Err((Some(self), "Sender not Found!".to_string()))
        }
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
