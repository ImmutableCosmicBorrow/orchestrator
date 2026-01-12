use crate::orchestrator::ExplorerBag;
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

///**Move To Planet Conversation - Wait Move To Planet Response**
///
/// This is the final state in the explorer movement sequence. After both the destination
/// and source planets have synchronized their communication channels, the Orchestrator
/// waits for the Explorer itself to confirm it has successfully transitioned.
///
/// Upon a successful [`ExplorerToOrchestrator::MovedToPlanetResult`], the Orchestrator
/// updates the global explorer location list. If the explorer was flagged as unable to
/// move (e.g., non-neighbor destination), the conversation closes gracefully without
/// updating the location.

// WAIT MOVE TO PLANET RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBag> for MoveToPlanetConversation<WaitMoveToPlanetResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitMoveToPlanetResponse`] state:
    ///
    /// Returns:
    ///
    /// * [None] - If the explorer moved correctly and the internal location list was updated.
    /// * [None] - If the explorer acknowledges the command but movement was invalid
    ///   (e.g., destination was not a neighbor).
    /// * [`ErrorState`] with [`MoveToPlanetErrors::ExplorerLocationNotFound`] - If the
    ///   internal location list does not contain the explorer.
    /// * [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] - If an unexpected message is received.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::MovedToPlanetResult {
                explorer_id,
                planet_id,
            },
        )) = msg_wrapped
        {
            //Explorer is moving, need to change its location in Orchestrator reference
            if self.state.is_explorer_moving {
                println!("Explorer {explorer_id} moved correctly to planet {planet_id}");
                return match self.move_explorer_location(explorer_id, planet_id) {
                    Ok(()) => {
                        println!("Changed Explorer Location in list to planet {planet_id}");
                        None
                    }
                    Err(e) => {
                        let err_struct = ErrorState::new(Box::new(e), self.id);
                        Some(Box::new(err_struct))
                    }
                };
            }
            //Explorer responded correctly and couldn't move
            println!(
                "Explorer {explorer_id} responded and cannot move due to dst planet not being a neighbor of current planet"
            );
            return None;
        }

        // Wrong message, closing Conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl MoveToPlanetConversation<WaitMoveToPlanetResponse> {
    /// The constructor for [`MoveToPlanetConversation`] in the [`WaitMoveToPlanetResponse`] state.
    ///
    /// Sets the expected message kind to [`ExplorerToOrchestratorKind::MovedToPlanetResult`].
    pub(crate) fn new(id: ID, state: WaitMoveToPlanetResponse) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                MovedToPlanetResult,
            )),
            state,
        }
    }

    /// Internal helper to update the thread-safe global list of explorer locations.
    ///
    /// Returns `Err(MoveToPlanetErrors::ExplorerLocationNotFound)` if the explorer ID is missing.
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
