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

struct SendingInternalStateRequest {
    to_planet_struct: ToPlanetStruct,
}

impl SendingInternalStateRequest {
    fn new(to_planet_struct: ToPlanetStruct) -> Self {
        Self { to_planet_struct }
    }
}

struct WaitingInternalStateResponse;

struct InternalStateConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
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
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::InternalStateRequest)
        {
            Ok(_) => {
                let next_state =
                    InternalStateConversation::<WaitingInternalStateResponse>::new(self.id);
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

impl InternalStateConversation<SendingInternalStateRequest> {
    pub(crate) fn new(id: ID, state: SendingInternalStateRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
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
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::InternalStateResponse {
            planet_id,
            planet_state,
        })) = msg_wrapped
        {
            //TODO: SEND PLANET STATE TO UI
            println!("Planet {planet_id} sent its internal state {planet_state:?}");
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
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
