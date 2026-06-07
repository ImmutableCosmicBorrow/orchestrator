use crate::convo_manager::OrchContextRef;
use crate::globals::get_convo_timeout;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::params::ConvoKind;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, ErrorType, PlanetCommunicator,
    PossibleExpectedKinds, PossibleMessage,
};
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
use std::time::Duration;

///** Advertising Dead Explorer Conversation**
///
/// This module manages the process of a planet handling a dead explorer.
/// It uses a Finite State Machine (FSM) to send the request and wait for the planet's confirmation
/// of the elimination of the channel used to communicate with the dead explorer.
///
/// If successful, it closes with a None, If it fails, it transitions to an [`ErrorState`].
/// Custom error type for when a planet fails to process an explorer departure.
struct FailedToHandleOutgoingExplorer {
    /// The planet that failed the operation.
    planet_id: ID,
    /// The explorer involved in the failed departure.
    explorer_id: ID,
}

impl ErrorType for FailedToHandleOutgoingExplorer {
    fn stringify(&self) -> String {
        format!(
            "Planet {} failed to handle dead explorer {}",
            self.planet_id, self.explorer_id
        )
    }
}

// --- ADVERTISING DEAD EXPLORER CONVERSATION ---

define_conversation!(
    name: AdvDeadExplorer
);

// --- SENDING DEAD EXPLORER ADV STATE ---

create_request_state!(
    state_name: SendingDeadExpAdv,
    conv_name: AdvDeadExplorer,
    convo_kind: ConvoKind::AdvDeadExplorer,
    timeout: None,
    expected_msg: None,
    fields: {
        planet_id: ID,
        dead_explorer_id: ID,
    },
    entities_id_fn: |this: &AdvDeadExplorer<SendingDeadExpAdv>| {(Some(this.state.planet_id), None)},
    transition_fn: send_dead_exp_adv_transition,
    methods_settings: { },
);

