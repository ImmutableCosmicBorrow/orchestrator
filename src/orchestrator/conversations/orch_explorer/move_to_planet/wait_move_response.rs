use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, WaitMoveToPlanetResponse,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
};
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestrator;
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestratorKind::MovedToPlanetResult;
use common_game::utils::ID;

//WaitMoveToPlanetResponse Implementation
impl Conversation<ExplorerBag> for MoveToPlanetConversation<WaitMoveToPlanetResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::MovedToPlanetResult { explorer_id },
        )) = msg_wrapped
        {
            if self.state.is_explorer_moving {
                println!(
                    "Explorer {explorer_id} moved correctly to planet {}",
                    self.state.dst_planet_id
                );
                return match self.move_explorer_location(explorer_id, self.state.dst_planet_id) {
                    Ok(()) => {
                        println!(
                            "Changed Explorer Location in list to planet {}",
                            self.state.dst_planet_id
                        );
                        None
                    }
                    Err(e) => {
                        let err_struct = ErrorState::new(Box::new(e), self.id);
                        Some(Box::new(err_struct))
                    }
                };
            }
            println!(
                "Explorer {explorer_id} responded and cannot move due to dst planet not being a neighbor of current planet"
            );
            return None;
        }

        //Wrong message, closing Conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }
}

impl MoveToPlanetConversation<WaitMoveToPlanetResponse> {
    pub(crate) fn new(id: ID, state: WaitMoveToPlanetResponse) -> Self {
        Self {
            id,
            expected_message: Some(ExplorerToOrchKind(MovedToPlanetResult)),
            state,
        }
    }

    fn move_explorer_location(
        &self,
        explorer_id: ID,
        dst_planet_id: ID,
    ) -> Result<(), MoveToPlanetErrors> {
        if let Some(location) = self
            .state
            .explorers_location_ref
            .lock()
            .unwrap()
            .get_mut(&explorer_id)
        {
            *location = dst_planet_id;
            return Ok(());
        }

        Err(MoveToPlanetErrors::ExplorerLocationNotFound(explorer_id))
    }
}
