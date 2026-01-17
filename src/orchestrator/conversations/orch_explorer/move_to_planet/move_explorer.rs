use crate::logging_utils::log_internal;
use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, SendMoveRequest, WaitMoveToPlanetResponse,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError,
};
use crate::payload;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestrator;
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestratorKind::MovedToPlanetResult;
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer::MoveToPlanet;
use common_game::protocols::planet_explorer::ExplorerToPlanet;
use common_game::utils::ID;
use crossbeam_channel::Sender;

impl Conversation<ExplorerBag> for MoveToPlanetConversation<SendMoveRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        None
    }

    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        // Determine the sender
        let sender_to_new_planet = if self.state.is_explorer_moving {
            //Explorer is moving, we need to find the sender to the planet
            if let Some(sender) = self.get_new_planet_sender(self.state.dst_planet_id) { Some(sender) } 
            else {
                let error = Box::new(CommonErrorTypes::ExplorerSenderNotFound(
                    self.state.dst_planet_id,
                ));
                let error_state = ErrorState::new(error, self.id);
                return Some(Box::new(error_state));
            }
        } else {
            None
        };

        // Send Message with correct sender
        let message = MoveToPlanet {
            sender_to_new_planet,
            planet_id: self.state.dst_planet_id,
        };

        match self.state.explorer_struct.to_explorer(message) {
            Ok(()) => {
                let state_struct = WaitMoveToPlanetResponse::new(
                    self.state.explorers_location_ref.clone(), // Ensure this is cloned if needed
                    self.state.is_explorer_moving,
                    self.state.dst_planet_id,
                    self.state.explorer_struct.explorer_id,
                );
                let next_state = MoveToPlanetConversation::<WaitMoveToPlanetResponse>::new(
                    self.id,
                    state_struct,
                );
                Some(Box::new(next_state))
            }
            Err(err) => {
                let error: Box<dyn ErrorType + Send + Sync> = match err {
                    ToExplorerError::SendingMessageFailure(id) => {
                        Box::new(CommonErrorTypes::MessageToExplorerFailed(id))
                    }
                    ToExplorerError::SenderNotFound(id) => {
                        Box::new(CommonErrorTypes::ExplorerSenderNotFound(id))
                    }
                };
                let error_state = ErrorState::new(error, self.id);
                Some(Box::new(error_state))
            }
        }
    }

    fn get_priority(&self) -> i32 {
        5
    }
}

impl MoveToPlanetConversation<SendMoveRequest> {
    fn get_new_planet_sender(&self, planet_id: ID) -> Option<Sender<ExplorerToPlanet>> {
        self.state
            .planet_explorer_channels
            .explorer_to_planet_senders
            .lock()
            .unwrap()
            .get(&planet_id)
            .cloned()
    }

    pub(crate) fn new(conv_id: ID, state: SendMoveRequest) -> Self {
        Self {
            id: conv_id,
            state,
            expected_message: None,
        }
    }
}

