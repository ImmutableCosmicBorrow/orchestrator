use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, SendIncomingRequest, SendManualMoveRequest,
};
use crate::orchestrator::conversations::{Conversation, PossibleExpectedKinds, PossibleMessage};
use common_game::utils::ID;

impl Conversation<ExplorerBag> for MoveToPlanetConversation<SendManualMoveRequest> {
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
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        let handle_outgoing = self.explorer_is_in_planets();
        let state_struct = SendIncomingRequest::new(
            self.state.curr_planet_struct,
            self.state.explorer_struct,
            self.state.dst_planet_struct,
            self.state.planet_explorer_channels,
            self.state.explorers_location_ref,
            handle_outgoing,
        );
        let next_conv = MoveToPlanetConversation::<SendIncomingRequest>::new(self.id, state_struct);
        Some(Box::new(next_conv))
    }

    fn get_priority(&self) -> i32 {
        5
    }
}

impl MoveToPlanetConversation<SendManualMoveRequest> {
    fn explorer_is_in_planets(&self) -> bool {
        self.state
            .explorers_location_ref
            .lock()
            .unwrap()
            .get(&self.state.explorer_struct.explorer_id)
            .is_some()
    }

    pub(crate) fn new(id: ID, state: SendManualMoveRequest) -> Self {
        Self {
            id,
            state,
            expected_message: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::PlanetExplorerChannels;
    use crate::orchestrator::conversations::SendersToPlanet;
    use crate::orchestrator::conversations::orch_explorer::move_to_planet::SendManualMoveRequest;
    use crate::orchestrator::conversations::orch_explorer::test_utils::{
        make_empty_senders, make_to_explorer_struct, make_to_planet_struct,
    };
    use common_game::protocols::orchestrator_planet::OrchestratorToPlanet;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: ID = 1;
    const EXPLORER_ID: ID = 2;
    const DST_PLANET_ID: ID = 3;
    const CURR_PLANET_ID: ID = 4;

    // --- Helper functions ---

    fn make_planet_senders_with(planet_id: ID) -> SendersToPlanet {
        let (tx, _rx) = unbounded::<OrchestratorToPlanet>();
        Arc::new(Mutex::new(HashMap::from([(planet_id, tx)])))
    }

    fn make_explorers_location_with(
        explorer_id: ID,
        planet_id: ID,
    ) -> crate::orchestrator::ExplorersLocationRef {
        Arc::new(Mutex::new(HashMap::from([(explorer_id, planet_id)])))
    }

    fn make_empty_explorers_location() -> crate::orchestrator::ExplorersLocationRef {
        Arc::new(Mutex::new(HashMap::new()))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_manual_move_conv(
        explorer_in_planets: bool,
    ) -> Box<MoveToPlanetConversation<SendManualMoveRequest>> {
        let curr_planet_senders = make_planet_senders_with(CURR_PLANET_ID);
        let dst_planet_senders = make_planet_senders_with(DST_PLANET_ID);
        let explorers_location_ref = if explorer_in_planets {
            make_explorers_location_with(EXPLORER_ID, CURR_PLANET_ID)
        } else {
            make_empty_explorers_location()
        };

        let state = SendManualMoveRequest::new(
            explorers_location_ref,
            make_to_planet_struct(CURR_PLANET_ID, curr_planet_senders),
            make_to_planet_struct(DST_PLANET_ID, dst_planet_senders),
            make_to_explorer_struct(EXPLORER_ID, make_empty_senders()),
            PlanetExplorerChannels::new(),
        );
        Box::new(MoveToPlanetConversation::<SendManualMoveRequest>::new(
            CONV_ID, state,
        ))
    }

    // --- Tests ---

    #[test]
    fn manual_move_explorer_in_planets() {
        let conv = make_manual_move_conv(true);

        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");

        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_error_details(), None);
        // Next state is SendIncomingRequest which has no expected message
        assert!(next_conv.get_expected_kind().is_none());
    }

    #[test]
    fn manual_move_explorer_not_in_planets() {
        let conv = make_manual_move_conv(false);

        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");

        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_error_details(), None);
        // Next state is SendIncomingRequest which has no expected message
        assert!(next_conv.get_expected_kind().is_none());
    }

    #[test]
    fn manual_move_getters() {
        let curr_planet_senders = make_planet_senders_with(CURR_PLANET_ID);
        let dst_planet_senders = make_planet_senders_with(DST_PLANET_ID);
        let explorers_location_ref = make_explorers_location_with(EXPLORER_ID, CURR_PLANET_ID);

        let state = SendManualMoveRequest::new(
            explorers_location_ref,
            make_to_planet_struct(CURR_PLANET_ID, curr_planet_senders),
            make_to_planet_struct(DST_PLANET_ID, dst_planet_senders),
            make_to_explorer_struct(EXPLORER_ID, make_empty_senders()),
            PlanetExplorerChannels::new(),
        );
        let conv = MoveToPlanetConversation::<SendManualMoveRequest>::new(CONV_ID, state);

        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entity_id(), EXPLORER_ID);
        assert!(conv.get_expected_kind().is_none());
        assert_eq!(conv.get_priority(), 5);
    }
}
