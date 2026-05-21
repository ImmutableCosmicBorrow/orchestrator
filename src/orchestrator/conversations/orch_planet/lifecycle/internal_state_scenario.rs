use crate::convo_manager::OrchContextRef;
use crate::globals::TIMEOUT;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::Duration;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, PlanetCommunicator,
    PossibleExpectedKinds, PossibleMessage, UiCommunicator,
};
use crate::ui::OrchestratorToUiUpdate;
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;

//**Internal State Conversation**
//
// This module manages the conversation between the Orchestrator and a Planet regarding its internal state.
// It uses a Finite State Machine (FSM) to ensure that requests and responses are handled in the correct
// order at compile time.
//
// The conversation flow starts by sending a request and terminates once the planet's state
// is received (intended for UI reporting).
//
// Marker struct for FSM state
//
// The conversation starts in the [`SendingInternalStateRequest`] state, which sends an
// [`OrchestratorToPlanet::InternalStateRequest`] when the [`Conversation::transition`] method is called.

// --- INTERNAL STATE CONVERSATION ---

define_conversation!(
    name: InternalStateConversation
);

// --- SENDING INTERNAL STATE REQUEST DEFINITION ---

create_request_state!(
    state_name: SendingInternalStateRequest,
    conv_name: InternalStateConversation,
    priority: 3,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        planet_id: ID,
    },
    entities_id_fn: |this: &InternalStateConversation<SendingInternalStateRequest>| { (Some(this.state.planet_id), None) },
    transition_fn: send_internal_state_req_transition,
    methods_settings: {

    },
);

fn send_internal_state_req_transition(
    this: Box<InternalStateConversation<SendingInternalStateRequest>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this.state.to_planet(
        this.state.planet_id,
        OrchestratorToPlanet::InternalStateRequest,
    ) {
        Ok(()) => {
            let next_state =
                WaitingInternalStateResponse::new(this.state.orch_context, this.state.planet_id);
            let next_conv =
                InternalStateConversation::<WaitingInternalStateResponse>::new(this.id, next_state);
            Some(Box::new(next_conv) as Box<dyn Conversation + Send + Sync>)
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAITING INTERNAL STATE RESPONSE ---

create_response_state!(
    state: WaitingInternalStateResponse,
    conv: InternalStateConversation,
    priority: 3,
    timeout: Some(TIMEOUT),
    expected_msg: PlanetToOrchKind(PlanetToOrchestratorKind::InternalStateResponse),
    fields: {
        planet_id: ID,
    },
    entities_id_closure: |this: &InternalStateConversation<WaitingInternalStateResponse>| { (Some(this.state.planet_id), None) },
    transition: wait_internal_state_res_transition,
    methods_settings: {

    },
);

/// Transition Function for [`WaitingInternalStateResponse`] state:
///
/// Returns:
///
/// [None] if the state is successfully received and sent to the UI, closing the conversation
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different from the expected one [`PlanetToOrchestrator::InternalStateResponse`]
/// [`ErrorState`] with [`CommonErrorTypes::MessageToUiFailed`] if the message sending to the UI fails
fn wait_internal_state_res_transition(
    this: Box<InternalStateConversation<WaitingInternalStateResponse>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::InternalStateResponse {
        planet_id,
        planet_state,
    })) = msg
    {
        log_internal(
            LogTarget::Conversations,
            Channel::Debug,
            payload!(
                action : "Planet sent its internal state",
                planet_id : planet_id,
                planet_state : format!("{planet_state:?}"),
                conversation_id : this.id
            ),
        );
        // Send planet state to UI
        return match this.state.to_ui(OrchestratorToUiUpdate::PlanetSnapshot(
            planet_id,
            planet_state,
        )) {
            Ok(()) => None,
            Err(err) => {
                let error_state = ErrorState::new(Box::new(err), this.id);
                Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
            }
        };
    }

    //Wrong Message, close conversation
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
    use common_game::components::planet::DummyPlanetState;
    use crossbeam_channel::unbounded;

    const CONV_ID: ID = 1;
    const PLANET_ID: ID = 2;

    // --- Helper functions ---

    fn make_send_conv(
        orch_context: OrchContextRef,
    ) -> Box<InternalStateConversation<SendingInternalStateRequest>> {
        let state = SendingInternalStateRequest::new(orch_context, PLANET_ID);
        Box::new(InternalStateConversation::<SendingInternalStateRequest>::new(CONV_ID, state))
    }

    fn make_wait_conv(
        orch_context: OrchContextRef,
    ) -> Box<InternalStateConversation<WaitingInternalStateResponse>> {
        let state = WaitingInternalStateResponse::new(orch_context, PLANET_ID);
        Box::new(InternalStateConversation::<WaitingInternalStateResponse>::new(CONV_ID, state))
    }

    fn make_dummy_planet_state() -> DummyPlanetState {
        DummyPlanetState {
            energy_cells: vec![],
            charged_cells_count: 0,
            has_rocket: false,
        }
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
            .expect("Should transition to Waiting state");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::InternalStateResponse
            ))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn send_missing_sender() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_send_conv(test_ctx.clone());
        let next_conv = conv.transition(None).expect("Should return ErrorState");
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("sender to planet {PLANET_ID} not found"))
        );
        assert!(next_conv.transition(None).is_none());
    }

    #[test]
    fn send_message_failure() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        add_broken_planet_sender(test_ctx.channels_manager.as_ref(), PLANET_ID);
        let conv = make_send_conv(test_ctx.clone());
        let next_conv = conv.transition(None).expect("Should return ErrorState");
        let error_msg = next_conv
            .get_error_details()
            .expect("Should have error details");
        assert_eq!(
            error_msg,
            format!("failed to send message to planet {PLANET_ID}")
        );
        assert!(next_conv.transition(None).is_none());
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
        assert_eq!(conv.get_priority(), 3);
    }

    #[test]
    fn wait_correct_response() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::InternalStateResponse {
            planet_id: PLANET_ID,
            planet_state: make_dummy_planet_state(),
        });
        let result = conv.transition(Some(msg));
        assert!(result.is_none(), "Conversation should finish successfully");
    }

    #[test]
    fn wait_wrong_message() {
        let (ui_tx, _ui_rx) = unbounded::<OrchestratorToUiUpdate>();
        let (_ui_cmd_tx, ui_cmd_rx) = unbounded::<UiToOrchestratorCommand>();
        let test_ctx = make_test_context(None, None, ui_tx, ui_cmd_rx);
        let conv = make_wait_conv(test_ctx.clone());
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StartPlanetAIResult {
            planet_id: PLANET_ID,
        });
        let result = conv
            .transition(Some(wrong_msg))
            .expect("Should return ErrorState");
        assert_eq!(
            result.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
        assert!(result.transition(None).is_none());
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
                PlanetToOrchestratorKind::InternalStateResponse
            ))
        );
        assert_eq!(conv.get_priority(), 3);
    }
}
