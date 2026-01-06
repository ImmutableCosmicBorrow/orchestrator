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
use std::fmt::Debug;

struct WaitingPlanetStopResult;
struct SendingPlanetStop {
    to_planet_struct: ToPlanetStruct,
}

impl SendingPlanetStop {
    fn new(to_planet_struct: ToPlanetStruct) -> Self {
        Self { to_planet_struct }
    }
}
///Stop Planet Conversation FSM
struct StopPlanetConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for StopPlanetConversation<SendingPlanetStop> {
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
            .to_planet(OrchestratorToPlanet::StopPlanetAI)
        {
            Ok(_) => {
                let next_state = StopPlanetConversation::<WaitingPlanetStopResult>::new(self.id);
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

impl StopPlanetConversation<SendingPlanetStop> {
    pub(crate) fn new(id: ID, state: SendingPlanetStop) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

impl Conversation<ExplorerBag> for StopPlanetConversation<WaitingPlanetStopResult> {
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
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StopPlanetAIResult {
            planet_id,
        })) = msg_wrapped
        {
            println!("Stopped Planet: {:?}", planet_id);
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }
}

impl StopPlanetConversation<WaitingPlanetStopResult> {
    pub(crate) fn new(id: ID) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::StopPlanetAIResult,
            )),
            state: WaitingPlanetStopResult,
        }
    }
}
