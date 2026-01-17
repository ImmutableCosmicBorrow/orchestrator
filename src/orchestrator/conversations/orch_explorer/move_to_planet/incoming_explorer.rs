use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, SendIncomingRequest, SendMoveRequest,
    WaitingIncomingResponse, WaitingOutgoingResponse,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::protocols::planet_explorer::PlanetToExplorer;
use common_game::utils::ID;
use crossbeam_channel::Sender;

impl Conversation<ExplorerBag> for MoveToPlanetConversation<SendIncomingRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.dst_planet_struct.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        None
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
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
                    Some(Box::new(error_state))
                }
            };
        }
        // The sender to the incoming explorer is not found!
        let error_state = ErrorState::new(
            Box::new(CommonErrorTypes::ExplorerSenderNotFound(
                self.state.explorer_struct.explorer_id,
            )),
            self.id,
        );
        Some(Box::new(error_state))
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
            return if res.is_ok() {
                if self.state.handle_outgoing {
                    match self
                        .state
                        .curr_planet_struct
                        .to_planet(OrchestratorToPlanet::OutgoingExplorerRequest { explorer_id })
                    {
                        Ok(()) => {
                            //TODO: Send to SendOutgoing
                            let state_struct = WaitingOutgoingResponse::new(
                                self.state.explorer_struct,
                                self.state.planet_explorer_channels,
                                self.state.dst_planet_id,
                                self.state.explorers_location_ref,
                            );
                            let next_state =
                                MoveToPlanetConversation::<WaitingOutgoingResponse>::new(
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
                } else {
                    //No need to do Outgoing, explorer had no current planet, moving to SendMoveRequest
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
            }
            //Dst Planet failed to acquire new explorer, moving to error state
            else {
                let error_state = ErrorState::new(
                    Box::new(MoveToPlanetErrors::DstPlanetFailed {
                        planet_id,
                        explorer_id,
                    }),
                    self.id,
                );
                Some(Box::new(error_state))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::PlanetExplorerChannels;
    use crate::orchestrator::conversations::orch_explorer::move_to_planet::WaitingIncomingResponse;
    use crate::orchestrator::conversations::{SendersToPlanet, ToExplorerStruct, ToPlanetStruct};
    use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::OutgoingExplorerResponse;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: ID = 1;
    const EXPLORER_ID: ID = 2;
    const DST_PLANET_ID: ID = 3;
    // Helper to create a mock WaitingIncomingResponse state
    fn create_mock_state(
        explorer_id: ID,
        curr_planet_id: ID,
        planet_senders: SendersToPlanet,
    ) -> WaitingIncomingResponse {
        WaitingIncomingResponse {
            explorer_struct: ToExplorerStruct {
                explorer_id,
                explorers_senders: Arc::new(Mutex::new(HashMap::new())),
            },
            curr_planet_struct: ToPlanetStruct {
                planet_id: curr_planet_id,
                planets_senders: planet_senders,
            },
            planet_explorer_channels: PlanetExplorerChannels::new(),
            dst_planet_id: 50,
            explorers_location_ref: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[test]
    fn test_transition_success() {
        let (tx, _rx) = unbounded::<OrchestratorToPlanet>();
        let planet_senders = Arc::new(Mutex::new(HashMap::from([(DST_PLANET_ID, tx)])));
        let state = create_mock_state(EXPLORER_ID, DST_PLANET_ID, planet_senders);

        let conv = Box::new(MoveToPlanetConversation::<WaitingIncomingResponse>::new(
            CONV_ID, state,
        ));

        // Simulate positive response from Destination Planet
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::IncomingExplorerResponse {
            planet_id: DST_PLANET_ID,
            explorer_id: EXPLORER_ID,
            res: Ok(()),
        });

        let next_state = conv
            .transition(Some(msg))
            .expect("Should transition to next state");

        assert_eq!(next_state.get_id(), CONV_ID);
        assert_eq!(next_state.get_error_details(), None);
        assert_eq!(
            next_state.get_expected_kind(),
            Some(PossibleExpectedKinds::PlanetToOrchKind(
                OutgoingExplorerResponse
            ))
        );
        assert_eq!(next_state.get_priority(), 4);
    }

    #[test]
    fn test_transition_destination_rejection() {
        let (tx, _rx) = unbounded::<OrchestratorToPlanet>();
        let planet_senders = Arc::new(Mutex::new(HashMap::from([(DST_PLANET_ID, tx)])));
        let state = create_mock_state(EXPLORER_ID, DST_PLANET_ID, planet_senders);

        let conv = Box::new(MoveToPlanetConversation::<WaitingIncomingResponse>::new(
            CONV_ID, state,
        ));

        // Simulate rejection from Destination Planet
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::IncomingExplorerResponse {
            planet_id: DST_PLANET_ID,
            explorer_id: EXPLORER_ID,
            res: Err("Planet Full".to_string()),
        });

        let next_state = conv
            .transition(Some(msg))
            .expect("Should transition to error state");

        assert_eq!(next_state.get_expected_kind(), None);
        assert_eq!(
            next_state.get_error_details(),
            Some(format!(
                "Destination planet {DST_PLANET_ID} failed to acquire incoming explorer {EXPLORER_ID}"
            ))
        );
    }

    #[test]
    fn test_transition_wrong_message() {
        let (tx, _rx) = unbounded::<OrchestratorToPlanet>();
        let planet_senders = Arc::new(Mutex::new(HashMap::from([(DST_PLANET_ID, tx)])));
        let state = create_mock_state(EXPLORER_ID, DST_PLANET_ID, planet_senders);

        let conv = Box::new(MoveToPlanetConversation::<WaitingIncomingResponse>::new(
            CONV_ID, state,
        ));

        // Send a message that isn't IncomingExplorerResponse
        let msg =
            PossibleMessage::PlanetToOrch(PlanetToOrchestrator::KillPlanetResult { planet_id: 5 });

        let next_state = conv
            .transition(Some(msg))
            .expect("Should transition to error state");

        assert_eq!(next_state.get_expected_kind(), None);
        assert_eq!(
            next_state.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }
}
