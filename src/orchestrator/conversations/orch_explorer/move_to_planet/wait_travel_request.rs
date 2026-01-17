use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::WaitingTravelRequest;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, SendIncomingRequest, SendMoveRequest,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind,
};
use common_game::utils::ID;

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

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::TravelToPlanetRequest {
                explorer_id: _explorer_id,
                dst_planet_id: _dst_planet_id,
                current_planet_id: _current_planet_id,
            },
        )) = msg_wrapped
        {
            if self.check_neighbors() {
                let next_state = SendIncomingRequest::new(
                    self.state.curr_planet_struct,
                    self.state.explorer_struct,
                    self.state.dst_planet_struct,
                    self.state.planet_explorer_channels,
                    self.state.explorers_location_ref,
                    true,
                );
                let next_conv =
                    MoveToPlanetConversation::<SendIncomingRequest>::new(self.id, next_state);
                return Some(Box::new(next_conv));
            }

            // Non-neighbors logic
            let next_state = SendMoveRequest::new(
                self.state.explorers_location_ref,
                self.state.dst_planet_struct.planet_id,
                self.state.explorer_struct,
                self.state.planet_explorer_channels,
                false,
            );
            let next_conv = MoveToPlanetConversation::<SendMoveRequest>::new(self.id, next_state);

            // Added 'return' so it doesn't hit the ErrorState below
            return Some(Box::new(next_conv));
        }

        // If msg_wrapped was None or didn't match the pattern
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
            return curr_planet_ref.has_neighbor(&dst_planet_ref.inner);
        }
        false
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy_setup::PlanetMap;
    use crate::orchestrator::PlanetExplorerChannels;
    use crate::orchestrator::conversations::SendersToPlanet;
    use crate::orchestrator::conversations::orch_explorer::test_utils::{
        make_empty_senders, make_to_explorer_struct, make_to_planet_struct,
    };
    use common_game::protocols::orchestrator_explorer::ExplorerToOrchestratorKind;
    use common_game::protocols::orchestrator_planet::OrchestratorToPlanet;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: ID = 1;
    const EXPLORER_ID: ID = 2;
    // These match the IDs in test_galaxy.txt
    const CURR_PLANET_ID: ID = 10_100_001;
    const DST_PLANET_ID: ID = 10_100_002;
    const OTHER_PLANET_ID: ID = 10_100_003;
    const NON_NEIGHBOR_ID: ID = 99_999_999;

    // --- Helper functions ---

    fn make_planet_senders_with(planet_id: ID) -> SendersToPlanet {
        let (tx, _rx) = unbounded::<OrchestratorToPlanet>();
        Arc::new(Mutex::new(HashMap::from([(planet_id, tx)])))
    }

    fn load_galaxy() -> PlanetMap {
        // Use the same loader as neighbors_discovery.rs for consistency
        let (galaxy, _planets_receiver, _orch_to_plan_senders, _expl_to_plan_senders) =
            crate::galaxy_setup::galaxy_loader(std::path::Path::new("test_galaxy.txt"));
        galaxy
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_waiting_travel_conv() -> Box<MoveToPlanetConversation<WaitingTravelRequest>> {
        let galaxy = load_galaxy();
        let curr_planet_senders = make_planet_senders_with(CURR_PLANET_ID);
        let dst_planet_senders = make_planet_senders_with(DST_PLANET_ID);

        let state = WaitingTravelRequest {
            galaxy,
            planet_explorer_channels: PlanetExplorerChannels::new(),
            curr_planet_struct: make_to_planet_struct(CURR_PLANET_ID, curr_planet_senders),
            dst_planet_struct: make_to_planet_struct(DST_PLANET_ID, dst_planet_senders),
            explorer_struct: make_to_explorer_struct(EXPLORER_ID, make_empty_senders()),
            explorers_location_ref: Arc::new(Mutex::new(HashMap::new())),
        };
        Box::new(MoveToPlanetConversation::<WaitingTravelRequest>::new(
            CONV_ID, state,
        ))
    }

    // --- Tests ---

    #[test]
    fn waiting_travel_neighbors_success() {
        let conv = make_waiting_travel_conv();
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::TravelToPlanetRequest {
            explorer_id: EXPLORER_ID,
            dst_planet_id: DST_PLANET_ID,
            current_planet_id: CURR_PLANET_ID,
        });
        let next_conv = conv
            .transition(Some(msg))
            .expect("Should transition to next state");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_error_details(), None);
        assert!(next_conv.get_expected_kind().is_none());
    }

    #[test]
    fn waiting_travel_non_neighbors() {
        let conv = make_waiting_travel_conv();
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::TravelToPlanetRequest {
            explorer_id: EXPLORER_ID,
            dst_planet_id: NON_NEIGHBOR_ID,
            current_planet_id: CURR_PLANET_ID,
        });
        let next_conv = conv
            .transition(Some(msg))
            .expect("Should transition to SendMoveRequest");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_error_details(), None);
        assert!(next_conv.get_expected_kind().is_none());
    }

    #[test]
    fn waiting_travel_wrong_message() {
        let conv = make_waiting_travel_conv();

        let wrong_msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::StartExplorerAIResult {
                explorer_id: EXPLORER_ID,
            });

        let next_conv = conv
            .transition(Some(wrong_msg))
            .expect("Should return an ErrorState");

        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(
            next_conv.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }

    #[test]
    fn waiting_travel_getters() {
        let galaxy = load_galaxy();
        let curr_planet_senders = make_planet_senders_with(CURR_PLANET_ID);
        let dst_planet_senders = make_planet_senders_with(DST_PLANET_ID);

        let state = WaitingTravelRequest {
            galaxy,
            planet_explorer_channels: PlanetExplorerChannels::new(),
            curr_planet_struct: make_to_planet_struct(CURR_PLANET_ID, curr_planet_senders),
            dst_planet_struct: make_to_planet_struct(DST_PLANET_ID, dst_planet_senders),
            explorer_struct: make_to_explorer_struct(EXPLORER_ID, make_empty_senders()),
            explorers_location_ref: Arc::new(Mutex::new(HashMap::new())),
        };
        let conv = MoveToPlanetConversation::<WaitingTravelRequest>::new(CONV_ID, state);

        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entity_id(), EXPLORER_ID);
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::TravelToPlanetRequest
            ))
        );
        assert_eq!(conv.get_priority(), 4);
    }
}
