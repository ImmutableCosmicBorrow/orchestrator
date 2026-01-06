use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::orch_explorer::kill_explorers_manager::KillExplorersManager;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    SendersToExplorer, SendersToPlanet, ToPlanetError, ToPlanetStruct,
};
use crate::orchestrator::{ExplorerBag, ExplorersLocationRef};
use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::KillPlanetResult;
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::utils::ID;

struct WaitingPlanetKillResult {
    explorers_location_ref: ExplorersLocationRef,
    explorers_senders: SendersToExplorer,
    planet_senders: SendersToPlanet,
}

impl WaitingPlanetKillResult {
    fn new(
        explorers_location_ref: ExplorersLocationRef,
        explorers_senders: SendersToExplorer,
        planet_senders: SendersToPlanet,
    ) -> Self {
        Self {
            explorers_location_ref,
            explorers_senders,
            planet_senders,
        }
    }
}
pub(crate) struct SendPlanetKill {
    to_planet_struct: ToPlanetStruct,
    explorers_senders: SendersToExplorer,
    explorers_location_ref: ExplorersLocationRef,
}

impl SendPlanetKill {
    pub(crate) fn new(
        to_planet_struct: ToPlanetStruct,
        explorers_location_ref: ExplorersLocationRef,
        explorers_senders: SendersToExplorer,
    ) -> Self {
        Self {
            to_planet_struct,
            explorers_location_ref,
            explorers_senders,
        }
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
                let state_struct = WaitingPlanetKillResult::new(
                    self.state.explorers_location_ref,
                    self.state.explorers_senders,
                    self.state.to_planet_struct.planets_senders,
                );
                let next_state =
                    KillPlanetConversation::<WaitingPlanetKillResult>::new(self.id, state_struct);
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
            let to_kill_list = self.get_explorers_in_planet(&planet_id);
            let next_state = KillExplorersManager::new(
                self.id,
                self.state.explorers_senders,
                self.state.planet_senders,
                false,
                to_kill_list,
            );
            return Some(Box::new(next_state));
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }
}

impl KillPlanetConversation<WaitingPlanetKillResult> {
    pub(crate) fn new(id: ID, state: WaitingPlanetKillResult) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(KillPlanetResult)),
            state,
        }
    }

    fn get_explorers_in_planet(&self, target_planet: &ID) -> Vec<(ID, ID)> {
        self.state
            .explorers_location_ref
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, planet_id)| *planet_id == target_planet)
            .map(|(explorer_id, planet_id)| (*explorer_id, *planet_id))
            .collect()
    }
}
