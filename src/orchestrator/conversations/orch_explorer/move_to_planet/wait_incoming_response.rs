use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, WaitingIncomingResponse, WaitingOutgoingResponse,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;

///**Move To Planet Conversation - Waiting Incoming Response**
///
/// This state represents the first critical waiting phase in an explorer's movement between planets.
/// The Orchestrator has already requested the destination planet to "accept" the incoming explorer.
///
/// If the destination planet accepts (`Ok`), this state transitions the conversation to
/// [`WaitingOutgoingResponse`] after requesting the current planet to "release" the explorer.
/// If the destination planet rejects the explorer, the conversation terminates in an [`ErrorState`].
// WAITING INCOMING RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBag> for MoveToPlanetConversation<WaitingIncomingResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingIncomingResponse`] state:
    ///
    /// Returns:
    ///
    /// * [`MoveToPlanetConversation<WaitingOutgoingResponse>`] if the destination planet accepts
    ///   the explorer and the release request is successfully sent to the current planet.
    ///
    /// * [`ErrorState`] with [`MoveToPlanetErrors::DstPlanetFailed`] if the destination planet
    ///   rejects the acquisition.
    ///
    /// * [`ErrorState`] with [`MoveToPlanetErrors::OutgoingMessageFailed`] if the orchestrator
    ///   cannot communicate with the current planet.
    ///
    /// * [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if an unexpected protocol message is received.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::PlanetToOrch(
            PlanetToOrchestrator::IncomingExplorerResponse {
                planet_id,
                explorer_id,
                res,
            },
        )) = msg_wrapped
        {
            // If the incoming response is positive, tries to send the Outgoing request,
            // otherwise terminates in error state
            return if let Ok(()) = res {
                match self
                    .state
                    .curr_planet_struct
                    .to_planet(OrchestratorToPlanet::OutgoingExplorerRequest { explorer_id })
                {
                    Ok(()) => {
                        let state_struct = WaitingOutgoingResponse::new(
                            self.state.explorer_struct,
                            self.state.planet_explorer_channels,
                            self.state.dst_planet_id,
                            self.state.explorers_location_ref,
                        );
                        let next_state = MoveToPlanetConversation::<WaitingOutgoingResponse>::new(
                            self.id,
                            state_struct,
                        );
                        Some(Box::new(next_state))
                    }
                    Err(err) => {
                        let error: Box<dyn ErrorType + Send + Sync> = match err {
                            ToPlanetError::SendingMessageFailure(id) => {
                                Box::new(MoveToPlanetErrors::OutgoingMessageFailed(id))
                            }
                            ToPlanetError::SenderNotFound(id) => {
                                Box::new(CommonErrorTypes::PlanetSenderNotFound(id))
                            }
                        };
                        let error_state = ErrorState::new(error, self.id);
                        Some(Box::new(error_state))
                    }
                }
            }
            //Dst Planet failed to acquire new explorer
            else {
                let error_state = ErrorState::new(
                    Box::new(MoveToPlanetErrors::DstPlanetFailed {
                        planet_id,
                        explorer_id,
                    }),
                    self.id,
                );
                return Some(Box::new(error_state));
            };
        }
        // Wrong message, closing Conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl MoveToPlanetConversation<WaitingIncomingResponse> {
    /// The constructor for [`MoveToPlanetConversation`] in the [`WaitingIncomingResponse`] state.
    ///
    /// Automatically sets the expected message kind to [`PlanetToOrchestratorKind::IncomingExplorerResponse`].
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
