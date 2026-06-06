use crate::convo_manager::OrchContextRef;
use crate::globals::get_convo_timeout;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::ExplorerCommunicator;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::MoveToPlanetConversation;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::params::ConvoKind;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds,
    PossibleMessage,
};
use crate::{create_request_state, create_response_state, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestrator;
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestratorKind::MovedToPlanetResult;
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer::MoveToPlanet;
use common_game::protocols::planet_explorer::ExplorerToPlanet;
use common_game::utils::ID;
use crossbeam_channel::Sender;
use std::time::Duration;

//**Move To Planet Conversation - Send Move Request**
//
// This state serves as the "Command Dispatch" phase. It bridges the gap between the successful
// Orchestrator-Planet handshake and the Explorer's actual transition. Its primary role is to
// provide the Explorer with the communication channels to interact with its new planet.

// SEND MOVE REQUEST DEFINITION
create_request_state!(
    state_name: SendMoveRequest,
    conv_name: MoveToPlanetConversation,
    convo_kind: ConvoKind::MoveExplorerHigh,
    timeout: Some(get_convo_timeout()),
    expected_msg: None,
    fields: {
        explorer_id: ID,
        dst_planet_id: ID,
        is_explorer_moving: bool,
    },
    entities_id_fn: |this: &MoveToPlanetConversation<SendMoveRequest>  | { (Some(this.state.dst_planet_id), Some(this.state.explorer_id)) },
    transition_fn: send_incoming_req_transition,
    methods_settings: {

    },
);

/// ### Transition Function: Dispatching the Move Command
///
/// This function evaluates the authorization state of the movement and constructs the
/// final instruction for the Explorer. It handles the following logic:
///
/// #### Handshake Verification (`is_explorer_moving == true`)
/// If the planet-to-planet handshake was successful, the Orchestrator attempts to resolve
/// the communication channel for the destination planet.
/// * **Channel Found**: The `Sender<ExplorerToPlanet>` is extracted from the global
///   registry and attached to the `MoveToPlanet` message. This allows the explorer to
///   speak to the destination planet immediately.
/// * **Channel Missing**: If no active channel is found for the destination ID,
///   the move transitions to an [`ErrorState`] with [`MoveToPlanetErrors::NewSenderToPlanetNotFound`].
///
/// #### Unauthorized Movement (`is_explorer_moving == false`)
/// Used when a move was rejected (e.g., non-neighbors). The transition proceeds but
/// sends a `None` sender. This signals the Explorer to handle a failed transition.
///
/// #### Execution Outcomes
/// * **Success**: Advances to [`WaitMoveToPlanetResponse`].
/// * **Failure**: If the Explorer's channel is not working, transitions to an
///   [`ErrorState`] via [`CommonErrorTypes::MessageToExplorerFailed`].
fn send_incoming_req_transition(
    this: Box<MoveToPlanetConversation<SendMoveRequest>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    let sender_to_new_planet = if this.state.is_explorer_moving {
        // Explorer is moving, we need to find the sender to the planet
        if let Some(sender) = this.state.get_new_planet_sender() {
            Some(sender)
        } else {
            //Sender no found, transition to error state
            let error = Box::new(MoveToPlanetErrors::NewSenderToPlanetNotFound(
                this.state.dst_planet_id,
            ));
            let error_state = ErrorState::new(error, this.id);
            return Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>);
        }
    } else {
        //Explorer is not moving, send message with None as new channel
        None
    };

    // Send Message with correct sender
    let message = MoveToPlanet {
        sender_to_new_planet,
        planet_id: this.state.dst_planet_id,
    };

    match this.state.to_explorer(this.state.explorer_id, message) {
        Ok(()) => {
            let state_struct = WaitMoveToPlanetResponse::new(
                this.state.orch_context,
                this.state.explorer_id,
                this.state.dst_planet_id,
                this.state.is_explorer_moving,
            );
            let next_state =
                MoveToPlanetConversation::<WaitMoveToPlanetResponse>::new(this.id, state_struct);
            Some(Box::new(next_state))
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

impl SendMoveRequest {
    /// Retrieves the sender to the destination planet from the shared registry.
    fn get_new_planet_sender(&self) -> Option<Sender<ExplorerToPlanet>> {
        self.get_channels_manager()
            .get_exp_to_planet_sender(self.dst_planet_id)
    }
}

//**Move To Planet Conversation - Wait Move To Planet Response**
//
// This is the final terminal state in the movement sequence. It updates the location of the explorer in the structure held by the Orchestrator.
// WAIT MOVE TO PLANET RESPONSE IMPLEMENTATION
create_response_state!(
    state: WaitMoveToPlanetResponse,
    conv: MoveToPlanetConversation,
    convo_kind: ConvoKind::MoveExplorerLow,
    timeout: Some(get_convo_timeout()),
    expected_msg: ExplorerToOrchKind(MovedToPlanetResult),
    fields: {
        explorer_id: ID,
        dst_planet_id: ID,
        is_explorer_moving: bool,
    },
    entities_id_closure: |this: &MoveToPlanetConversation<WaitMoveToPlanetResponse>| { (Some(this.state.dst_planet_id), Some(this.state.explorer_id)) },
    transition: wait_move_response_transition,
    methods_settings: {

    },
);

/// ### Transition Function: Finalizing World State
///
/// This function acts as the final gatekeeper for the global location registry. It processes
/// the Explorer's arrival confirmation in three distinct ways:
///
/// #### 1. Successful Location Update
/// When `is_explorer_moving` is true, the Orchestrator performs a thread-safe update to
/// the `explorers_location_ref` map.
/// * **Update Success**: Returns `None`. This **terminates** the conversation successfully,
///   closing the movement lifecycle.
///
/// #### 2. Graceful Termination of Rejections
/// If the move was flagged as unauthorized, the explorer still acknowledges the instruction.
/// The function logs a `Warning` explaining that the move was blocked (e.g., non-neighbors)
/// and returns `None` to close the conversation without modifying the world state.
///
/// #### 3. Protocol Enforcement
/// Receiving any message other than the movement result results in a transition to
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`].
fn wait_move_response_transition(
    this: Box<MoveToPlanetConversation<WaitMoveToPlanetResponse>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::MovedToPlanetResult {
        explorer_id,
        planet_id,
    })) = msg
    {
        // Explorer is moving, need to change its location in Orchestrator reference
        if this.state.is_explorer_moving {
            log_internal(
                LogTarget::Conversations,
                Channel::Info,
                payload!(
                    action : "Explorer correctly moved to Planet",
                    explorer_id : explorer_id,
                    destination_planet_id : planet_id,
                    conversation_id : this.id,
                ),
            );

            //Update explorer location
            this.state.move_explorer_location(explorer_id, planet_id);
            log_internal(
                LogTarget::Conversations,
                Channel::Debug,
                payload!(
                        action : "Changed Explorer location in List, closing conversation",
                        explorer_id : explorer_id,
                        changed_to_planet_id : planet_id,
                        conversation_id : this.id
                ),
            );
        } else {
            // Explorer responded correctly but move was disallowed previously
            log_internal(
                LogTarget::Conversations,
                Channel::Warning,
                payload!(
                    action : "Explorer cannot move (destination not a neighbor), closing conversation",
                    explorer_id : explorer_id,
                    destination_planet_id : planet_id,
                    conversation_id : this.id
                ),
            );
        }
        return None; // Graceful close
    }
    // Wrong message, transitioning to error state
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}

impl WaitMoveToPlanetResponse {
    /// Internal helper to update the thread-safe global list of explorer locations.
    fn move_explorer_location(&self, explorer_id: ID, dst_planet_id: ID) {
        self.orch_context
            .explorers_location
            .insert(explorer_id, dst_planet_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::orch_explorer::test_utils::{
        add_broken_explorer_sender, add_working_explorer_sender, make_test_context,
    };
    use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
    use crossbeam_channel::unbounded;

    const CONV_ID: ID = 1;
    const EXPLORER_ID: ID = 2;
    const DST_PLANET_ID: ID = 10;

    fn make_send_conv(
        orch_context: OrchContextRef,
        is_moving: bool,
    ) -> Box<MoveToPlanetConversation<SendMoveRequest>> {
        let state = SendMoveRequest::new(orch_context, EXPLORER_ID, DST_PLANET_ID, is_moving);
        Box::new(MoveToPlanetConversation::<SendMoveRequest>::new(
            CONV_ID, state,
        ))
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
        is_moving: bool,
    ) -> Box<MoveToPlanetConversation<WaitMoveToPlanetResponse>> {
        let state =
            WaitMoveToPlanetResponse::new(orch_context, EXPLORER_ID, DST_PLANET_ID, is_moving);
        Box::new(MoveToPlanetConversation::<WaitMoveToPlanetResponse>::new(
            CONV_ID, state,
        ))
    }

    #[test]
    fn send_move_not_moving() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);

        let conv = make_send_conv(test_ctx.clone(), false);
        let next_conv = conv.transition(None).expect("Should transition");
        assert_eq!(next_conv.get_id(), CONV_ID);
    }

    #[test]
    fn send_move_missing_new_planet_sender() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);

        // is_moving = true, but we haven't added the exp_to_planet sender
        let conv = make_send_conv(test_ctx.clone(), true);
        let next_conv = conv.transition(None).expect("Should return ErrorState");
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!(
                "sender to dest planet {DST_PLANET_ID} not found, planets already changed explorer channels but explorer did not"
            ))
        );
    }

    #[test]
    fn send_move_message_failure() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        add_broken_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);

        let conv = make_send_conv(test_ctx.clone(), false);
        let next_conv = conv.transition(None).expect("Should return ErrorState");
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("failed to send message to explorer {EXPLORER_ID}"))
        );
    }

    #[test]
    fn wait_move_success_moving() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone(), true);

        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::MovedToPlanetResult {
            explorer_id: EXPLORER_ID,
            planet_id: DST_PLANET_ID,
        });

        let next_conv = conv.transition(Some(msg));
        assert!(next_conv.is_none());
        assert_eq!(
            *test_ctx.explorers_location.get(&EXPLORER_ID).unwrap(),
            DST_PLANET_ID
        );
    }

    #[test]
    fn wait_move_success_not_moving() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone(), false);

        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::MovedToPlanetResult {
            explorer_id: EXPLORER_ID,
            planet_id: DST_PLANET_ID,
        });

        let next_conv = conv.transition(Some(msg));
        assert!(next_conv.is_none());
        assert!(test_ctx.explorers_location.get(&EXPLORER_ID).is_none());
    }

    #[test]
    fn wait_move_wrong_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone(), true);

        let wrong_msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::StartExplorerAIResult {
                explorer_id: (EXPLORER_ID),
            });

        let next_conv = conv
            .transition(Some(wrong_msg))
            .expect("Should return ErrorState");
        assert_eq!(
            next_conv.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }
}
