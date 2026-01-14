use crate::galaxy_setup::PlanetMap;
use crate::logging_utils::log_internal;
use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToExplorerStruct,
};
use crate::payload;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;

///**Neighbors Discovery Conversation**
///
/// This module manages the process of an Explorer discovering the adjacent planets (neighbors)
/// of its current location.
/// It uses a Finite State Machine (FSM) to ensure that the exchange of messages happens in the appropriate order
/// Custom error type for when a planet ID provided by an explorer does not exist in the galaxy.
struct PlanetNotFound(ID);
impl ErrorType for PlanetNotFound {
    fn stringify(&self) -> String {
        format!(
            "Planet {} not found in current galaxy, can't provide neighbors",
            self.0
        )
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingExplorerNeighborsRequest`] state, the conversation waits for the explorer
/// to send a [`ExplorerToOrchestrator::NeighborsRequest`]. It holds a reference to the [`PlanetMap`]
/// to resolve the query.
pub(crate) struct WaitingExplorerNeighborsRequest {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
    /// Reference to the galaxy map used to find neighboring IDs
    galaxy: PlanetMap,
}

impl WaitingExplorerNeighborsRequest {
    /// Constructor for [`WaitingExplorerNeighborsRequest`] state struct
    pub(crate) fn new(to_explorer_struct: ToExplorerStruct, galaxy: PlanetMap) -> Self {
        Self {
            to_explorer_struct,
            galaxy,
        }
    }
}

/// Marker struct for FSM state
///
/// In the [`SendingNeighborsResponse`] state, the conversation sends the collected
/// list of neighboring planet IDs back to the explorer via [`OrchestratorToExplorer::NeighborsResponse`].
struct SendingNeighborsResponse {
    /// A struct containing fields to send messages to the specific explorer
    to_explorer_struct: ToExplorerStruct,
    /// The list of neighbor planet IDs found in the galaxy map
    neighbors: Vec<ID>,
}

impl SendingNeighborsResponse {
    /// Constructor for [`SendingNeighborsResponse`] state struct
    fn new(to_explorer_struct: ToExplorerStruct, neighbors: Vec<ID>) -> Self {
        Self {
            to_explorer_struct,
            neighbors,
        }
    }
}

/// Neighbors Discovery Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
pub(crate) struct NeighborsDiscoveryConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING NEIGHBORS RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBag> for NeighborsDiscoveryConversation<SendingNeighborsResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingNeighborsResponse`] state:
    ///
    /// Returns:
    ///
    /// [None] if the neighbor list is successfully sent to the explorer, ending the conversation.
    ///
    /// [`ErrorState`] if the message failed to send or the explorer's sender is missing.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_explorer_struct
            .to_explorer(OrchestratorToExplorer::NeighborsResponse {
                neighbors: self.state.neighbors,
            }) {
            Ok(()) => {
                log_internal(
                    Channel::Debug,
                    payload!(
                        action : "Correctly sent its neighbors to Explorer, closing conversation",
                        explorer_id : self.state.to_explorer_struct.explorer_id,
                        conversation_id : self.id
                    ),
                );
                None
            }
            Err(err) => {
                let error = match err {
                    ToExplorerError::SendingMessageFailure(id) => {
                        CommonErrorTypes::MessageToExplorerFailed(id)
                    }
                    ToExplorerError::SenderNotFound(id) => {
                        CommonErrorTypes::ExplorerSenderNotFound(id)
                    }
                };
                let error_state = ErrorState::new(Box::new(error), self.id);
                Some(Box::new(error_state))
            }
        }
    }

    fn get_priority(&self) -> i32 {
        3
    }
}

impl NeighborsDiscoveryConversation<SendingNeighborsResponse> {
    /// The constructor for [`NeighborsDiscoveryConversation`] in the [`SendingNeighborsResponse`] state
    fn new(id: ID, state: SendingNeighborsResponse) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING EXPLORER NEIGHBORS REQUEST IMPLEMENTATION
impl Conversation<ExplorerBag> for NeighborsDiscoveryConversation<WaitingExplorerNeighborsRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_explorer_struct.explorer_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingExplorerNeighborsRequest`] state:
    ///
    /// Returns:
    ///
    /// [`NeighborsDiscoveryConversation<SendingNeighborsResponse>`] if the request is valid and neighbors are found.
    ///
    /// [`ErrorState`] if the planet ID is not found in the galaxy or a wrong message type is received.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::NeighborsRequest {
            explorer_id: _explorer_id,
            current_planet_id,
        })) = msg_wrapped
        {
            return match self.get_neighbors(current_planet_id) {
                Ok(neighbors) => {
                    let state_struct =
                        SendingNeighborsResponse::new(self.state.to_explorer_struct, neighbors);
                    let next_state =
                        NeighborsDiscoveryConversation::<SendingNeighborsResponse>::new(
                            self.id,
                            state_struct,
                        );
                    Some(Box::new(next_state))
                }
                Err(err) => {
                    let error_struct = ErrorState::new(err, self.id);
                    Some(Box::new(error_struct))
                }
            };
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        3
    }
}

impl NeighborsDiscoveryConversation<WaitingExplorerNeighborsRequest> {
    /// The constructor for [`NeighborsDiscoveryConversation`] in the [`WaitingExplorerNeighborsRequest`] state
    pub(crate) fn new(id: ID, state: WaitingExplorerNeighborsRequest) -> Self {
        Self {
            id,
            expected_message: Some(ExplorerToOrchKind(
                ExplorerToOrchestratorKind::NeighborsRequest,
            )),
            state,
        }
    }

    /// Helper function to access the galaxy map and retrieve the neighbors of a specific planet.
    fn get_neighbors(
        &self,
        curr_planet_id: ID,
    ) -> Result<Vec<ID>, Box<dyn ErrorType + Send + Sync>> {
        if let Some(curr_planet_ref) = self.state.galaxy.lock().unwrap().get(&curr_planet_id) {
            let neighbors = curr_planet_ref.lock().unwrap().get_neighbors();
            Ok(neighbors)
        } else {
            Err(Box::new(PlanetNotFound(curr_planet_id)))
        }
    }
}
