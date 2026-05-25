use crate::convo_manager::OrchContextRef;
use crate::globals::{TIMEOUT, get_explorer_timeout};
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, ErrorType, ExplorerCommunicator,
    PossibleExpectedKinds, PossibleMessage,
};
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
        explorer_id: ID,
        neighbors_list: Vec<ID>
    },
    entities_id_fn: |this: &NeighborsDiscoveryConversation<SendingNeighbors>  | { (None, Some(this.state.explorer_id)) },
    transition_fn: send_neighbors_transition,
    methods_settings: {

    },
);

/// Transition Function for [`SendingNeighborsResponse`] state:
///
/// Returns:
///
/// [None] if the neighbor list is successfully sent to the explorer, ending the conversation.
///
/// [`ErrorState`] if the message failed to send or the explorer's sender is missing.
fn send_neighbors_transition(
    this: Box<NeighborsDiscoveryConversation<SendingNeighbors>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this.state.to_explorer(
        this.state.explorer_id,
        OrchestratorToExplorer::NeighborsResponse {
            neighbors: this.state.neighbors_list.clone(),
        },
    ) {
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
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
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
        explorer_id: ID,
    },
    entities_id_closure: |this: &NeighborsDiscoveryConversation<WaitingNeighborsRequest>| { (None, Some(this.state.explorer_id)) },
    transition: wait_neighbors_req_transition,
    methods_settings: {

    },
);

impl WaitingNeighborsRequest {
    /// Helper function to access the galaxy map and retrieve the neighbors of a specific planet.
    fn get_neighbors(
        &self,
        curr_planet_id: ID,
    ) -> Result<Vec<ID>, Box<dyn ErrorType + Send + Sync>> {
        let galaxy = self.orch_context.galaxy.clone();

        if let Some(curr_planet_ref) = galaxy.read().unwrap().get(&curr_planet_id) {
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
fn wait_neighbors_req_transition(
    this: Box<NeighborsDiscoveryConversation<WaitingNeighborsRequest>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::NeighborsRequest {
        explorer_id: _explorer_id,
        current_planet_id,
    })) = msg
    {
        return match this.state.get_neighbors(current_planet_id) {
            Ok(neighbors) => {
                let state_struct = SendingNeighbors::new(
                    this.state.orch_context,
                    this.state.explorer_id,
                    neighbors,
                );
                let next_state =
                    NeighborsDiscoveryConversation::<SendingNeighbors>::new(this.id, state_struct);
                Some(Box::new(next_state))
            }

            Err(err) => {
                let error_struct = ErrorState::new(err, this.id);
                Some(Box::new(error_struct) as Box<dyn Conversation + Send + Sync>)
            }
        };
    }

    //Wrong Message, return error state
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::orch_explorer::test_utils::{
        add_broken_explorer_sender, add_working_explorer_sender, make_test_context,
    };
    use crate::planet::{PlanetMap, add_planet_with_neighbors};
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
    use crossbeam_channel::unbounded;

    const CONV_ID: ID = 1;
    const EXPLORER_ID: ID = 2;
    const PLANET_ID: ID = 10;
    const NEIGHBOR_ID: ID = 11;

    fn make_galaxy() -> PlanetMap {
        let galaxy: PlanetMap = Arc::new(RwLock::new(HashMap::new()));
        add_planet_with_neighbors(&galaxy, PLANET_ID, [NEIGHBOR_ID]);
        galaxy
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
    ) -> Box<NeighborsDiscoveryConversation<WaitingNeighborsRequest>> {
        let state = WaitingNeighborsRequest::new(orch_context, EXPLORER_ID);
        Box::new(NeighborsDiscoveryConversation::<WaitingNeighborsRequest>::new(CONV_ID, state))
    }

    fn make_send_conv(
        orch_context: OrchContextRef,
        neighbors: Vec<ID>,
    ) -> Box<NeighborsDiscoveryConversation<SendingNeighbors>> {
        let state = SendingNeighbors::new(orch_context, EXPLORER_ID, neighbors);
        Box::new(NeighborsDiscoveryConversation::<SendingNeighbors>::new(
            CONV_ID, state,
        ))
    }

    // --- Tests ---

    #[test]
    fn wait_correct_transition() {
        let galaxy = make_galaxy();

        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();

        let test_ctx = make_test_context(Some(galaxy), None, ui_tx, ui_cmd_rx);

        let conv = make_wait_conv(test_ctx.clone());
        let msg = PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::NeighborsRequest {
            explorer_id: EXPLORER_ID,
            current_planet_id: PLANET_ID,
        });

        let next_conv = conv
            .transition(Some(msg))
            .expect("Should transition to SendingNeighbors state");
        assert_eq!(next_conv.get_expected_kind(), None);
        assert_eq!(next_conv.get_id(), CONV_ID);
    }

    #[test]
    fn wait_wrong_message() {
        let galaxy = make_galaxy();

        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(Some(galaxy), None, ui_tx, ui_cmd_rx);

        let conv = make_wait_conv(test_ctx.clone());
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
        let galaxy = make_galaxy();
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(Some(galaxy), None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
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
        let galaxy = make_galaxy();
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(Some(galaxy), None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        // Use a planet ID that does not exist in our test galaxy
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
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);
        let conv = make_send_conv(test_ctx.clone(), vec![10, 20, 30]);
        let result = conv.transition(None);
        assert!(
            result.is_none(),
            "Conversation should terminate after sending NeighborsResponse"
        );
    }

    #[test]
    fn send_missing_sender() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_send_conv(test_ctx.clone(), vec![10, 20, 30]);
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
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        add_broken_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);
        let conv = make_send_conv(test_ctx.clone(), vec![10, 20, 30]);
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
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_explorer_sender(test_ctx.channels_manager.as_ref(), EXPLORER_ID);
        let conv = make_send_conv(test_ctx.clone(), vec![10, 20, 30]);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (None, Some(EXPLORER_ID)));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 3);
    }
}
