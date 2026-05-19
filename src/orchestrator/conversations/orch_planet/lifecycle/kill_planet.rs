use crate::convo_manager::OrchContextRef;
use crate::globals::TIMEOUT;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ChannelsManagerRef;
use crate::orchestrator::Duration;
use crate::orchestrator::conversations::EntitiesIDTuple;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    ChannelsContext, CommonErrorTypes, Conversation, ErrorState, KillExplorersList,
    PlanetCommunicator, PossibleExpectedKinds, PossibleMessage,
};
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::KillPlanetResult;
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::utils::ID;

//**Kill Planet Conversation**
//
// This module manages the complex process of destroying a planet.
// It uses an FSM to send the kill command, wait for confirmation, and then
// sends via its method [`Conversation::get_kill_explorers_vec`] the IDs of the explorers on the planet
// so that the Orchestrator can kill them
//
//
// The conversation starts in the [`SendPlanetKill`] state, which sends an
// [`OrchestratorToPlanet::KillPlanet`] message when the [`Conversation::transition`] method is called.
// --- INTERNAL STATE CONVERSATION ---

define_conversation!(
    name: KillPlanetConversation
);

// --- SEND KILL PLANET DEFINITION ---

create_request_state!(
    state_name: SendPlanetKill,
    conv_name: KillPlanetConversation,
    priority: 5,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: {
        planet_id: ID,
    },
    entities_id_fn: |this: &KillPlanetConversation<SendPlanetKill>| { (Some(this.state.planet_id), None) },
    transition_fn: send_kill_planet_transition,
    methods_settings: {

    },
);

