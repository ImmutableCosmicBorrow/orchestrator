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
}
