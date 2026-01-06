use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError, ToPlanetStruct,
};
use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::KillPlanetResult;
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::utils::ID;

//TODO: ADD CHECK IF EXPLORER IS IN PLANET AND KILL HIM

struct WaitingPlanetKillResult;
pub(crate) struct SendPlanetKill {
    to_planet_struct: ToPlanetStruct,
}

impl SendPlanetKill {
    pub(crate) fn new(to_planet_struct: ToPlanetStruct) -> Self {
        Self { to_planet_struct }
    }
}

pub(crate) struct KillPlanetConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
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
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::KillPlanet)
        {
            Ok(_) => {
                let next_state = KillPlanetConversation::<WaitingPlanetKillResult>::new(self.id);
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

impl KillPlanetConversation<SendPlanetKill> {
    pub(crate) fn new(id: ID, state: SendPlanetKill) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
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
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::KillPlanetResult {
            planet_id,
        })) = msg_wrapped
        {
            println!("Killed Planet: {:?}", planet_id);
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
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