/// Transition Function for [`SendPlanetKill`] state:
///
/// Returns:
///
/// [`ErrorState`] if the message to the planet fails or the sender is not found.
///
/// [`KillPlanetConversation<WaitingPlanetKillResult>`] if the kill command was sent successfully.
fn send_kill_planet_transition(
    this: Box<KillPlanetConversation<SendPlanetKill>>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this
        .state
        .to_planet(this.state.planet_id, OrchestratorToPlanet::KillPlanet)
    {
        Ok(()) => {
            let state_struct =
                WaitingPlanetKillResult::new(this.state.orch_context, this.state.planet_id);
            let next_state =
                KillPlanetConversation::<WaitingPlanetKillResult>::new(this.id, state_struct);
            Some(Box::new(next_state))
        }
        Err(err) => {
            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAITING KILL PLANET RESPONSE DEFINITION ---

create_response_state!(
    state: WaitingPlanetKillResult,
    conv: KillPlanetConversation,
    priority: 5,
    timeout: Some(TIMEOUT),
    expected_msg: PlanetToOrchKind(KillPlanetResult),
    fields: {
        planet_id: ID,
    },
    entities_id_closure: |this: &KillPlanetConversation<WaitingPlanetKillResult>| { (Some(this.state.planet_id), None) },
    transition: wait_planet_kill_res_transition,
    methods_settings: {
        get_kill_exp_vec: |this: &KillPlanetConversation<WaitingPlanetKillResult>| { Some((this.state.get_explorers_in_planet(this.state.planet_id), false)) }
    },
);

impl WaitingPlanetKillResult {
    //Helper function to find all explorers in the planet
    fn get_explorers_in_planet(&self, target_planet: ID) -> Vec<(ID, ID)> {
        self.orch_context
            .explorers_location
            .iter()
            .filter(|r| *r.value() == target_planet) // Access value via the guard
            .map(|r| (*r.key(), *r.value())) // Dereference/copy the data
            .collect()
    }
}

/// Transition Function for [`WaitingPlanetKillResult`] state:
///
/// Returns:
///
/// None to end the conversation if planet is killed correctly - NOTE: to kill explorers on this planet, we return the list of them
/// through the dedicated method of the trait and let the Orchestrator take care of that
///
/// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different then the expected [`PlanetToOrchestrator::KillPlanetResult`]
fn wait_planet_kill_res_transition(
    this: Box<KillPlanetConversation<WaitingPlanetKillResult>>,
    msg: Option<PossibleMessage>,
) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::KillPlanetResult {
        planet_id,
    })) = msg
    {
        log_internal(
            LogTarget::Conversations,
            Channel::Info,
            payload!(
                action : "Killed Planet",
                planet_id : planet_id,
                conversation_id : this.id
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

    const CONV_ID: ID = 100;
    const PLANET_ID: ID = 200;
    const EXPLORER_ID_1: ID = 301;
    const EXPLORER_ID_2: ID = 302;

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

    fn make_empty_explorer_refs() -> (ExplorersLocationRef, OrchToExplorerSenders) {
        (
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashMap::new())),
        )
    }

    fn make_send_conv(senders: PlanetSenders) -> Box<KillPlanetConversation<SendPlanetKill>> {
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let (explorers_location, explorers_senders) = make_empty_explorer_refs();
        let state = SendPlanetKill::new(to_planet, explorers_location, explorers_senders);
        Box::new(KillPlanetConversation::<SendPlanetKill>::new(
            CONV_ID, state,
        ))
    }

    fn make_wait_conv() -> Box<KillPlanetConversation<WaitingPlanetKillResult>> {
        let (explorers_location, explorers_senders) = make_empty_explorer_refs();
        let planet_senders = make_empty_senders();
        let state = WaitingPlanetKillResult::new(
            PLANET_ID,
            explorers_location,
            explorers_senders,
            planet_senders,
        );
        Box::new(KillPlanetConversation::<WaitingPlanetKillResult>::new(
            CONV_ID, state,
        ))
    }

    fn make_wait_conv_with_explorers() -> Box<KillPlanetConversation<WaitingPlanetKillResult>> {
        let explorers_location = Arc::new(Mutex::new(HashMap::from([
            (EXPLORER_ID_1, PLANET_ID),
            (EXPLORER_ID_2, PLANET_ID),
            (999, 888), // Explorer on a different planet (should be ignored)
        ])));
        let explorers_senders = Arc::new(Mutex::new(HashMap::new()));
        let planet_senders = make_empty_senders();
        let state = WaitingPlanetKillResult::new(
            PLANET_ID,
            explorers_location,
            explorers_senders,
            planet_senders,
        );
        Box::new(KillPlanetConversation::<WaitingPlanetKillResult>::new(
            CONV_ID, state,
        ))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let MakeSendersResult(senders, _rx) = make_senders_with(PLANET_ID);
        let conv = make_send_conv(senders);
        let next_conv = conv
            .transition(None)
            .expect("Should transition to WaitingPlanetKillResult");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PlanetToOrchKind(KillPlanetResult))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn send_missing_sender() {
        let senders = make_empty_senders();
        let conv = make_send_conv(senders);
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
        let (explorers_location, explorers_senders) = make_empty_explorer_refs();
        let state = SendPlanetKill::new(to_planet, explorers_location, explorers_senders);
        let conv = KillPlanetConversation::<SendPlanetKill>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 5);
    }

    #[test]
    fn wait_success_and_cleanup() {
        let conv = make_wait_conv_with_explorers();
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::KillPlanetResult {
            planet_id: PLANET_ID,
        });
        let next_conv = conv.transition(Some(msg));
        // After planet kill, the conversation should end (return None)
        assert!(
            next_conv.is_none(),
            "Conversation should end and return None"
        );
    }

    #[test]
    fn wait_wrong_message() {
        let conv = make_wait_conv();
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::AsteroidAck {
            planet_id: PLANET_ID,
            rocket: None,
        });
        let next_conv = conv
            .transition(Some(wrong_msg))
            .expect("Should return an ErrorState");
        assert_eq!(
            next_conv.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }

    #[test]
    fn wait_getters() {
        let (explorers_location, explorers_senders) = make_empty_explorer_refs();
        let planet_senders = make_empty_senders();
        let state = WaitingPlanetKillResult::new(
            PLANET_ID,
            explorers_location,
            explorers_senders,
            planet_senders,
        );
        let conv = KillPlanetConversation::<WaitingPlanetKillResult>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PlanetToOrchKind(KillPlanetResult))
        );
        assert_eq!(conv.get_priority(), 5);
    }
}
*/
