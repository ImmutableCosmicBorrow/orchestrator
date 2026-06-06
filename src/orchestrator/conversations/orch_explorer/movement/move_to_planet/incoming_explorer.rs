use crate::convo_manager::OrchContextRef;
use crate::globals::get_convo_timeout;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::move_explorer::SendMoveRequest;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::outgoing_explorer::SendOutgoingRequest;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::MoveToPlanetConversation;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{ChannelsContext, CommonErrorTypes, Conversation, ErrorState, PlanetCommunicator, PossibleExpectedKinds, PossibleMessage};
use crate::orchestrator::conversations::params::ConvoKind;
use crate::orchestrator::ChannelsManagerRef;
use crate::{create_request_state, create_response_state};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::protocols::planet_explorer::PlanetToExplorer;
use common_game::utils::ID;
use crossbeam_channel::Sender;
use std::time::Duration;

//**Move To Planet Conversation - Send Incoming Request**
//
// This state initiates the acquisition phase of the movement protocol. It is responsible
// for notifying the destination planet that an explorer is arriving and providing that
// planet with the necessary communication bridge to contact the entity.

// --- SEND INCOMING REQUEST DEFINITION ---
create_request_state!(
    state_name: SendIncomingRequest,
    conv_name: MoveToPlanetConversation,
    convo_kind: ConvoKind::IncomingExplorer,
    timeout: Some(get_convo_timeout()),
    expected_msg: None,
    fields: {
        explorer_id: ID,
        dst_planet_id: ID,
        curr_planet_id: Option<ID>,
    },
    entities_id_fn: |this: &MoveToPlanetConversation<SendIncomingRequest>  | { (Some(this.state.dst_planet_id), Some(this.state.explorer_id)) },
    transition_fn: send_incoming_req_transition,
    methods_settings: {

    },
);

/// ### Transition Function: Initiating the Acquisition
///
/// This function performs the critical handshake setup by resolving communication
/// channels and dispatching the acquisition request.
///
/// #### 1. Channel Resolution
/// The Orchestrator attempts to retrieve the `Sender<PlanetToExplorer>` for the explorer.
/// * **Success**: If the sender is found in the registry, the Orchestrator wraps it in
///   an `IncomingExplorerRequest` and sends it to the destination planet.
/// * **Failure**: If the explorer's channel is missing,
///   it transitions to an [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`].
///
/// #### 2. Handshake Dispatch
/// * **Success Path**: On a successful message delivery to the destination planet, the
///   conversation advances to [`WaitingIncomingResponse`].
/// * **Communication Errors**: If the planet sender is missing or the channel is
///   closed, it transitions to [`ErrorState`] with either [`PlanetSenderNotFound`]
///   or [`IncomingMessageFailed`].
fn send_incoming_req_transition(
    this: Box<MoveToPlanetConversation<SendIncomingRequest>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    //Try to get the sender to the explorer to give to the p,anet that will host the explorer
    if let Some(sender) = this.state.get_plan_to_explorer_sender() {
        // Try to initiate the handshake with the destination planet
        return match this.state.to_planet(
            this.state.dst_planet_id,
            OrchestratorToPlanet::IncomingExplorerRequest {
                explorer_id: this.state.explorer_id,
                new_sender: sender,
            },
        ) {
            Ok(()) => {
                let state_struct = WaitingIncomingResponse::new(
                    this.state.orch_context,
                    this.state.explorer_id,
                    this.state.dst_planet_id,
                    this.state.curr_planet_id,
                );

                let new_state =
                    MoveToPlanetConversation::<WaitingIncomingResponse>::new(this.id, state_struct);
                Some(Box::new(new_state))
            }

            //Sender to planet not found or message failed
            Err(err) => {
                let error_state = ErrorState::new(Box::new(err), this.id);
                Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
            }
        };
    }

    //Sender to explorer not found
    let error_state = ErrorState::new(
        Box::new(CommonErrorTypes::ExplorerSenderNotFound(
            this.state.explorer_id,
        )),
        this.id,
    );
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}

impl SendIncomingRequest {
    fn get_plan_to_explorer_sender(&self) -> Option<Sender<PlanetToExplorer>> {
        self.get_channels_manager()
            .get_planet_to_exp_sender(self.explorer_id)
    }
}

//WAITING INCOMING RESPONSE DEFINITION

create_response_state!(
    state: WaitingIncomingResponse,
    conv: MoveToPlanetConversation,
    convo_kind: ConvoKind::IncomingExplorer,
    timeout: Some(get_convo_timeout()),
    expected_msg: PlanetToOrchKind(PlanetToOrchestratorKind::IncomingExplorerResponse),
    fields: {
        explorer_id: ID,
        dst_planet_id: ID,
        curr_planet_id: Option<ID>,
    },
    entities_id_closure: |this: &MoveToPlanetConversation<WaitingIncomingResponse>| { (Some(this.state.dst_planet_id), Some(this.state.explorer_id)) },
    transition: wait_incoming_res_transition,
    methods_settings: {

    },
);

