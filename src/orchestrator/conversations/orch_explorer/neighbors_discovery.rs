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

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, Some(self.state.to_explorer_struct.explorer_id))
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
                Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBag> + Send + Sync>)
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

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, Some(self.state.to_explorer_struct.explorer_id))
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
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBag> + Send + Sync>)
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
            let neighbors = curr_planet_ref.get_neighbors();
            Ok(neighbors)
        } else {
            Err(Box::new(PlanetNotFound(curr_planet_id)))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::galaxy_setup::galaxy_loader;

    use super::*;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: u32 = 1;
    const EXPLORER_ID: u32 = 2;
    const INIT_PATH: &str = "test_galaxy.txt";

    type ExplorerSenders =
        Arc<Mutex<HashMap<ID, crossbeam_channel::Sender<OrchestratorToExplorer>>>>;

    struct MakeSendersResult(
        ExplorerSenders,
        crossbeam_channel::Receiver<OrchestratorToExplorer>,
    );

    // --- Helper functions ---
    fn make_senders_with(explorer_id: ID) -> MakeSendersResult {
        let (tx, rx) = unbounded::<OrchestratorToExplorer>();
        MakeSendersResult(Arc::new(Mutex::new(HashMap::from([(explorer_id, tx)]))), rx)
    }

    fn make_empty_senders() -> ExplorerSenders {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn make_to_explorer_struct(explorer_id: ID, senders: ExplorerSenders) -> ToExplorerStruct {
        ToExplorerStruct {
            explorer_id,
            explorers_senders: senders,
        }
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv(
        senders: ExplorerSenders,
        file_path: &str,
    ) -> Box<NeighborsDiscoveryConversation<WaitingExplorerNeighborsRequest>> {
        let to_explorer_struct = make_to_explorer_struct(EXPLORER_ID, senders);
        let (galaxy, _planets_receiver, _orch_to_plan_senders, _expl_to_plan_senders) =
            galaxy_loader(std::path::Path::new(file_path));
        let state = WaitingExplorerNeighborsRequest::new(to_explorer_struct, galaxy);
        Box::new(NeighborsDiscoveryConversation::<
            WaitingExplorerNeighborsRequest,
        >::new(CONV_ID, state))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(
        senders: ExplorerSenders,
        neighbors: Vec<ID>,
    ) -> Box<NeighborsDiscoveryConversation<SendingNeighborsResponse>> {
        let to_explorer_struct = make_to_explorer_struct(EXPLORER_ID, senders);
        let state = SendingNeighborsResponse::new(to_explorer_struct, neighbors);
        Box::new(NeighborsDiscoveryConversation::<SendingNeighborsResponse>::new(CONV_ID, state))
    }

    // --- Tests ---

    #[test]
    fn wait_correct_transition() {
        let MakeSendersResult(senders, _rx) = make_senders_with(EXPLORER_ID);
        let conv = make_wait_conv(senders, INIT_PATH);
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::NeighborsRequest {
            explorer_id: EXPLORER_ID,
            current_planet_id: 1_010_001, // Valid planet ID from galaxy.json
        });
        let next_conv = conv
            .transition(Some(msg))
            .expect("Should transition to SendingNeighborsResponse state");
        assert_eq!(next_conv.get_expected_kind(), None);
        assert_eq!(next_conv.get_id(), CONV_ID);
    }

    #[test]
    fn wait_wrong_message() {
        let explorer_senders = make_empty_senders();
        let conv = make_wait_conv(explorer_senders, INIT_PATH);
        let wrong_msg =
            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::StartExplorerAIResult {
                explorer_id: EXPLORER_ID,
            });
        let result = conv
            .transition(Some(wrong_msg))
            .expect("Should return an ErrorState");
        assert_eq!(result.get_id(), CONV_ID);
        assert_eq!(
            result.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }

    #[test]
    fn wait_getters() {
        let explorer_senders = make_empty_senders();
        let conv = make_wait_conv(explorer_senders, INIT_PATH);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PossibleExpectedKinds::ExplorerToOrchKind(
                ExplorerToOrchestratorKind::NeighborsRequest
            ))
        );
        assert_eq!(conv.get_priority(), 3);
    }

    #[test]
    fn wait_planet_not_found_error() {
        let MakeSendersResult(senders, _rx) = make_senders_with(EXPLORER_ID);
        let conv = make_wait_conv(senders, INIT_PATH);
        // Use a planet ID that does not exist in galaxy.txt
        let invalid_planet_id = 9_999_999;
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::NeighborsRequest {
            explorer_id: EXPLORER_ID,
            current_planet_id: invalid_planet_id,
        });
        let result = conv
            .transition(Some(msg))
            .expect("Should return an ErrorState");
        let details = result
            .get_error_details()
            .expect("Should have error details");
        assert!(
            details.contains(&format!("Planet {invalid_planet_id} not found")),
            "Error message should mention planet not found"
        );
    }

    #[test]
    fn send_success() {
        let MakeSendersResult(senders, _rx) = make_senders_with(EXPLORER_ID);
        let conv = make_send_conv(senders, vec![10, 20, 30]);
        let result = conv.transition(None);
        assert!(
            result.is_none(),
            "Conversation should terminate after sending NeighborsResponse"
        );
    }

    #[test]
    fn send_missing_sender() {
        let senders = make_empty_senders();
        let conv = make_send_conv(senders, vec![10, 20, 30]);
        let next_conv = conv.transition(None).expect("Should return an ErrorState");
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("sender to explorer {EXPLORER_ID} not found"))
        );
    }

    #[test]
    fn send_message_failure() {
        let (tx, rx) = unbounded::<OrchestratorToExplorer>();
        drop(rx);
        let senders = Arc::new(Mutex::new(HashMap::from([(EXPLORER_ID, tx)])));
        let conv = make_send_conv(senders, vec![10, 20, 30]);
        let next_conv = conv.transition(None).expect("Should return an ErrorState");
        let error_msg = next_conv
            .get_error_details()
            .expect("Should return an Error Details String");
        assert_eq!(
            error_msg,
            format!("failed to send message to explorer {EXPLORER_ID}")
        );
    }

    #[test]
    fn send_getters() {
        let MakeSendersResult(senders, _rx) = make_senders_with(EXPLORER_ID);
        let to_explorer = make_to_explorer_struct(EXPLORER_ID, senders);
        let state = SendingNeighborsResponse::new(to_explorer, vec![10, 20, 30]);
        let conv = NeighborsDiscoveryConversation::<SendingNeighborsResponse>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 3);
    }
}
