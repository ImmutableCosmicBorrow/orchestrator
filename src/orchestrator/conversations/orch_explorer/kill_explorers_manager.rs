use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_explorer::kill_explorer::{
    KillExplorerConversation, SendingKillExplorer,
};
use crate::orchestrator::conversations::{
    Conversation, PossibleExpectedKinds, PossibleMessage, SendersToExplorer, SendersToPlanet,
    ToExplorerStruct, ToPlanetStruct,
};
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer::KillExplorer;
use common_game::utils::ID;

#[derive(Clone)]
pub(crate) struct KillExplorersManager {
    id: ID,
    explorers_senders: SendersToExplorer,
    explorers_to_kill: Vec<(ID, ID)>,
    planet_senders: SendersToPlanet,
    handle_outgoing: bool,
}

impl KillExplorersManager {
    pub(crate) fn new(
        id: ID,
        explorers_senders: SendersToExplorer,
        planet_senders: SendersToPlanet,
        handle_outgoing: bool,
        explorers_to_kill: Vec<(ID, ID)>,
    ) -> Self {
        Self {
            id,
            explorers_senders,
            planet_senders,
            handle_outgoing,
            explorers_to_kill,
        }
    }
}

impl Conversation<ExplorerBag> for KillExplorersManager {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        None
    }

    fn transition(
        mut self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        if let Some((explorer_id, planet_id)) = self.explorers_to_kill.pop() {
            let conv_id = self.id.clone(); //TODO: CHANGE THIS TO DIFFERENT IDs
            let to_explorer_struct = ToExplorerStruct {
                explorer_id,
                explorers_senders: self.explorers_senders.clone(),
            };
            let to_planet_struct = ToPlanetStruct {
                planet_id,
                planets_senders: self.planet_senders.clone(),
            };
            let state_struct = SendingKillExplorer::new(
                to_explorer_struct,
                to_planet_struct,
                self.handle_outgoing,
                self,
            );
            let next_state =
                KillExplorerConversation::<SendingKillExplorer>::new(conv_id, state_struct);
            return Some(Box::new(next_state));
        }
        None
    }
}
