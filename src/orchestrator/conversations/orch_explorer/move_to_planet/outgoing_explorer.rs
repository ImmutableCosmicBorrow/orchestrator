use crate::logging_utils::log_msg_to;
use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::WaitingOutgoingResponse;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, SendMoveRequest, SendOutgoingRequest,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError,
};
use crate::payload;
use common_game::logging::{ActorType, Channel, EventType};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;

///**Move To Planet Conversation - Send Outgoing Request**
///
/// This state initiates the second half of the Orchestrator-to-planet handshake. It commands
/// the current (source) planet to release the explorer.
///
/// **Logic Flow:**
/// 1. Sends an [`OrchestratorToPlanet::OutgoingExplorerRequest`] to the explorer's current planet.
/// 2. **Success:** Transitions to [`WaitingOutgoingResponse`] to wait for the planet's confirmation.
/// 3. **Failure:** If the message cannot be sent (e.g., communication channel broken) or the sender to the current planet is not found, it
///    transitions to an [`ErrorState`].
// SEND OUTGOING REQUEST IMPLEMENTATION
impl Conversation<ExplorerBag> for MoveToPlanetConversation<SendOutgoingRequest> {
    /// Returns the unique ID of the conversation instance.
    fn get_id(&self) -> ID {
        self.id
    }

    /// Returns the ID of the current planet from which the explorer is departing.
    fn get_entity_id(&self) -> ID {
        self.state.curr_planet_struct.planet_id
    }

    /// This state is an action state (fire-and-forget); it does not wait for a message
    /// within this specific transition.
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        None
    }

    /// Executes the request to release the explorer and handles transmission outcomes:
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
    /// * **[`MoveToPlanetErrors::IncomingMessageFailed`]**: Occurs if the sender exists but the
    ///   underlying transport (channel) has failed or closed unexpectedly.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self.state.curr_planet_struct.to_planet(
            OrchestratorToPlanet::OutgoingExplorerRequest {
                explorer_id: self.state.explorer_struct.explorer_id,
            },
        ) {
            Ok(()) => {
                log_msg_to(
                    Channel::Debug,
                    EventType::MessageOrchestratorToPlanet,
                    (ActorType::Planet, self.state.curr_planet_struct.planet_id),
                    payload!(
                        action: "Sent Outgoing Request correctly, transitioning to WaitingOutgoingResponse".to_string(),
                        conversation_id: self.id
                    ),
                );

                let state_struct = WaitingOutgoingResponse::new(
                    self.state.explorer_struct,
                    self.state.planet_explorer_channels,
                    self.state.dst_planet_id,
                    self.state.explorers_location_ref,
                );
                //Transition to WaitingOutgoingResponse
                let new_state =
                    MoveToPlanetConversation::<WaitingOutgoingResponse>::new(self.id, state_struct);
                Some(Box::new(new_state))
            }

            Err(err) => {
                let error: Box<dyn ErrorType + Send + Sync> = match err {
                    ToPlanetError::SenderNotFound(id) => {
                        Box::new(CommonErrorTypes::PlanetSenderNotFound(id))
                    }
                    ToPlanetError::SendingMessageFailure(id) => {
                        Box::new(MoveToPlanetErrors::IncomingMessageFailed(id))
                    }
                };
                let error_state = ErrorState::new(error, self.id);
                Some(Box::new(error_state))
            }
        }
    }

    /// Returns the priority of this conversation.
    fn get_priority(&self) -> i32 {
        4
    }
}

impl MoveToPlanetConversation<SendOutgoingRequest> {
    /// Internal constructor for the [`SendOutgoingRequest`] state.
    pub fn new(id: ID, state: SendOutgoingRequest) -> Self {
        Self {
            id,
            state,
            expected_message: None,
        }
    }
}

///**Move To Planet Conversation - Waiting Outgoing Response**
///
/// This state represents the intermediate phase where the destination planet has already
/// acknowledged the explorer, and the Orchestrator is waiting for the source planet
/// to confirm the explorer has been successfully detached.
///
/// **Logic Flow:**
/// 1. Listens for a [`PlanetToOrchestrator::OutgoingExplorerResponse`] from the source planet.
/// 2. **If `Ok`:** Both planets have agreed. Transitions to [`SendMoveRequest`] to finally
///    update the explorer with their new destination.
/// 3. **If `Err`:** The source planet failed to release the explorer. Transitions to
///    an [`ErrorState`] to abort the movement.
/// 4. **Error Handling:** Transitions to [`ErrorState`] if an unexpected message is received.
// WAITING OUTGOING RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBag> for MoveToPlanetConversation<WaitingOutgoingResponse> {
    /// Returns the unique ID of the conversation instance.
    fn get_id(&self) -> ID {
        self.id
    }

    /// Returns the ID of the explorer attempting to move.
    fn get_entity_id(&self) -> ID {
        self.state.explorer_struct.explorer_id
    }

    /// Returns the expected kind: [`PlanetToOrchestratorKind::OutgoingExplorerResponse`].
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Processes the planet's response regarding the explorer's departure.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::PlanetToOrch(
            PlanetToOrchestrator::OutgoingExplorerResponse {
                planet_id,
                explorer_id,
                res,
            },
        )) = msg_wrapped
        {
            return if res.is_ok() {
                let state = SendMoveRequest::new(
                    self.state.explorers_location_ref,
                    self.state.dst_planet_id,
                    self.state.explorer_struct,
                    self.state.planet_explorer_channels,
                    true, // success flag for MoveToPlanet command
                );
                let next_conv = MoveToPlanetConversation::<SendMoveRequest>::new(self.id, state);
                Some(Box::new(next_conv))
            } else {
                let error_state = ErrorState::new(
                    Box::new(MoveToPlanetErrors::CurrPlanetFailed {
                        planet_id,
                        explorer_id,
                    }),
                    self.id,
                );
                Some(Box::new(error_state))
            };
        }

        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    /// Returns the priority of this conversation.
    fn get_priority(&self) -> i32 {
        4
    }
}

impl MoveToPlanetConversation<WaitingOutgoingResponse> {
    /// The constructor for [`MoveToPlanetConversation`] in the [`WaitingOutgoingResponse`] state.
    ///
    /// Automatically sets the expected message kind to [`PlanetToOrchestratorKind::OutgoingExplorerResponse`].
    pub(crate) fn new(id: ID, state: WaitingOutgoingResponse) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::PlanetToOrchKind(
                PlanetToOrchestratorKind::OutgoingExplorerResponse,
            )),
            state,
        }
    }
}
