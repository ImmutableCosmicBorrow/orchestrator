use crate::convo_manager::OrchContextRef;
use crate::globals::TIMEOUT;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::Duration;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, PlanetCommunicator,
    PossibleExpectedKinds, PossibleMessage,
};
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;

//**Start Planet Conversation**
//
// This module manages the conversation between the Orchestrator and a Planet regarding the activation of its AI.
// It uses a Finite State Machine (FSM) to ensure that the start command and the confirmation result
// are handled in the correct order at compile time.
//
// The conversation flow starts by sending a start request and terminates once the planet
// confirms the AI has started.
// Marker struct for FSM state
//
// In the [`WaitingPlanetStartResult`] state, the conversation expects a
// [`PlanetToOrchestrator::StartPlanetAIResult`] message to confirm the planet has successfully initialized its AI.
// --- START PLANET CONVERSATION ---

define_conversation!(
    name: StartPlanetConversation
);

// --- SENDING PLANET START STATE DEFINITION ---

create_request_state!(
    state_name: SendingPlanetStart,
    conv_name: StartPlanetConversation,
    priority: 5,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        planet_id: ID,
    },
    entities_id_fn: |this: &StartPlanetConversation<SendingPlanetStart>| { (Some(this.state.planet_id), None) },
    transition_fn: send_planet_start_transition,
    methods_settings: {

    },
);

fn send_planet_start_transition(
    this: Box<StartPlanetConversation<SendingPlanetStart>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this
        .state
        .to_planet(this.state.planet_id, OrchestratorToPlanet::StartPlanetAI)
    {
        Ok(()) => {
            let next_state =
                WaitingPlanetStartResult::new(this.state.orch_context, this.state.planet_id);
            let next_conv =
                StartPlanetConversation::<WaitingPlanetStartResult>::new(this.id, next_state);
            Some(Box::new(next_conv))
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAIT START PLANET RESULT STATE DEFINITION ---

create_response_state!(
    state: WaitingPlanetStartResult,
    conv: StartPlanetConversation,
    priority: 5,
    timeout: Some(TIMEOUT),
    expected_msg: PlanetToOrchKind(PlanetToOrchestratorKind::StartPlanetAIResult),
    fields: {
        planet_id: ID
    },
    entities_id_closure: |this: &StartPlanetConversation<WaitingPlanetStartResult>| { (Some(this.state.planet_id), None) },
    transition: wait_planet_start_res_transition,
    methods_settings: {

    },
);

/// Transition Function for [`WaitingPlanetStartResult`] state:
///
/// Returns:
///
/// [None] if the start result is successfully received, ending the conversation.
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different from [`PlanetToOrchestrator::StartPlanetAIResult`]
fn wait_planet_start_res_transition(
    this: Box<StartPlanetConversation<WaitingPlanetStartResult>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StartPlanetAIResult {
        planet_id,
    })) = msg
    {
        log_internal(
            LogTarget::Conversations,
            Channel::Info,
            payload!(
                action : "Started Planet, closing conversation",
                planet_id : planet_id,
                conversation_id : this.id,
            ),
        );
        return None;
    }

    //Wrong Message, close conversation
    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: u32 = 1;
    const PLANET_ID: u32 = 2;

    type PlanetSenders = Arc<Mutex<HashMap<ID, crossbeam_channel::Sender<OrchestratorToPlanet>>>>;

    struct MakeSendersResult(
        PlanetSenders,
        crossbeam_channel::Receiver<OrchestratorToPlanet>,
    );

    // --- Helper functions ---
    fn make_senders_with(planet_id: ID) -> MakeSendersResult {
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();
        MakeSendersResult(Arc::new(Mutex::new(HashMap::from([(planet_id, tx)]))), rx)
    }

    fn make_empty_senders() -> PlanetSenders {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn make_to_planet_struct(planet_id: ID, senders: PlanetSenders) -> ToPlanetStruct {
        ToPlanetStruct {
            planet_id,
            planets_senders: senders,
        }
    }

    fn make_send_conv(senders: PlanetSenders) -> Box<StartPlanetConversation<SendingPlanetStart>> {
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let state = SendingPlanetStart::new(to_planet);
        Box::new(StartPlanetConversation::<SendingPlanetStart>::new(
            CONV_ID, state,
        ))
    }

    fn make_wait_conv() -> Box<StartPlanetConversation<WaitingPlanetStartResult>> {
        Box::new(StartPlanetConversation::<WaitingPlanetStartResult>::new(
            CONV_ID, PLANET_ID,
        ))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let MakeSendersResult(senders, _rx) = make_senders_with(PLANET_ID);
        let conv = make_send_conv(senders);
        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PlanetToOrchKind(StartPlanetAIResult))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn send_missing_sender() {
        let senders = make_empty_senders();
        let conv = make_send_conv(senders);
        let next_conv = conv
            .transition(None)
            .expect("Should transition to next state");
        assert!(next_conv.get_expected_kind().is_none());
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(
            next_conv.get_error_details(),
            Some(format!("sender to planet {PLANET_ID} not found"))
        );
    }

    #[test]
    fn send_message_failure() {
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();
        drop(rx);
        let senders = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx)])));
        let conv = make_send_conv(senders);
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
        let MakeSendersResult(senders, _rx) = make_senders_with(PLANET_ID);
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let state = SendingPlanetStart::new(to_planet);
        let conv = StartPlanetConversation::<SendingPlanetStart>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 5);
    }

    #[test]
    fn wait_correct_message() {
        let conv = make_wait_conv();
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::StartPlanetAIResult {
            planet_id: PLANET_ID,
        });
        let result = conv.transition(Some(msg));
        assert!(
            result.is_none(),
            "Conversation should terminate successfully (None)"
        );
    }

    #[test]
    fn wait_wrong_message() {
        let conv = make_wait_conv();
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::SunrayAck {
            planet_id: PLANET_ID,
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
        let conv = StartPlanetConversation::<WaitingPlanetStartResult>::new(CONV_ID, PLANET_ID);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PlanetToOrchKind(StartPlanetAIResult))
        );
        assert_eq!(conv.get_priority(), 5);
    }
}

*/
