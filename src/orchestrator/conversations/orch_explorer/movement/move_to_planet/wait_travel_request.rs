use crate::convo_manager::OrchContextRef;
use crate::globals::get_explorer_timeout;
use crate::logging::{log_internal, LogTarget};
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::{
    MoveToPlanetConversation,
};
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::{ChannelsContext, CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage};
use crate::orchestrator::ChannelsManagerRef;
use crate::{create_response_state, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind,
};
use common_game::utils::ID;
use std::time::Duration;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::incoming_explorer::SendIncomingRequest;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::move_explorer::SendMoveRequest;

//**Move To Planet Conversation - Waiting Travel Request**
//
// This is the starting state of the movement lifecycle when it is requested by the explorer. It listens for a
// [`ExplorerToOrchestrator::TravelToPlanetRequest`] from an explorer.
//
// **Logic Flow:**
// 1. Verifies if the destination planet is a neighbor of the current planet via the Galaxy Map.
// 2. **If valid (Neighbors):** Initiates the arrival handshake by transitioning to [`SendIncomingRequest`],
//    which will notify the destination planet of the incoming explorer.
// 3. **If invalid (Non-neighbors):** Skips the destination handshake and transitions directly to
//    [`SendMoveRequest`] with a failure flag to inform the explorer that the move is impossible.
// 4. **Error Handling:** If an unexpected message type is received, transitions to an [`ErrorState`].

// --- WAIT TRAVEL REQUEST DEFINITION ---
create_response_state!(
    state: WaitingTravelRequest,
    conv: MoveToPlanetConversation,
    priority: 4,
    timeout: Some(get_explorer_timeout()),
    expected_msg: ExplorerToOrchKind(ExplorerToOrchestratorKind::TravelToPlanetRequest),
    fields: {
        explorer_id: ID,
        curr_planet_id: ID,
    },
    entities_id_closure: |this: &MoveToPlanetConversation<WaitingTravelRequest>| { (Some(this.state.curr_planet_id), Some(this.state.explorer_id)) },
    transition: wait_travel_req_transition,
    methods_settings: {

    },
);

impl WaitingTravelRequest {
    /// Accesses the Galaxy Map (thread-safe) to verify if the destination planet
    /// shares an edge with the current planet.
    ///
    /// Returns `true` if they are neighbors, `false` otherwise.
    fn check_neighbors(&self, planet_1: ID, planet_2: ID) -> bool {
        let galaxy = self.orch_context.galaxy.read().unwrap();
        if let (Some(curr_planet_ref), Some(dst_planet_ref)) =
            (galaxy.get(&planet_1), galaxy.get(&planet_2))
        {
            // Check if dst_planet_id is in the neighbors of curr_planet_ref
            return curr_planet_ref
                .neighbors_snapshot()
                .contains(&dst_planet_ref.id());
        }
        false
    }
}

/// Orchestrates the transition based on the received explorer request.
///
/// Validates the spatial relationship between planets and determines whether to
/// proceed with the travel handshake or reject the request.
fn wait_travel_req_transition(
    this: Box<MoveToPlanetConversation<WaitingTravelRequest>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::TravelToPlanetRequest {
        explorer_id,
        dst_planet_id,
        current_planet_id,
    })) = msg
    {
        // Destination is reachable. Transition to notify the destination planet.
        if this.state.check_neighbors(current_planet_id, dst_planet_id) {
            let next_state = SendIncomingRequest::new(
                this.state.orch_context,
                explorer_id,
                dst_planet_id,
                Some(current_planet_id),
            );
            //logging
            log_internal(
                LogTarget::Conversations,
                Channel::Trace,
                payload!(
                    action : "Destination planet can be reached, transitioning to SendIncomingRequest".to_string(),
                    explorer_id : explorer_id,
                    conversation_id : this.id,
                    planet_id: dst_planet_id,
                ),
            );
            //Transition
            let next_conv =
                MoveToPlanetConversation::<SendIncomingRequest>::new(this.id, next_state);
            return Some(Box::new(next_conv));
        }

        // Case 2: Destination unreachable. Transition to send a negative MoveToPlanet to the explorer
        let next_state =
            SendMoveRequest::new(this.state.orch_context,  explorer_id, dst_planet_id,false);
        let next_conv = MoveToPlanetConversation::<SendMoveRequest>::new(this.id, next_state);
        return Some(Box::new(next_conv));
    }

    // Case 3: Invalid message.
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}