/// Transition Function for [`SendingDeadExpAdv`] state:
///
/// Returns:
///
/// [`ErrorState`] if the request failed to send to the planet or the sender to the planet is not found.
///
/// [`AdvDeadExplorer<WaitingDeadAdvResponse>`] if the request was sent successfully.
fn send_dead_exp_adv_transition(
    this: Box<AdvDeadExplorer<SendingDeadExpAdv>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this.state.to_planet(
        this.state.planet_id,
        OrchestratorToPlanet::OutgoingExplorerRequest {
            explorer_id: this.state.planet_id,
        },
    ) {
        Ok(()) => {
            let planet_id = this.state.planet_id;
            let state_struct = WaitingDeadAdvResponse::new(this.state.orch_context, planet_id);
            let next_state = AdvDeadExplorer::<WaitingDeadAdvResponse>::new(this.id, state_struct);
            Some(Box::new(next_state))
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAITING DEAD EXPLORER STATE DEFINITION ---

create_response_state!(
    state: WaitingDeadAdvResponse,
    conv: AdvDeadExplorer,
    convo_kind: ConvoKind::AdvDeadExplorer,
    timeout: Some(get_convo_timeout()),
    expected_msg: PlanetToOrchKind(PlanetToOrchestratorKind::OutgoingExplorerResponse),
    fields: {
        planet_id: ID,
    },
    entities_id_closure: |this: &AdvDeadExplorer<WaitingDeadAdvResponse>| { (Some(this.state.planet_id), None) },
    transition: wait_dead_adv_response_transition,
    methods_settings: {},
);

/// Transition Function for [`WaitingDeadAdvResponse`] state:
///
/// Returns:
///
/// [None] if the planet confirms the explorer departure, closing the conversation.
///
/// [`ErrorState`] if the planet returns an error response.
fn wait_dead_adv_response_transition(
    this: Box<AdvDeadExplorer<WaitingDeadAdvResponse>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::OutgoingExplorerResponse {
        planet_id,
        explorer_id,
        res,
    })) = msg
    {
        return if res.is_ok() {
            log_internal(
                LogTarget::Conversations,
                Channel::Trace,
                payload!(
                    action : "Planet correctly handled dead explorer, closing conversation",
                    planet_id : planet_id,
                    outgoing_explorer_id : explorer_id,
                    conversation_id : this.id,
                ),
            );
            None
        } else {
            //Explorer is killed but channel in planet is there!
            let error = FailedToHandleOutgoingExplorer {
                planet_id,
                explorer_id,
            };
            let error_state = ErrorState::new(Box::new(error), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        };
    }

    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::conversations::orch_planet::test_utils::{
        add_broken_planet_sender, add_working_planet_sender, make_test_context,
    };
    use crate::ui::{OrchestratorToUiUpdate, UiToOrchestratorCommand};
    use crossbeam_channel::unbounded;

    const CONV_ID: ID = 10;
    const PLANET_ID: ID = 20;
    const EXPLORER_ID: ID = 30;

    // --- Helper functions ---

    fn make_send_conv(orch_context: OrchContextRef) -> Box<AdvDeadExplorer<SendingDeadExpAdv>> {
        let state = SendingDeadExpAdv::new(orch_context, PLANET_ID, EXPLORER_ID);
        Box::new(AdvDeadExplorer::<SendingDeadExpAdv>::new(CONV_ID, state))
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
    ) -> Box<AdvDeadExplorer<WaitingDeadAdvResponse>> {
        let state = WaitingDeadAdvResponse::new(orch_context, PLANET_ID);
        Box::new(AdvDeadExplorer::<WaitingDeadAdvResponse>::new(
            CONV_ID, state,
        ))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let _rx = add_working_planet_sender(test_ctx.channels_manager.as_ref(), PLANET_ID);
        let conv = make_send_conv(test_ctx.clone());
        let next_conv = conv
            .transition(None)
            .expect("Should transition to WaitingOutgoingResponse");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::OutgoingExplorerResponse
            ))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn send_missing_sender() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_send_conv(test_ctx.clone());
        let next_conv = conv
            .transition(None)
            .expect("Should transition to ErrorState");
        assert!(next_conv.get_error_details().is_some());
        assert_eq!(
            next_conv.get_error_details().unwrap(),
            format!("sender to planet {PLANET_ID} not found")
        );
    }

    #[test]
    fn send_message_failure() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        add_broken_planet_sender(test_ctx.channels_manager.as_ref(), PLANET_ID);
        let conv = make_send_conv(test_ctx.clone());
        let next_conv = conv.transition(None).expect("Should return an ErrorState");
        let error_msg = next_conv
            .get_error_details()
            .expect("Should return an Error Details String");
        assert_eq!(
            error_msg,
            format!("failed to send message to planet {PLANET_ID}")
        );
    }

    #[test]
    fn send_getters() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_send_conv(test_ctx.clone());
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(
            conv.get_priority(),
            ConvoKind::AdvDeadExplorer.priority().as_i32()
        );
    }

    #[test]
    fn wait_success() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::OutgoingExplorerResponse {
            planet_id: PLANET_ID,
            explorer_id: EXPLORER_ID,
            res: Ok(()),
        });
        let next_conv = conv.transition(Some(msg));
        assert!(next_conv.is_none(), "Conversation should end successfully");
    }

    #[test]
    fn wait_wrong_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::AsteroidAck {
            planet_id: PLANET_ID,
            rocket: None,
        });
        let next_conv = conv.transition(Some(wrong_msg));
        // Now, the conversation should return an ErrorState on a wrong message
        assert!(
            next_conv.is_some(),
            "Conversation should return an ErrorState on wrong message"
        );
        let error_details = next_conv.unwrap().get_error_details();
        assert!(
            error_details.is_some(),
            "ErrorState should have error details"
        );
        assert_eq!(error_details.unwrap(), "Wrong Message Received");
    }

    #[test]
    fn wait_getters() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::OutgoingExplorerResponse
            ))
        );
        assert_eq!(
            conv.get_priority(),
            ConvoKind::AdvDeadExplorer.priority().as_i32()
        );
    }

    #[test]
    fn wait_error_response() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::OutgoingExplorerResponse {
            planet_id: PLANET_ID,
            explorer_id: EXPLORER_ID,
            res: Err(String::new()),
        });
        let next_conv = conv
            .transition(Some(msg))
            .expect("Should return ErrorState");
        assert_eq!(next_conv.get_id(), CONV_ID);
        let details = next_conv
            .get_error_details()
            .expect("Should have error details");
        assert_eq!(
            details,
            format!("Planet {PLANET_ID} failed to handle dead explorer {EXPLORER_ID}")
        );
    }
}