/// ### Transition Function: Processing Acquisition Results
///
/// This function evaluates whether the destination planet has accepted the entity and
/// determines if the handshake needs to proceed to a source-planet release phase.
///
/// #### 1. Destination Acceptance (`res.is_ok()`)
/// If the planet accepts the explorer, the transition logic branches based on the
/// `curr_planet_id` field:
/// * **`curr_planet_id` is Some (Standard Move)**: The explorer is currently on a planet. The Orchestrator
///   must now command that planet to release the entity.
///   To do so, it transitions to [`SendOutgoingRequest`].
/// * **`curr_planet_id` is None (Spawn/Forced)**: The explorer does not have a current planet (or is
///   being moved externally). It skips the source release and transitions directly
///   to [`SendMoveRequest`] to notify the explorer of the success.
///
/// #### 2. Destination Rejection (`res.is_err()`)
/// If the planet refuses (e.g., due to internal logic or population limits), the
/// move is aborted. Transitions to [`SendMoveRequest`] with a negative flag signaling to the explorer that the move is not possible.
///
/// #### 3. Error Handling
/// * **Dispatch Failure**: If the release request to the current planet fails, it
///   transitions to an error state.
/// * **Protocol Violation**: If a message other than the acquisition response is
///   received, transitions to [`CommonErrorTypes::WrongMessage`].
fn wait_incoming_res_transition(
    this: Box<MoveToPlanetConversation<WaitingIncomingResponse>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::IncomingExplorerResponse {
        planet_id: _planet_id,
        explorer_id: _explorer_id,
        res,
    })) = msg
    {
        return if res.is_ok() {
            //Explorer comes from another planet, transition to SendOutgoingRequest
            if let Some(curr_planet) = this.state.curr_planet_id {
                let state_struct = SendOutgoingRequest::new(
                    this.state.orch_context,
                    this.state.explorer_id,
                    this.state.dst_planet_id,
                    curr_planet,
                );
                //transition to SendOutgoingRequest
                let next_state =
                    MoveToPlanetConversation::<SendOutgoingRequest>::new(this.id, state_struct);
                Some(Box::new(next_state))
            } else {
                let state = SendMoveRequest::new(
                    this.state.orch_context,
                    this.state.explorer_id,
                    this.state.dst_planet_id,
                    true,
                );
                //transition to SendMoveRequest
                let next_state = MoveToPlanetConversation::<SendMoveRequest>::new(this.id, state);
                Some(Box::new(next_state))
            }
        } else {
            //Incoming Request has failed, transitioning to SendMoveRequest with flag is_explorer_moving to false

            let state = SendMoveRequest::new(
                this.state.orch_context,
                this.state.explorer_id,
                this.state.dst_planet_id,
                false,
            );
            //transition to SendMoveRequest
            let next_state = MoveToPlanetConversation::<SendMoveRequest>::new(this.id, state);
            Some(Box::new(next_state))
        };
    }
    //Wrong message arrived
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
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
    ) -> Box<MoveToPlanetConversation<SendIncomingRequest>> {
        let state = SendIncomingRequest::new(
            orch_context,
            EXPLORER_ID,
            DST_PLANET_ID,
            Some(CURR_PLANET_ID),
        );
        Box::new(MoveToPlanetConversation::<SendIncomingRequest>::new(
            CONV_ID, state,
        ))
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
        curr_planet: Option<ID>,
    ) -> Box<MoveToPlanetConversation<WaitingIncomingResponse>> {
        let state =
            WaitingIncomingResponse::new(orch_context, EXPLORER_ID, DST_PLANET_ID, curr_planet);
        Box::new(MoveToPlanetConversation::<WaitingIncomingResponse>::new(
            CONV_ID, state,
        ))
    }

    #[test]
    fn send_incoming_missing_explorer_sender() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_send_conv(test_ctx.clone());

        let next_conv = conv.transition(None).expect("Should return an ErrorState");
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("sender to explorer {EXPLORER_ID} not found"))
        );
    }

    #[test]
    fn wait_incoming_success_with_curr_planet() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone(), Some(CURR_PLANET_ID));

        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::IncomingExplorerResponse {
            planet_id: DST_PLANET_ID,
            explorer_id: EXPLORER_ID,
            res: Ok(()),
        });

        let next_conv = conv.transition(Some(msg)).expect("Should transition");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_expected_kind(), None); // SendOutgoingRequest expects no message at first
    }

    #[test]
    fn wait_incoming_success_without_curr_planet() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone(), None);

        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::IncomingExplorerResponse {
            planet_id: DST_PLANET_ID,
            explorer_id: EXPLORER_ID,
            res: Ok(()),
        });

        let next_conv = conv.transition(Some(msg)).expect("Should transition");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_expected_kind(), None); // SendMoveRequest expects no message at first
    }

    #[test]
    fn wait_incoming_rejection() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone(), Some(CURR_PLANET_ID));

        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::IncomingExplorerResponse {
            planet_id: DST_PLANET_ID,
            explorer_id: EXPLORER_ID,
            res: Err("No space".into()),
        });

        let next_conv = conv.transition(Some(msg)).expect("Should transition");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_expected_kind(), None); // SendMoveRequest expects no message at first
    }

    #[test]
    fn wait_incoming_wrong_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone(), Some(CURR_PLANET_ID));

        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::KillPlanetResult {
            planet_id: DST_PLANET_ID,
        });

        let result = conv
            .transition(Some(msg))
            .expect("Should return ErrorState");
        assert_eq!(
            result.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }
}
