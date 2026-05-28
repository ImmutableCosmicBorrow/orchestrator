use crate::convo_manager::OrchContextRef;
use crate::orchestrator::conversations::{EntitiesIDTuple};
use crate::orchestrator::conversations::ErrorState;
use crate::orchestrator::conversations::CommonErrorTypes;
use std::time::Duration;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::{
    MoveToPlanetConversation,
};
use crate::orchestrator::conversations::{ChannelsContext, Conversation, PossibleExpectedKinds, PossibleMessage};
use crate::orchestrator::conversations::params::ConvoKind;
use common_game::utils::ID;
use crate::create_request_state;
use crate::globals::TIMEOUT;
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::incoming_explorer::SendIncomingRequest;

//**Move To Planet Conversation - Send Manual Move Request**
//
// This state handles movements triggered manually (e.g., by administrative commands or
// specific game logic) rather than an explorer's own request. It serves as an
// initialization point for forced transitions.

// --- SEND MANUAL MOVE REQUEST DEFINITION ---
create_request_state!(
    state_name: SendManualMoveRequest,
    convo_kind: ConvoKind::ManualMoveToPlanet,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        explorer_id: ID,
        dst_planet_id: ID,
        curr_planet_id: Option<ID>,
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
fn send_manual_move_req_transition(
    this: Box<MoveToPlanetConversation<SendManualMoveRequest>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    let state_struct = SendIncomingRequest::new(
        this.state.orch_context,
        this.state.explorer_id,
        this.state.dst_planet_id,
        this.state.curr_planet_id,
    );

    let next_conv = MoveToPlanetConversation::<SendIncomingRequest>::new(this.id, state_struct);
    Some(Box::new(next_conv))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::orch_explorer::test_utils::make_test_context;
    use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
    use crossbeam_channel::unbounded;

    const CONV_ID: ID = 1;
    const EXPLORER_ID: ID = 2;
    const DST_PLANET_ID: ID = 10;
    const CURR_PLANET_ID: ID = 20;

    fn make_send_conv(
        orch_context: OrchContextRef,
    ) -> Box<MoveToPlanetConversation<SendManualMoveRequest>> {
        let state = SendManualMoveRequest::new(
            orch_context,
            EXPLORER_ID,
            DST_PLANET_ID,
            Some(CURR_PLANET_ID),
        );
        Box::new(MoveToPlanetConversation::<SendManualMoveRequest>::new(
            CONV_ID, state,
        ))
    }

    #[test]
    fn send_manual_move_transition() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_send_conv(test_ctx.clone());

        let next_conv = conv.transition(None).expect("Should transition");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_expected_kind(), None);
    }
}
