use crate::convo_manager::OrchContextRef;
use crate::orchestrator::conversations::{EntitiesIDTuple};
use crate::orchestrator::conversations::ErrorState;
use crate::orchestrator::conversations::CommonErrorTypes;
use std::time::Duration;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::{
    MoveToPlanetConversation,
};
use crate::orchestrator::conversations::{ChannelsContext, Conversation, PossibleExpectedKinds, PossibleMessage};
use common_game::utils::ID;
use crate::create_request_state;
use crate::globals::TIMEOUT;
use crate::orchestrator::{ChannelsManagerRef, ExplorersLocationRef};
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::incoming_explorer::SendIncomingRequest;

///**Move To Planet Conversation - Send Manual Move Request**
///
/// This state handles movements triggered manually (e.g., by administrative commands or
/// specific game logic) rather than an explorer's own request. It serves as an
/// initialization point for forced transitions.

// --- SEND MANUAL MOVE REQUEST DEFINITION ---
create_request_state!(
    state_name: SendManualMoveRequest,
    conv_name: MoveToPlanetConversation,
    priority: 5,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        explorer_id: ID,
        curr_planet_id: Option<ID>,
        dst_planet_id: ID,
    },
    entities_id_fn: |this: &MoveToPlanetConversation<SendManualMoveRequest>  | { (Some(this.state.dst_planet_id), Some(this.state.explorer_id)) },
    transition_fn: send_manual_move_req_transition,
    methods_settings: {

    },
);

/// ### Transition Function: Initiating Manual Handover
///
/// This function prepares the standard movement handshake
///
/// #### 2. Handshake Initiation
/// The conversation transitions directly to [`SendIncomingRequest`], which begins
/// the process of notifying the destination planet of the entity's arrival.
fn send_manual_move_req_transition(this: Box<MoveToPlanetConversation<SendManualMoveRequest>>) -> Option<Box<dyn Conversation + Send + Sync>> {
    let state_struct = SendIncomingRequest::new(
        this.state.orch_context,
        this.state.explorer_id,
        this.state.curr_planet_id,
        this.state.dst_planet_id,
    );

    let next_conv = MoveToPlanetConversation::<SendIncomingRequest>::new(this.id, state_struct);
    Some(Box::new(next_conv))
}

