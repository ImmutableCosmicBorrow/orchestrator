use crate::convo_manager::OrchContextRef;
use crate::globals::TIMEOUT;
use crate::logging::{LogTarget, log_msg_to};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::Duration;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::MoveToPlanetConversation;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::move_explorer::SendMoveRequest;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds,
    PossibleMessage,
};
use crate::orchestrator::conversations::{EntitiesIDTuple, PlanetCommunicator};
use crate::{create_request_state, create_response_state, payload};
use common_game::logging::{ActorType, Channel, EventType};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
//TODO: ASK THE OTHERS, IF OUTGOING FAILS WE MIGHT SEND AN OUTGOING TO THE DST_PLANET TO FREE THE CHANNEL OF THE EXPLORER, BUT THIS MIGHT RESULT IN AN INIFINTE LOOP

//**Move To Planet Conversation - Send Outgoing Request**
//
// This state initiates the second half of the Orchestrator-to-planet handshake. It commands
// the current (source) planet to release the explorer.
//
// **Logic Flow:**
// 1. Sends an [`OrchestratorToPlanet::OutgoingExplorerRequest`] to the explorer's current planet.
// 2. **Success:** Transitions to [`WaitingOutgoingResponse`] to wait for the planet's confirmation.
// 3. **Failure:** If the message cannot be sent (e.g., communication channel broken) or the sender to the current planet is not found, it
//    transitions to an [`ErrorState`].
// SEND OUTGOING REQUEST DEFINITION

create_request_state!(
    state_name: SendOutgoingRequest,
    conv_name: MoveToPlanetConversation,
    priority: 4,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        explorer_id: ID,
        dst_planet_id: ID,
        curr_planet_id: ID,
    },
    entities_id_fn: |this: &MoveToPlanetConversation<SendOutgoingRequest>  | { (Some(this.state.curr_planet_id), Some(this.state.explorer_id)) },
    transition_fn: send_incoming_req_transition,
    methods_settings: {

    },
);

/// Transition Function for the [`SendOutgoingRequest`] state:
///
/// The function tries to send a [`OrchestratorToPlanet::OutgoingExplorerRequest`] to the planet currenlty hosting
/// the explorer to release him.
///
/// ### Success Path
/// * **Message Sent**: If the communication with the source planet is successful, the conversation
///   advances to [`WaitingOutgoingResponse`]. This new state preserves the explorer's metadata
///   and the target destination ID to complete the handover later.
///
/// ### Error Paths
/// * **[`CommonErrorTypes::PlanetSenderNotFound`]**: Occurs if the Orchestrator has no registered
///   communication channel for the source planet ID. This represents a critical desync in the
///   Galaxy Map or Orchestrator state.
/// * **[`MoveToPlanetErrors::OutgoingMessageFailed`]**: Occurs if the sender exists but the
///   underlying transport (channel) has failed or closed unexpectedly.
fn send_incoming_req_transition(
    this: Box<MoveToPlanetConversation<SendOutgoingRequest>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this.state.to_planet(
        this.state.curr_planet_id,
        OrchestratorToPlanet::OutgoingExplorerRequest {
            explorer_id: this.state.explorer_id,
        },
    ) {
        Ok(()) => {
            log_msg_to(
                LogTarget::Conversations,
                Channel::Trace,
                EventType::MessageOrchestratorToPlanet,
                (ActorType::Planet, this.state.curr_planet_id),
                payload!(
                    action: "Sent Outgoing Request correctly, transitioning to WaitingOutgoingResponse".to_string(),
                    conversation_id: this.id
                ),
            );

            let state_struct = WaitingOutgoingResponse::new(
                this.state.orch_context,
                this.state.explorer_id,
                this.state.dst_planet_id,
                this.state.curr_planet_id,
            );
            //Transition to WaitingOutgoingResponse
            let new_state =
                MoveToPlanetConversation::<WaitingOutgoingResponse>::new(this.id, state_struct);
            Some(Box::new(new_state))
        }

        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

//**Move To Planet Conversation - Waiting Outgoing Response**
//
// This state represents the intermediate phase where the destination planet has already
// acknowledged the explorer, and the Orchestrator is waiting for the source planet
// to confirm the explorer has been successfully detached.
//
// **Logic Flow:**
// 1. Listens for a [`PlanetToOrchestrator::OutgoingExplorerResponse`] from the source planet.
// 2. **If `Ok`:** Both planets have agreed. Transitions to [`SendMoveRequest`] to finally
//    update the explorer with their new destination.
// 3. **If `Err`:** The source planet failed to release the explorer. Transitions to
//    an [`ErrorState`] to abort the movement.
// 4. **Error Handling:** Transitions to [`ErrorState`] if an unexpected message is received.
// WAITING OUTGOING RESPONSE DEFINITION

create_response_state!(
    state: WaitingOutgoingResponse,
    conv: MoveToPlanetConversation,
    priority: 4,
    timeout: Some(TIMEOUT),
    expected_msg: PlanetToOrchKind(PlanetToOrchestratorKind::OutgoingExplorerResponse),
    fields: {
        explorer_id: ID,
        dst_planet_id: ID,
        curr_planet_id: ID,
        
    },
    entities_id_closure: |this: &MoveToPlanetConversation<WaitingOutgoingResponse>| { (Some(this.state.curr_planet_id), Some(this.state.explorer_id)) },
    transition: wait_outgoing_res_transition,
    methods_settings: {

    },
);

/// ### Transition Function: Processing Explorer Release Results
///
/// This function evaluates whether the source planet has released the entity and
/// proceeds to confirm ghe movement to the explorer.
///
/// #### Source Release (`res.is_ok()`)
/// If the planet releases the explorer, the conversation transitions to a [`SendMoveRequest`] to confirm the movement.
///
/// #### Release Rejection (`res.is_err()`)
/// If the planet refuses (e.g., due to internal logic or population limits), the conversation transitions to a [`MoveToPlanetErrors::CurrPlanetFailed`],
/// in this case the situation is pretty delicate as the destination planet already received the channel to communicate to the explorer,
/// but the source planet didn't release it.
///
/// #### Error Handling
/// * **Protocol Violation**: If a message other than the Outgoing response is
///   received, transitions to [`CommonErrorTypes::WrongMessage`].
fn wait_outgoing_res_transition(
    this: Box<MoveToPlanetConversation<WaitingOutgoingResponse>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::OutgoingExplorerResponse {
        planet_id,
        explorer_id,
        res,
    })) = msg
    {
        return if res.is_ok() {
            let state = SendMoveRequest::new(
                this.state.orch_context,
                this.state.explorer_id,
                this.state.dst_planet_id,
                true, // success flag for MoveToPlanet command
            );
            let next_conv = MoveToPlanetConversation::<SendMoveRequest>::new(this.id, state);
            Some(Box::new(next_conv))
        } else {
            let error_state = ErrorState::new(
                Box::new(MoveToPlanetErrors::CurrPlanetFailed {
                    planet_id,
                    explorer_id,
                }),
                this.id,
            );
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        };
    }

    //Wrong message arrived, transition to error state
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}
