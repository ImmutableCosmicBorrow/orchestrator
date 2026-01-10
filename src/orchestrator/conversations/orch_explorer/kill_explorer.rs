use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_explorer::kill_explorers_manager::KillExplorersManager;
use crate::orchestrator::conversations::orch_planet::outgoing_explorer::{
    OutgoingExplorer, SendingOutgoingRequest,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct, ToPlanetStruct,
};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

pub(crate) struct SendingKillExplorer {
    to_explorer_struct: ToExplorerStruct,
    to_planet_struct: ToPlanetStruct,
    handle_outgoing: bool,
    manager: Box<KillExplorersManager>,
}
impl SendingKillExplorer {
    pub(crate) fn new(
        to_explorer_struct: ToExplorerStruct,
        to_planet_struct: ToPlanetStruct,
        handle_outgoing: bool,
        manager: Box<KillExplorersManager>,
    ) -> Self {
        Self {
            to_explorer_struct,
            to_planet_struct,
            handle_outgoing,
            manager,
        }
    }
}

struct WaitingKillExplorerResult {
    explorer_id: ID,
    to_planet_struct: ToPlanetStruct,
    handle_outgoing: bool,
    manager: Box<KillExplorersManager>,
}

impl WaitingKillExplorerResult {
    fn new(
        explorer_id: ID,
        to_planet_struct: ToPlanetStruct,
        handle_outgoing: bool,
        manager: Box<KillExplorersManager>,
    ) -> Self {
        Self {
            explorer_id,
            to_planet_struct,
            handle_outgoing,
            manager,
        }
    }
}

pub(crate) struct KillExplorerConversation<State> {
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
    state: State,
}

impl Conversation<ExplorerBag> for KillExplorerConversation<SendingKillExplorer> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::KillExplorer)
        {
            Ok(()) => {
                let explorer_id = self.state.to_explorer_struct.explorer_id;
                let state_struct = WaitingKillExplorerResult::new(
                    explorer_id,
                    self.state.to_planet_struct,
                    self.state.handle_outgoing,
                    self.state.manager,
                );
                let next_state = KillExplorerConversation::<WaitingKillExplorerResult>::new(
                    self.id,
                    state_struct,
                );
                Some(Box::new(next_state))
            }
            Err(err) => {
                let error = match err {
                    ToExplorerError::SendingMessageFailure(id) => {
                        CommonErrorTypes::MessageToExplorerFailed(id)
                    }
                    ToExplorerError::SenderNotFound(id) => {
                        CommonErrorTypes::ExplorerSenderNotFound(id)
                    }
                };
                let error_state = ErrorState::new(Box::new(error), self.id);
                Some(Box::new(error_state))
            }
        }
    }

    fn get_priority(&self) -> i32 {
        5
    }
}

impl KillExplorerConversation<SendingKillExplorer> {
    pub(crate) fn new(id: ID, state: SendingKillExplorer) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

impl Conversation<ExplorerBag> for KillExplorerConversation<WaitingKillExplorerResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::KillExplorerResult {
            explorer_id,
        })) = msg_wrapped
        {
            println!("Killed explorer {explorer_id}");
            if self.state.handle_outgoing {
                let state_struct = SendingOutgoingRequest::new(
                    self.state.to_planet_struct,
                    explorer_id,
                    self.state.manager,
                );
                let next_state =
                    OutgoingExplorer::<SendingOutgoingRequest>::new(self.id, state_struct);
                return Some(Box::new(next_state));
            }
            println!(
                "Conversation already took care of outgoing explorer {explorer_id}, going back to manager!"
            );
            return Some(self.state.manager);
        }

        //Wrong Message, return to manager
        Some(self.state.manager)
    }

    fn get_priority(&self) -> i32 {
        5
    }
}

impl KillExplorerConversation<WaitingKillExplorerResult> {
    fn new(id: ID, state: WaitingKillExplorerResult) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::KillExplorerResult,
            )),
            state,
        }
    }
}
