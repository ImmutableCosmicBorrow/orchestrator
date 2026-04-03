use crate::globals::get_explorer_timeout;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::WaitingTravelRequest;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::{
    MoveToPlanetConversation, SendIncomingRequest, SendMoveRequest,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
};
use crate::payload;
use common_explorer::ExplorerBagContent;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind,
};
use common_game::utils::ID;
use std::time::Duration;

///**Move To Planet Conversation - Waiting Travel Request**
///
/// This is the starting state of the movement lifecycle when it is requested by the explorer. It listens for a
/// [`ExplorerToOrchestrator::TravelToPlanetRequest`] from an explorer.
///
/// **Logic Flow:**
/// 1. Verifies if the destination planet is a neighbor of the current planet via the Galaxy Map.
/// 2. **If valid (Neighbors):** Initiates the arrival handshake by transitioning to [`SendIncomingRequest`],
///    which will notify the destination planet of the incoming explorer.
/// 3. **If invalid (Non-neighbors):** Skips the destination handshake and transitions directly to
///    [`SendMoveRequest`] with a failure flag to inform the explorer that the move is impossible.
/// 4. **Error Handling:** If an unexpected message type is received, transitions to an [`ErrorState`].
// WAITING TRAVEL REQUEST IMPLEMENTATION
impl Conversation<ExplorerBagContent> for MoveToPlanetConversation<WaitingTravelRequest> {
    /// Returns the unique ID of the conversation instance.
    fn get_id(&self) -> ID {
        self.id
    }

    /// Returns the ID of the explorer associated with this movement request.
    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (
            Some(self.state.curr_planet_struct.planet_id),
            Some(self.state.explorer_struct.explorer_id),
        )
    }

    /// Returns the specific message type this state is currently polling for.
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Orchestrates the transition based on the received explorer request.
    ///
    /// Validates the spatial relationship between planets and determines whether to
    /// proceed with the travel handshake or reject the request.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::TravelToPlanetRequest {
                explorer_id: _explorer_id,
                dst_planet_id: _dst_planet_id,
                current_planet_id: _current_planet_id,
            },
        )) = msg_wrapped
        {
            // Case 1: Destination is reachable. Transition to notify the destination planet.
            if self.check_neighbors() {
                let next_state = SendIncomingRequest::new(
                    Some(self.state.curr_planet_struct),
                    self.state.explorer_struct.clone(),
                    self.state.dst_planet_struct.clone(),
                    self.state.planet_explorer_channels,
                    self.state.explorers_location_ref,
                    true,
                );
                //logging
                log_internal(
                    LogTarget::Conversations,
                    Channel::Trace,
                    payload!(
                        action : "Destination planet can be reached, transitioning to SendIncomingRequest".to_string(),
                        explorer_id : self.state.explorer_struct.explorer_id,
                        conversation_id : self.id,
                        planet_id: self.state.dst_planet_struct.planet_id
                    ),
                );
                //Transition
                let next_conv =
                    MoveToPlanetConversation::<SendIncomingRequest>::new(self.id, next_state);
                return Some(Box::new(next_conv));
            }

            // Case 2: Destination unreachable. Transition to send a negative MoveToPlanet to the explorer
            let next_state = SendMoveRequest::new(
                self.state.explorers_location_ref,
                self.state.dst_planet_struct.planet_id,
                self.state.explorer_struct,
                self.state.planet_explorer_channels,
                false, // 'false' indicates the move is not allowed
            );
            let next_conv = MoveToPlanetConversation::<SendMoveRequest>::new(self.id, next_state);

            return Some(Box::new(next_conv));
        }

        // Case 3: Invalid message or timeout.
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
    }

    /// Returns the priority of this conversation within the orchestrator's queue.
    fn get_priority(&self) -> i32 {
        4
    }

    // Longer timeout, since it involves a communication with an Explorer
    fn get_timeout(&self) -> Option<Duration> {
        Some(get_explorer_timeout())
    }
}

impl MoveToPlanetConversation<WaitingTravelRequest> {
    /// Accesses the Galaxy Map (thread-safe) to verify if the destination planet
    /// shares an edge with the current planet.
    ///
    /// Returns `true` if they are neighbors, `false` otherwise.
    fn check_neighbors(&self) -> bool {
        let galaxy = self.state.galaxy.read().unwrap();
        if let (Some(curr_planet_ref), Some(dst_planet_ref)) = (
            galaxy.get(&self.state.curr_planet_struct.planet_id),
            galaxy.get(&self.state.dst_planet_struct.planet_id),
        ) {
            // Check if dst_planet_id is in the neighbors of curr_planet_ref
            return curr_planet_ref
                .neighbors_snapshot()
                .contains(&dst_planet_ref.id());
        }
        false
    }

    /// Internal constructor to initialize the conversation in its starting state.
    pub(crate) fn new(id: ID, state: WaitingTravelRequest) -> Self {
        Self {
            id,
            state,
            expected_message: Some(ExplorerToOrchKind(
                ExplorerToOrchestratorKind::TravelToPlanetRequest,
            )),
        }
    }
}
