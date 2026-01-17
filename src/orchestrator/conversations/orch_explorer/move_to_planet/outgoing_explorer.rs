use crate::orchestrator::conversations::orch_explorer::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, SendMoveRequest, SendOutgoingRequest,
};
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    WaitMoveToPlanetResponse, WaitingOutgoingResponse,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage
    , ToPlanetError,
};
use crate::orchestrator::ExplorerBag;
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer::MoveToPlanet;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::protocols::planet_explorer::ExplorerToPlanet;
use common_game::utils::ID;

impl Conversation<ExplorerBag> for MoveToPlanetConversation<SendOutgoingRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.curr_planet_struct.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        None
    }

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
                let state_struct = WaitingOutgoingResponse::new(
                    self.state.explorer_struct,
                    self.state.planet_explorer_channels,
                    self.state.dst_planet_id,
                    self.state.explorers_location_ref,
                );
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

    fn get_priority(&self) -> i32 {
        4
    }
}

///**Move To Planet Conversation - Waiting Outgoing Response**
///
/// This state represents the intermediate phase of movement where the destination planet has
/// already acknowledged the incoming explorer, and the Orchestrator is waiting for the current
/// (source) planet to confirm the explorer's release.
///
/// Once the current planet confirms (`OutgoingExplorerResponse`), the Orchestrator retrieves
/// the communication channel for the new planet and sends it to the Explorer via a
/// [`MoveToPlanet`] command, transitioning to the final [`WaitMoveToPlanetResponse`] state.
// WAITING OUTGOING RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBag> for MoveToPlanetConversation<WaitingOutgoingResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingOutgoingResponse`] state:
    ///
    /// Returns:
    ///
    /// * [`MoveToPlanetConversation<WaitMoveToPlanetResponse>`] if the current planet confirms
    ///   the release and the explorer successfully receives the new destination channel.
    ///
    /// * [`ErrorState`] with [`MoveToPlanetErrors::NewSenderToPlanetNotFound`] if the planets
    ///   have swapped channels but the Orchestrator cannot resolve the new destination's sender.
    ///
    /// * [`ErrorState`] with [`MoveToPlanetErrors::DstPlanetFailed`] if the current planet
    ///   fails to let go of the explorer.
    ///
    /// * [`ErrorState`] with explorer communication errors if the [`MoveToPlanet`] command
    ///   fails to send.
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
            //Got both planets acks, moving to SendMoveRequest
            return if res.is_ok() {
                let state = SendMoveRequest::new(
                    self.state.explorers_location_ref,
                    self.state.dst_planet_id,
                    self.state.explorer_struct,
                    self.state.planet_explorer_channels,
                    true,
                );
                let next_conv = MoveToPlanetConversation::<SendMoveRequest>::new(self.id, state);
                Some(Box::new(next_conv))
            }
            //Current planet failed in handling outgoing explorer
            else {
                let error_state = ErrorState::new(
                    Box::new(MoveToPlanetErrors::CurrPlanetFailed {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::{SendersToExplorer, ToExplorerStruct};
    use crate::orchestrator::PlanetExplorerChannels;
    use common_game::protocols::orchestrator_explorer::ExplorerToOrchestratorKind::MovedToPlanetResult;
    use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: ID = 1;
    const EXPLORER_ID: ID = 2;
    const DST_PLANET_ID: ID = 50;
    const CURR_PLANET_ID: ID = 51;

    // --------------- HELPER FUNCTIONS ------------
    fn create_mock_state(
        explorer_id: ID,
        explorers_senders: SendersToExplorer,
        dst_planet_id: ID,
    ) -> WaitingOutgoingResponse {
        let (tx, _) = unbounded::<ExplorerToPlanet>();
        let mut planet_explorer_channels = PlanetExplorerChannels::new();
        planet_explorer_channels.add_expl_to_plan_sender(dst_planet_id, tx);
        WaitingOutgoingResponse {
            explorer_struct: ToExplorerStruct {
                explorers_senders,
                explorer_id,
            },
            planet_explorer_channels,
            dst_planet_id: 50,
            explorers_location_ref: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // --------------------- TEST ---------------
    #[test]
    fn test_transition_success() {
        let (tx, _rx) = unbounded::<OrchestratorToExplorer>();
        let explorers_senders = Arc::new(Mutex::new(HashMap::from([(EXPLORER_ID, tx)])));
        let state = create_mock_state(EXPLORER_ID, explorers_senders, DST_PLANET_ID);

        let conv = Box::new(MoveToPlanetConversation::<WaitingOutgoingResponse>::new(
            CONV_ID, state,
        ));

        // Simulate positive response from Destination Planet
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::OutgoingExplorerResponse {
            planet_id: CURR_PLANET_ID,
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
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                MovedToPlanetResult
            ))
        );
        assert_eq!(next_state.get_priority(), 4);
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
