use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::{
    MoveToPlanetConversation, SendIncomingRequest, SendManualMoveRequest,
};
use crate::orchestrator::conversations::{Conversation, PossibleExpectedKinds, PossibleMessage};
use common_explorer::ExplorerBagContent;
use common_game::utils::ID;

///**Move To Planet Conversation - Send Manual Move Request**
///
/// This state handles movements triggered manually (e.g., by administrative commands or
/// specific game logic) rather than an explorer's own request. It serves as an
/// initialization point for forced transitions.
// SEND MANUAL MOVE REQUEST IMPLEMENTATION
impl Conversation<ExplorerBagContent> for MoveToPlanetConversation<SendManualMoveRequest> {
    /// Returns the unique ID of the conversation instance.
    fn get_id(&self) -> ID {
        self.id
    }

    /// Returns the IDs of the destination planet and the explorer being manually moved.
    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (
            Some(self.state.dst_planet_struct.planet_id),
            Some(self.state.explorer_struct.explorer_id),
        )
    }

    /// This is an action state (fire-and-forget); it does not wait for an external message.
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// ### Transition Function: Initiating Manual Handover
    ///
    /// This function prepares the standard movement handshake by determining the current
    /// physical status of the explorer.
    ///
    /// #### 1. Location Verification
    /// It checks the `explorers_location_ref` to see if the explorer is currently assigned
    /// to a planet.
    /// * **If in a planet**: The `handle_outgoing` flag is set to `true`, ensuring the
    ///   Orchestrator will eventually ask the current planet to release the explorer.
    /// * **If not in a planet**: The explorer is likely being "spawned" or moved from
    ///   limbo. `handle_outgoing` is set to `false`, skipping the source-planet release phase.
    ///
    /// #### 2. Handshake Initiation
    /// The conversation transitions directly to [`SendIncomingRequest`], which begins
    /// the process of notifying the destination planet of the entity's arrival.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        //check if explorer is currently in a planet (in case we need to notify the planet to release him)
        let handle_outgoing = self.state.curr_planet_struct.is_some();

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

    /// **Priority 5**: Manual moves are treated with high priority to ensure world state
    /// overrides are processed immediately.
    fn get_priority(&self) -> i32 {
        5
    }
}

impl MoveToPlanetConversation<SendManualMoveRequest> {
    /// Checks the global registry to determine if the explorer is currently managed by a planet.
    fn explorer_is_in_planets(&self) -> bool {
        self.state
            .explorers_location_ref
            .lock()
            .unwrap()
            .get(&self.state.explorer_struct.explorer_id)
            .is_some()
    }

    /// Internal constructor for the [`SendManualMoveRequest`] state.
    pub(crate) fn new(id: ID, state: SendManualMoveRequest) -> Self {
        Self {
            id,
            state,
            expected_message: None,
        }
    }
}
