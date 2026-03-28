use crate::orchestrator::conversations::{EntitiesIDTuple, PlanetContext};
use crate::orchestrator::conversations::ErrorState;
use crate::orchestrator::conversations::CommonErrorTypes;
use std::time::Duration;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::{
    MoveToPlanetConversation,
};
use crate::orchestrator::conversations::{ChannelsContext, Conversation, ExplorerContext, PossibleExpectedKinds, PossibleMessage};
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
        channels_manager: ChannelsManagerRef,
        explorer_id: ID,
        curr_planet_id: Option<ID>,
        dst_planet_id: ID,
        explorers_location_ref: ExplorersLocationRef,
    },
    entities_id_fn: |this: &MoveToPlanetConversation<SendManualMoveRequest>  | { (Some(this.state.explorer_id), Some(this.state.dst_planet_id)) },
    transition_fn: send_manual_move_req_transition,
    methods_settings: {

    },
);


impl ExplorerContext for SendManualMoveRequest {
    fn get_explorer_id(&self) -> ID {
        self.explorer_id
    }
}

impl PlanetContext for SendManualMoveRequest {
    fn get_planet_id(&self) -> ID {
        self.dst_planet_id
    }
}

impl ChannelsContext for SendManualMoveRequest {
    fn get_channels_manager(&self) -> &ChannelsManagerRef {
        &self.channels_manager
    }
}
/// ### Transition Function: Initiating Manual Handover
///
/// This function prepares the standard movement handshake
///
/// #### 2. Handshake Initiation
/// The conversation transitions directly to [`SendIncomingRequest`], which begins
/// the process of notifying the destination planet of the entity's arrival.
fn send_manual_move_req_transition(this: Box<MoveToPlanetConversation<SendManualMoveRequest>>) -> Option<Box<dyn Conversation + Send + Sync>> {
    let state_struct = SendIncomingRequest::new(
        this.state.channels_manager,
        this.state.explorer_id,
        this.state.curr_planet_id,
        this.state.dst_planet_id,
        this.state.explorers_location_ref,
    );

    let next_conv = MoveToPlanetConversation::<SendIncomingRequest>::new(this.id, state_struct);
    Some(Box::new(next_conv))
}