///**Move To Planet Conversation - Wait Move To Planet Response**
///
/// This is the final state in the explorer movement sequence. After both the destination
/// and source planets have synchronized their communication channels, the Orchestrator
/// waits for the Explorer itself to confirm it has successfully transitioned.
///
/// Upon a successful [`ExplorerToOrchestrator::MovedToPlanetResult`], the Orchestrator
/// updates the global explorer location list. If the explorer was flagged as unable to
/// move (e.g., non-neighbor destination), the conversation closes gracefully without
/// updating the location.
// WAIT MOVE TO PLANET RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBag> for MoveToPlanetConversation<WaitMoveToPlanetResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitMoveToPlanetResponse`] state:
    ///
    /// Returns:
    ///
    /// * [None] - If the explorer moved correctly and the internal location list was updated.
    /// * [None] - If the explorer acknowledges the command but movement was invalid
    ///   (e.g., destination was not a neighbor).
    /// * [`ErrorState`] with [`MoveToPlanetErrors::ExplorerLocationNotFound`] - If the
    ///   internal location list does not contain the explorer.
    /// * [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] - If an unexpected message is received.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::MovedToPlanetResult {
                explorer_id,
                planet_id,
            },
        )) = msg_wrapped
        {
            //Explorer is moving, need to change its location in Orchestrator reference
            if self.state.is_explorer_moving {
                log_internal(
                    Channel::Info,
                    payload!(
                        action : "Explorer correctly moved to Planet",
                        explorer_id : explorer_id,
                        destination_planet_id : planet_id,
                        conversation_id : self.id,
                    ),
                );

                return match self.move_explorer_location(explorer_id, planet_id) {
                    Ok(()) => {
                        log_internal(
                            Channel::Debug,
                            payload!(
                                action : "Changed Explorer location in List, closing conversation",
                                explorer_id : explorer_id,
                                changed_to_planet_id : planet_id,
                                conversation_id : self.id
                            ),
                        );
                        None
                    }
                    Err(e) => {
                        let err_struct = ErrorState::new(Box::new(e), self.id);
                        Some(Box::new(err_struct))
                    }
                };
            }
            //Explorer responded correctly and couldn't move
            log_internal(
                Channel::Warning,
                payload!(
                    action : "Explorer cannot move due to destination Planet not being a neighbor of current Planet, closing conversation",
                    explorer_id : explorer_id,
                    destination_planet_id : planet_id,
                    conversation_id : self.id
                ),
            );
            return None;
        }

        // Wrong message, closing Conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl MoveToPlanetConversation<WaitMoveToPlanetResponse> {
    /// The constructor for [`MoveToPlanetConversation`] in the [`WaitMoveToPlanetResponse`] state.
    ///
    /// Sets the expected message kind to [`ExplorerToOrchestratorKind::MovedToPlanetResult`].
    pub(crate) fn new(id: ID, state: WaitMoveToPlanetResponse) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::ExplorerToOrchKind(
                MovedToPlanetResult,
            )),
            state,
        }
    }

    /// Internal helper to update the thread-safe global list of explorer locations.
    ///
    /// Returns [`Err(MoveToPlanetErrors::ExplorerLocationNotFound)`] if the explorer ID is missing.
    fn move_explorer_location(
        &self,
        explorer_id: ID,
        dst_planet_id: ID,
    ) -> Result<(), MoveToPlanetErrors> {
        if let Some(location) = self
            .state
            .explorers_location_ref
            .lock()
            .unwrap()
            .get_mut(&explorer_id)
        {
            *location = dst_planet_id;
            return Ok(());
        }

        Err(MoveToPlanetErrors::ExplorerLocationNotFound(explorer_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::ExplorersLocationRef;
    use crate::orchestrator::conversations::PossibleMessage::ExplorerToOrch;
    use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use common_game::protocols::orchestrator_planet::PlanetToOrchestrator;
    use crate::orchestrator::conversations::orch_explorer::move_to_planet::WaitingOutgoingResponse;

    const CONV_ID: ID = 1;
    const EXPLORER_ID: ID = 2;
    const DST_PLANET_ID: ID = 50;

    // --------------- HELPER FUNCTIONS ------------
    fn create_mock_state(
        explorer_id: ID,
        explorers_location_ref: ExplorersLocationRef,
        dst_planet_id: ID,
        is_moving: bool,
    ) -> WaitMoveToPlanetResponse {
        WaitMoveToPlanetResponse::new(
            explorers_location_ref,
            is_moving,
            dst_planet_id,
            explorer_id,
        )
    }

    // --------------------- TEST ---------------
    #[test]
    fn test_transition_success() {
        let exp_locations = Arc::new(Mutex::new(HashMap::from([(EXPLORER_ID, 5)])));
        let state = create_mock_state(EXPLORER_ID, exp_locations, DST_PLANET_ID, true);
        let conv = Box::new(MoveToPlanetConversation::<WaitMoveToPlanetResponse>::new(
            CONV_ID, state,
        ));

        // Simulate positive response from Explorer
        let msg = ExplorerToOrch(ExplorerToOrchestrator::MovedToPlanetResult {
            explorer_id: EXPLORER_ID,
            planet_id: DST_PLANET_ID,
        });

        let next_state = conv.transition(Some(msg));

        assert!(next_state.is_none());
    }

    #[test]
    fn test_transition_destination_rejection() {
        let (tx, _rx) = unbounded::<OrchestratorToExplorer>();
        let explorers_senders = Arc::new(Mutex::new(HashMap::from([(EXPLORER_ID, tx)])));
        let state = create_mock_state(EXPLORER_ID, explorers_senders, DST_PLANET_ID);

        let conv = Box::new(MoveToPlanetConversation::<WaitingOutgoingResponse>::new(
            CONV_ID, state,
        ));

        // Simulate negative response from Destination Planet
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::OutgoingExplorerResponse {
            planet_id: CURR_PLANET_ID,
            explorer_id: EXPLORER_ID,
            res: Err("Didn't manage outgoing explorer".to_string()),
        });

        let next_state = conv
            .transition(Some(msg))
            .expect("Should transition to error state");

        assert_eq!(next_state.get_expected_kind(), None);
        assert_eq!(
            next_state.get_error_details(),
            Some(format!(
                "Current planet {CURR_PLANET_ID} failed to let go of outgoing explorer {EXPLORER_ID}"
            )),
        );
    }

    #[test]
    fn test_transition_wrong_message() {
        let (tx, _rx) = unbounded::<OrchestratorToExplorer>();
        let explorers_senders = Arc::new(Mutex::new(HashMap::from([(EXPLORER_ID, tx)])));
        let state = create_mock_state(EXPLORER_ID, explorers_senders, DST_PLANET_ID);

        let conv = Box::new(MoveToPlanetConversation::<WaitingOutgoingResponse>::new(
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
