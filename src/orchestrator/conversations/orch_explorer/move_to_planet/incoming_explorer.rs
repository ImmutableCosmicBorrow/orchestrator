use crate::orchestrator::conversations::orch_explorer::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, SendIncomingRequest, SendMoveRequest, SendOutgoingRequest,
    WaitingIncomingResponse,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError,
};
use common_explorer::ExplorerBagContent;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::protocols::planet_explorer::PlanetToExplorer;
use common_game::utils::ID;
use crossbeam_channel::Sender;

///**Move To Planet Conversation - Send Incoming Request**
///
/// This state initiates the acquisition phase of the movement protocol. It is responsible
/// for notifying the destination planet that an explorer is arriving and providing that
/// planet with the necessary communication bridge to contact the entity.
// SEND INCOMING REQUEST IMPLEMENTATION
impl Conversation<ExplorerBagContent> for MoveToPlanetConversation<SendIncomingRequest> {
    /// Returns the unique ID of the conversation instance.
    fn get_id(&self) -> ID {
        self.id
    }

    /// Returns the ID of the destination planet receiving the explorer.
    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (
            Some(self.state.dst_planet_struct.planet_id),
            Some(self.state.explorer_struct.explorer_id),
        )
    }

    /// This is an action state; it does not poll for incoming messages because it
    /// is actively dispatching a request to a planet.
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        None
    }

    /// ### Transition Function: Initiating the Acquisition
    ///
    /// This function performs the critical handshake setup by resolving communication
    /// channels and dispatching the acquisition request.
    ///
    /// #### 1. Channel Resolution
    /// The Orchestrator attempts to retrieve the `Sender<PlanetToExplorer>` for the explorer.
    /// * **Success**: If the sender is found in the registry, the Orchestrator wraps it in
    ///   an `IncomingExplorerRequest` and sends it to the destination planet.
    /// * **Failure**: If the explorer's channel is missing (indicating a lifecycle error),
    ///   it transitions to an [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`].
    ///
    /// #### 2. Handshake Dispatch
    /// * **Success Path**: On a successful message delivery to the destination planet, the
    ///   conversation advances to [`WaitingIncomingResponse`].
    /// * **Communication Errors**: If the planet sender is missing or the channel is
    ///   closed, it transitions to [`ErrorState`] with either [`PlanetSenderNotFound`]
    ///   or [`IncomingMessageFailed`].
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        if let Some(sender) = self.get_new_explorer_sender() {
            // Try to initiate the handshake with the destination planet
            return match self.state.dst_planet_struct.to_planet(
                OrchestratorToPlanet::IncomingExplorerRequest {
                    explorer_id: self.state.explorer_struct.explorer_id,
                    new_sender: sender,
                },
            ) {
                Ok(()) => {
                    let state_struct = WaitingIncomingResponse {
                        curr_planet_struct: self.state.curr_planet_struct,
                        explorer_struct: self.state.explorer_struct,
                        dst_planet_id: self.state.dst_planet_struct.planet_id,
                        planet_explorer_channels: self.state.planet_explorer_channels,
                        explorers_location_ref: self.state.explorers_location_ref,
                        handle_outgoing: self.state.handle_outgoing,
                    };
                    let new_state = MoveToPlanetConversation::<WaitingIncomingResponse>::new(
                        self.id,
                        state_struct,
                    );
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
                    Some(Box::new(error_state)
                        as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
                }
            };
        }

        let error_state = ErrorState::new(
            Box::new(CommonErrorTypes::ExplorerSenderNotFound(
                self.state.explorer_struct.explorer_id,
            )),
            self.id,
        );
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl MoveToPlanetConversation<SendIncomingRequest> {
    pub(crate) fn new(conv_id: ID, state: SendIncomingRequest) -> Self {
        Self {
            id: conv_id,
            expected_message: None,
            state,
        }
    }

    fn get_new_explorer_sender(&self) -> Option<Sender<PlanetToExplorer>> {
        self.state
            .planet_explorer_channels
            .planet_to_explorer_senders
            .lock()
            .unwrap()
            .get(&self.state.explorer_struct.explorer_id)
            .cloned()
    }
}

///**Move To Planet Conversation - Waiting Incoming Response**
///
/// This state represents the primary gatekeeping phase of movement. The Orchestrator
/// remains in this state until the destination planet responds to the acquisition request.
// WAITING INCOMING RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBagContent> for MoveToPlanetConversation<WaitingIncomingResponse> {
    /// Returns the unique ID of the conversation instance.
    fn get_id(&self) -> ID {
        self.id
    }

    /// Returns the IDs of the current planet and the explorer whose movement is being validated.
    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (
            Some(self.state.dst_planet_id),
            Some(self.state.explorer_struct.explorer_id),
        )
    }

    /// Returns the expected message kind: [`PlanetToOrchestratorKind::IncomingExplorerResponse`].
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// ### Transition Function: Processing Acquisition Results
    ///
    /// This function evaluates whether the destination planet has accepted the entity and
    /// determines if the handshake needs to proceed to a source-planet release phase.
    ///
    /// #### 1. Destination Acceptance (`res.is_ok()`)
    /// If the planet accepts the explorer, the transition logic branches based on the
    /// `handle_outgoing` flag:
    /// * **Flag True (Standard Move)**: The explorer is currently on a planet. The Orchestrator
    ///   must now command that planet to release the entity.
    ///   To do so, it transitions to [`SendOutgoingRequest`].
    /// * **Flag False (Spawn/Forced)**: The explorer does not have a current planet (or is
    ///   being moved externally). It skips the source release and transitions directly
    ///   to [`SendMoveRequest`] to notify the explorer of the success.
    ///
    /// #### 2. Destination Rejection (`res.is_err()`)
    /// If the planet refuses (e.g., due to internal logic or population limits), the
    /// move is aborted. Transitions to [`ErrorState`] with [`MoveToPlanetErrors::DstPlanetFailed`].
    ///
    /// #### 3. Error Handling
    /// * **Dispatch Failure**: If the release request to the current planet fails, it
    ///   transitions to an error state.
    /// * **Protocol Violation**: If a message other than the acquisition response is
    ///   received, transitions to [`CommonErrorTypes::WrongMessage`].
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        if let Some(PossibleMessage::PlanetToOrch(
            PlanetToOrchestrator::IncomingExplorerResponse {
                planet_id,
                explorer_id,
                res,
            },
        )) = msg_wrapped
        {
            return if res.is_ok() {
                //Explorer comes from another planet, transition to SendOutgoingRequest
                if self.state.handle_outgoing {
                    let state_struct = SendOutgoingRequest::new(
                          self.state.curr_planet_struct,
                          self.state.explorer_struct,
                          self.state.planet_explorer_channels,
                          self.state.dst_planet_id,
                          self.state.explorers_location_ref,
                    );
                    let next_state = MoveToPlanetConversation::<SendOutgoingRequest>::new(
                        self.id,
                        state_struct,
                    );
                    Some(Box::new(next_state))

                } else {
                    let state = SendMoveRequest::new(
                        self.state.explorers_location_ref,
                        self.state.dst_planet_id,
                        self.state.explorer_struct,
                        self.state.planet_explorer_channels,
                        true,
                    );
                    let next_state =
                        MoveToPlanetConversation::<SendMoveRequest>::new(self.id, state);
                    Some(Box::new(next_state))
                }
            } else {
                let error_state = ErrorState::new(
                    Box::new(MoveToPlanetErrors::DstPlanetFailed {
                        planet_id,
                        explorer_id,
                    }),
                    self.id,
                );
                Some(Box::new(error_state)
                    as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
            };
        }
        //Wrong message arrived
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl MoveToPlanetConversation<WaitingIncomingResponse> {
    pub(crate) fn new(id: ID, state: WaitingIncomingResponse) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::PlanetToOrchKind(
                PlanetToOrchestratorKind::IncomingExplorerResponse,
            )),
            state,
        }
    }
}
