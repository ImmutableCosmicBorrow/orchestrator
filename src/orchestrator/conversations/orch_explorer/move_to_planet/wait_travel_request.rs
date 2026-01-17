use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, SendIncomingRequest, SendMoveRequest,
};
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    WaitMoveToPlanetResponse, WaitingIncomingResponse, WaitingTravelRequest,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind,
};
use common_game::utils::ID;

///**Move To Planet Conversation - Waiting Travel Request**
///
/// This is the starting state of the movement lifecycle. It listens for a
/// [`ExplorerToOrchestrator::TravelToPlanetRequest`] from an explorer.
///
/// **Logic Flow:**
/// 1. Verifies if the destination planet is a neighbor of the current planet via the Galaxy Map.
/// 2. If valid, sends an [`IncomingExplorerRequest`] to the destination planet and transitions
///    to [`WaitingIncomingResponse`].
/// 3. If invalid (not neighbors), it informs the explorer movement is impossible and transitions
///    directly to [`WaitMoveToPlanetResponse`] to gracefully close the attempt.
// WAITING TRAVEL REQUEST IMPLEMENTATION
impl Conversation<ExplorerBag> for MoveToPlanetConversation<WaitingTravelRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::TravelToPlanetRequest {
                explorer_id: _explorer_id,
                dst_planet_id: _dst_planet_id,
                current_planet_id: _current_planet_id,
            },
        )) = msg_wrapped
        {
            if self.check_neighbors() {
                let next_state = SendIncomingRequest::new(
                    self.state.curr_planet_struct,
                    self.state.explorer_struct,
                    self.state.dst_planet_struct,
                    self.state.planet_explorer_channels,
                    self.state.explorers_location_ref,
                    true,
                );
                let next_conv =
                    MoveToPlanetConversation::<SendIncomingRequest>::new(self.id, next_state);
                return Some(Box::new(next_conv));
            }

            // Non-neighbors logic
            let next_state = SendMoveRequest::new(
                self.state.explorers_location_ref,
                self.state.dst_planet_struct.planet_id,
                self.state.explorer_struct,
                self.state.planet_explorer_channels,
                false,
            );
            let next_conv = MoveToPlanetConversation::<SendMoveRequest>::new(self.id, next_state);

            // Added 'return' so it doesn't hit the ErrorState below
            return Some(Box::new(next_conv));
        }

        // If msg_wrapped was None or didn't match the pattern
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl MoveToPlanetConversation<WaitingTravelRequest> {
    /// Checks the Galaxy Map to see if the target destination is reachable from the current location.
    fn check_neighbors(&self) -> bool {
        let galaxy = self.state.galaxy.lock().unwrap();
        if let (Some(curr_planet_ref), Some(dst_planet_ref)) = (
            galaxy.get(&self.state.curr_planet_struct.planet_id),
            galaxy.get(&self.state.dst_planet_struct.planet_id),
        ) {
            return curr_planet_ref.has_neighbor(&dst_planet_ref.inner);
        }
        false
    }

    /// Internal constructor for the initial state.
    fn new(id: ID, state: WaitingTravelRequest) -> Self {
        Self {
            id,
            state,
            expected_message: Some(ExplorerToOrchKind(
                ExplorerToOrchestratorKind::TravelToPlanetRequest,
            )),
        }
    }
}
