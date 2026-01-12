use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::MoveToPlanetConversation;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    WaitMoveToPlanetResponse, WaitingIncomingResponse, WaitingTravelRequest,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToPlanetError,
};
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer::MoveToPlanet;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind,
};
use common_game::protocols::orchestrator_planet::OrchestratorToPlanet;
use common_game::protocols::planet_explorer::PlanetToExplorer;
use common_game::utils::ID;
use crossbeam_channel::Sender;

///**Move To Planet Conversation - Waiting Travel Request**
///
/// This is the starting state of the movement lifecycle. It listens for a
/// [`ExplorerToOrchestrator::TravelToPlanetRequest`] from an explorer.
///
/// **Logic Flow:**
/// 1. Verifies if the destination planet is a neighbor of the current planet via the Galaxy Map.
/// 2. If valid, sends an [`IncomingExplorerRequest`] to the destination planet and transitions
///    to [`WaitingIncomingResponse`].
/// 3. If invalid (not neighbors), it informs the explorer movement is impossible and transitions
///    directly to [`WaitMoveToPlanetResponse`] to gracefully close the attempt.

// WAITING TRAVEL REQUEST IMPLEMENTATION
impl Conversation<ExplorerBag> for MoveToPlanetConversation<WaitingTravelRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingTravelRequest`] state:
    ///
    /// Returns:
    ///
    /// * [`MoveToPlanetConversation<WaitingIncomingResponse>`] if the neighbor check passes
    ///   and the destination planet is successfully notified.
    ///
    /// * [`MoveToPlanetConversation<WaitMoveToPlanetResponse>`] if the neighbor check fails;
    ///   informs the explorer that movement is denied.
    ///
    /// * [`ErrorState`] if the destination planet sender is missing or if communication
    ///   with the explorer fails during a denial.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::TravelToPlanetRequest {
                explorer_id,
                current_planet_id: _current_planet_id,
                dst_planet_id: _dst_planet_id,
            },
        )) = msg_wrapped
        {
            // 1. Verify spatial validity (Neighborhood check)
            if self.check_neighbors() {
                // Try to find the new explorer sender and send it to the dst planet
                if let Some(sender) = self.get_new_explorer_sender(explorer_id) {
                    // Try to initiate the handshake with the destination planet
                    return match self.state.dst_planet_struct.to_planet(
                        OrchestratorToPlanet::IncomingExplorerRequest {
                            explorer_id,
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
                            };
                            let new_state =
                                MoveToPlanetConversation::<WaitingIncomingResponse>::new(
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
                // The sender to explorer is not found
                let error_state = ErrorState::new(
                    Box::new(CommonErrorTypes::ExplorerSenderNotFound(explorer_id)),
                    self.id,
                );
                return Some(Box::new(error_state));
            }

            // 2. Deny movement (Destination is not a neighbor)
            return match self.state.explorer_struct.to_explorer(MoveToPlanet {
                sender_to_new_planet: None,
                planet_id: self.state.dst_planet_struct.planet_id,
            }) {
                Ok(()) => {
                    let explorer_id = self.state.explorer_struct.explorer_id;
                    let state_struct = WaitMoveToPlanetResponse::new(
                        self.state.explorers_location_ref,
                        false,
                        self.state.dst_planet_struct.planet_id,
                        explorer_id,
                    );
                    let next_state = MoveToPlanetConversation::<WaitMoveToPlanetResponse>::new(
                        self.id,
                        state_struct,
                    );
                    Some(Box::new(next_state))
                }
                Err(err) => {
                    let error: Box<dyn ErrorType + Send + Sync> = match err {
                        ToExplorerError::SenderNotFound(id) => {
                            Box::new(CommonErrorTypes::ExplorerSenderNotFound(id))
                        }
                        ToExplorerError::SendingMessageFailure(id) => {
                            Box::new(MoveToPlanetErrors::IncomingMessageFailed(id))
                        }
                    };
                    let error_state = ErrorState::new(error, self.id);
                    Some(Box::new(error_state))
                }
            };
        }
        // Wrong Message type received for this state
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl MoveToPlanetConversation<WaitingTravelRequest> {
    /// Checks the Galaxy Map to see if the target destination is reachable from the current location.
    fn check_neighbors(&self) -> bool {
        let galaxy = self.state.galaxy.lock().unwrap();
        if let (Some(curr_planet_ref), Some(dst_planet_ref)) = (
            galaxy.get(&self.state.curr_planet_struct.planet_id),
            galaxy.get(&self.state.dst_planet_struct.planet_id),
        ) {
            return curr_planet_ref.lock().unwrap().has_neighbor(dst_planet_ref);
        }
        false
    }

    /// Resolves the specific communication channel for the explorer.
    fn get_new_explorer_sender(&self, explorer_id: ID) -> Option<Sender<PlanetToExplorer>> {
        self.state
            .planet_explorer_channels
            .planet_to_explorer_senders
            .lock()
            .unwrap()
            .get(&explorer_id)
            .cloned()
    }

    /// Internal constructor for the initial state.
    fn new(id: ID, state: WaitingTravelRequest) -> Self {
        Self {
            id,
            state,
            expected_message: Some(ExplorerToOrchKind(
                ExplorerToOrchestratorKind::TravelToPlanetRequest,
            )),
        }
    }
}
