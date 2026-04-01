use crate::globals::{get_explorer_timeout, TIMEOUT};
use crate::logging_utils::{log_internal, LogTarget};
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::{ChannelsContext, CommonErrorTypes, Conversation, ErrorState, ErrorType, ExplorerCommunicator, ExplorerContext, PossibleExpectedKinds, PossibleMessage};
use crate::orchestrator::ChannelsManagerRef;
use crate::planet::PlanetMap;
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::utils::ID;
use std::time::Duration;

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

// --- NEIGHBORS DISCOVERY CONVERSATION ---
define_conversation!(
    name: NeighborsDiscoveryConversation
);

// --- SEND NEIGHBORS DEFINITION ---
create_request_state!(
    state_name: SendingNeighbors,
    conv_name: NeighborsDiscoveryConversation,
    priority: 3,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        channels_manager: ChannelsManagerRef,
        explorer_id: ID,
        neighbors_list: Vec<ID>
    },
    entities_id_fn: |this: &NeighborsDiscoveryConversation<SendingNeighbors>  | { (None, Some(this.state.explorer_id)) },
    transition_fn: send_neighbors_transition,
    methods_settings: {

    },
);

impl ExplorerContext for SendingNeighbors {
    fn get_explorer_id(&self) -> ID {
        self.explorer_id
    }
}

impl ChannelsContext for SendingNeighbors {
    fn get_channels_manager(&self) -> &ChannelsManagerRef {
        &self.channels_manager
    }
}

/// Transition Function for [`SendingNeighborsResponse`] state:
///
/// Returns:
///
/// [None] if the neighbor list is successfully sent to the explorer, ending the conversation.
///
/// [`ErrorState`] if the message failed to send or the explorer's sender is missing.
fn send_neighbors_transition(this: Box<NeighborsDiscoveryConversation<SendingNeighbors>>) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this
        .state
        .to_explorer(OrchestratorToExplorer::NeighborsResponse {
            neighbors: this.state.neighbors_list.clone(),
        }) {
        Ok(()) => {
            log_internal(
                LogTarget::Conversations,
                Channel::Debug,
                payload!(
                        action : "Correctly sent its neighbors to Explorer, closing conversation",
                        explorer_id : this.state.explorer_id,
                        conversation_id : this.id
                    ),
            );
            None
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state)
                as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAITING NEIGHBORS REQUEST DEFINITION ---

create_response_state!(
    state: WaitingNeighborsRequest,
    conv: NeighborsDiscoveryConversation,
    priority: 3,
    timeout: Some(get_explorer_timeout()),
    expected_msg: ExplorerToOrchKind(ExplorerToOrchestratorKind::NeighborsRequest),
    fields: {
        channels_manager: ChannelsManagerRef,
        explorer_id: ID,
        galaxy: PlanetMap
    },
    entities_id_closure: |this: &NeighborsDiscoveryConversation<WaitingNeighborsRequest>| { (None, Some(this.state.explorer_id)) },
    transition: wait_neighbors_req_transition,
    methods_settings: {

    },
);

impl ExplorerContext for WaitingNeighborsRequest {
    fn get_explorer_id(&self) -> ID {
        self.explorer_id
    }
}

impl WaitingNeighborsRequest {
    /// Helper function to access the galaxy map and retrieve the neighbors of a specific planet.
    fn get_neighbors(
        &self,
        curr_planet_id: ID,
    ) -> Result<Vec<ID>, Box<dyn ErrorType + Send + Sync>> {
        if let Some(curr_planet_ref) = self.galaxy.read().unwrap().get(&curr_planet_id) {
            let neighbors = curr_planet_ref.neighbors_snapshot();
            Ok(neighbors)
        } else {
            Err(Box::new(PlanetNotFound(curr_planet_id)))
        }
    }
}

/// Transition Function for [`WaitingExplorerNeighborsRequest`] state:
///
/// Returns:
///
/// [`NeighborsDiscoveryConversation<SendingNeighborsResponse>`] if the request is valid and neighbors are found.
///
/// [`ErrorState`] if the planet ID is not found in the galaxy or a wrong message type is received.
fn wait_neighbors_req_transition(this: Box<NeighborsDiscoveryConversation<WaitingNeighborsRequest>>, msg: Option<PossibleMessage>) -> Option<Box<dyn Conversation + Send + Sync>> {

    if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::NeighborsRequest {
          explorer_id: _explorer_id,
          current_planet_id,
     })) = msg
    {
        return match this.state.get_neighbors(current_planet_id) {
            Ok(neighbors) => {
                let state_struct =
                    SendingNeighbors::new(this.state.channels_manager, this.state.explorer_id, neighbors);
                let next_state =
                    NeighborsDiscoveryConversation::<SendingNeighbors>::new(
                        this.id,
                        state_struct,
                    );
                Some(Box::new(next_state))
            }

            Err(err) => {
                let error_struct = ErrorState::new(err, this.id);
                Some(Box::new(error_struct)
                    as Box<dyn Conversation + Send + Sync>)
            }
        };
    }

    //Wrong Message, return error state
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}

/*
#[cfg(test)]
mod tests {
    use crate::galaxy_setup::galaxy_loader;

    use super::*;
    use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
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
        let (to_ui_tx, _to_ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_from_ui_tx, from_ui_rx) = unbounded::<UiToOrchestratorCommand>();
        let (galaxy, _join_handles) = galaxy_loader(
            std::path::Path::new(file_path),
            &crate::orchestrator::ChannelsManager::new(to_ui_tx, from_ui_rx),
        );
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
            current_planet_id: 49_153, // Valid planet ID from galaxy.json
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
            Some(ExplorerToOrchKind(
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
*/