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

struct WaitingSunrayAck;
struct SendSunray {
    to_planet_struct: ToPlanetStruct,
    forge_ref: Arc<Forge>,
}

impl SendSunray {
    fn new(to_planet_struct: ToPlanetStruct, forge_ref: Arc<Forge>) -> Self {
        Self {
            to_planet_struct,
            forge_ref,
        }
    }
}

struct SunrayConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
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
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        let sunray = self.state.forge_ref.generate_sunray();
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::Sunray(sunray))
        {
            Ok(_) => {
                let next_state = SunrayConversation::<WaitingSunrayAck>::new(self.id);
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

impl SunrayConversation<SendSunray> {
    pub(crate) fn new(id: ID, state: SendSunray) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
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
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::SunrayAck { planet_id })) =
            msg_wrapped
        {
            println!("Planet {:?} received the sunray", planet_id);
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
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
