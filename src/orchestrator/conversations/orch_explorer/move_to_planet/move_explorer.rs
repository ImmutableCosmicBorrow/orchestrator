use crate::globals::get_explorer_timeout;
use crate::logging_utils::log_internal;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, SendMoveRequest, WaitMoveToPlanetResponse,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError,
};
use crate::payload;
use common_explorer::ExplorerBagContent;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestrator;
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestratorKind::MovedToPlanetResult;
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer::MoveToPlanet;
use common_game::protocols::planet_explorer::ExplorerToPlanet;
use common_game::utils::ID;
use crossbeam_channel::Sender;
use std::time::Duration;

///**Move To Planet Conversation - Send Move Request**
///
/// This state serves as the "Command Dispatch" phase. It bridges the gap between the successful
/// Orchestrator-Planet handshake and the Explorer's actual transition. Its primary role is to
/// provide the Explorer with the technical means (communication channels) to interact with its new home.
// SEND MOVE REQUEST IMPLEMENTATION
impl Conversation<ExplorerBagContent> for MoveToPlanetConversation<SendMoveRequest> {
    /// Returns the unique ID of the conversation instance.
    fn get_id(&self) -> ID {
        self.id
    }

    /// Returns the IDs of the destination planet and the explorer being commanded to move.
    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (
            Some(self.state.dst_planet_id),
            Some(self.state.explorer_struct.explorer_id),
        )
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        None
    }

    /// ### Transition Function: Dispatching the Move Command
    ///
    /// This function evaluates the authorization state of the movement and constructs the
    /// final instruction for the Explorer. It handles the following logic:
    ///
    /// #### 1. Handshake Verification (`is_explorer_moving == true`)
    /// If the planet-to-planet handshake was successful, the Orchestrator attempts to resolve
    /// the communication channel for the destination planet.
    /// * **Channel Found**: The `Sender<ExplorerToPlanet>` is extracted from the global
    ///   registry and attached to the `MoveToPlanet` message. This allows the explorer to
    ///   speak to the destination planet immediately.
    /// * **Channel Missing**: If no active channel is found for the destination ID,
    ///   the move transitions to an [`ErrorState`] with [`CommonErrorTypes::ExplorerSenderNotFound`].
    ///
    /// #### 2. Unauthorized Movement (`is_explorer_moving == false`)
    /// Used when a move was rejected (e.g., non-neighbors). The transition proceeds but
    /// sends a `None` sender. This signals the Explorer to handle a failed transition.
    ///
    /// #### 3. Execution Outcomes
    /// * **Success**: Advances to [`WaitMoveToPlanetResponse`].
    /// * **Failure**: If the Explorer's channel is not working, transitions to an
    ///   [`ErrorState`] via [`CommonErrorTypes::MessageToExplorerFailed`].
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        // Determine the sender
        let sender_to_new_planet = if self.state.is_explorer_moving {
            // Explorer is moving, we need to find the sender to the planet
            if let Some(sender) = self.get_new_planet_sender(self.state.dst_planet_id) {
                Some(sender)
            } else {
                let error = Box::new(CommonErrorTypes::ExplorerSenderNotFound(
                    self.state.dst_planet_id,
                ));
                let error_state = ErrorState::new(error, self.id);
                return Some(Box::new(error_state)
                    as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>);
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
                    self.state.explorers_location_ref.clone(),
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
                Some(Box::new(error_state)
                    as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
            }
        }
    }

    /// **Priority 5**: Movement commands are high-priority to ensure entity locations
    /// are synchronized across the system before processing lower-level AI tasks.
    fn get_priority(&self) -> i32 {
        5
    }
}

impl MoveToPlanetConversation<SendMoveRequest> {
    /// Retrieves the sender to the destination planet from the shared registry.
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
/// This is the final terminal state in the movement sequence. It ensures that the Orchestrator
/// and the Explorer have a synchronized view of the world state after the handover.
// WAIT MOVE TO PLANET RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBagContent> for MoveToPlanetConversation<WaitMoveToPlanetResponse> {
    /// Returns the unique ID of the conversation instance.
    fn get_id(&self) -> ID {
        self.id
    }

    /// Returns the IDs of the destination planet and the explorer finalizing the move.
    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (Some(self.state.dst_planet_id), Some(self.state.explorer_id))
    }

    /// Listens specifically for [`ExplorerToOrchestratorKind::MovedToPlanetResult`].
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// ### Transition Function: Finalizing World State
    ///
    /// This function acts as the final gatekeeper for the global location registry. It processes
    /// the Explorer's arrival confirmation in three distinct ways:
    ///
    /// #### 1. Successful Location Update
    /// When `is_explorer_moving` is true, the Orchestrator performs a thread-safe update to
    /// the `explorers_location_ref` map.
    /// * **Update Success**: Returns `None`. This **terminates** the conversation successfully,
    ///   closing the movement lifecycle.
    ///
    /// #### 2. Graceful Termination of Rejections
    /// If the move was flagged as unauthorized, the explorer still acknowledges the instruction.
    /// The function logs a `Warning` explaining that the move was blocked (e.g., non-neighbors)
    /// and returns `None` to close the conversation without modifying the world state.
    ///
    /// #### 3. Protocol Enforcement
    /// Receiving any message other than the movement result results in a transition to
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`].
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::MovedToPlanetResult {
                explorer_id,
                planet_id,
            },
        )) = msg_wrapped
        {
            // Explorer is moving, need to change its location in Orchestrator reference
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

                self.move_explorer_location(explorer_id, planet_id);
                log_internal(
                    Channel::Debug,
                    payload!(
                        action : "Changed Explorer location in List, closing conversation",
                        explorer_id : explorer_id,
                        changed_to_planet_id : planet_id,
                        conversation_id : self.id
                    ),
                );
            } else {
                // Explorer responded correctly but move was disallowed previously
                log_internal(
                    Channel::Warning,
                    payload!(
                        action : "Explorer cannot move (destination not a neighbor), closing conversation",
                        explorer_id : explorer_id,
                        destination_planet_id : planet_id,
                        conversation_id : self.id
                    ),
                );
            }
            return None; // Graceful close
        }
        // Wrong message, closing Conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
    }

    fn get_priority(&self) -> i32 {
        4
    }

    // Longer timeout, since it involves a communication with an Explorer
    fn get_timeout(&self) -> Option<Duration> {
        Some(Duration::from_millis(get_explorer_timeout()))
    }
}

impl MoveToPlanetConversation<WaitMoveToPlanetResponse> {
    /// Internal constructor for the [`WaitMoveToPlanetResponse`] state.
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
    fn move_explorer_location(&self, explorer_id: ID, dst_planet_id: ID) {
        self.state
            .explorers_location_ref
            .lock()
            .unwrap()
            .insert(explorer_id, dst_planet_id);
    }
}
