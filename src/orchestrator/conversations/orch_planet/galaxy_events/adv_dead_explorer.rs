use crate::orchestrator::conversations::EntitiesIDTuple;
use std::time::Duration;
use crate::logging_utils::{LogTarget, log_internal};
use crate::orchestrator::{ChannelsManagerRef, ExplorerBagContent};
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{ChannelsContext, CommonErrorTypes, Conversation, ErrorState, ErrorType, PlanetCommunicator, PlanetContext, PossibleExpectedKinds, PossibleMessage, ToPlanetError, ToPlanetStruct};
use crate::{create_request_state, create_response_state, define_conversation, payload};
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
use crate::globals::TIMEOUT;
use crate::orchestrator::conversations::orch_planet::galaxy_events::sunray_scenario::WaitingSunrayAck;

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
    priority: 4,
    timeout: Some(TIMEOUT),
    expected_msg: None,
    fields: { 
        channels_manager: ChannelsManagerRef,
        planet_id: ID,
        dead_explorer_id: ID,
    },
    entities_id_fn: |this: &AdvDeadExplorer<SendingDeadExpAdv>| {(Some(this.state.planet_id), None)},
    transition_fn: send_dead_exp_adv_transition,
    methods_settings: { },
);

impl PlanetContext for SendingDeadExpAdv {
    fn get_planet_id(&self) -> ID {
        self.planet_id
    }
}

impl ChannelsContext for SendingDeadExpAdv {
    fn get_channels_manager(&self) -> &ChannelsManagerRef {
        &self.channels_manager
    }
}


/// Transition Function for [`SendingDeadExpAdv`] state:
///
/// Returns:
///
/// [`ErrorState`] if the request failed to send to the planet or the sender to the planet is not found.
///
/// [`AdvDeadExplorer<WaitingDeadAdvResponse>`] if the request was sent successfully.
fn send_dead_exp_adv_transition(this: Box<AdvDeadExplorer<SendingDeadExpAdv>>) -> Option<Box<dyn Conversation + Send + Sync>> {
    match this
        .state
        .to_planet(OrchestratorToPlanet::OutgoingExplorerRequest {
            explorer_id: this.state.planet_id,
        }) {
        Ok(()) => {
            let planet_id = this.state.planet_id;
            let state_struct = WaitingDeadAdvResponse::new(planet_id);
            let next_state =
                AdvDeadExplorer::<WaitingDeadAdvResponse>::new(this.id, state_struct);
            Some(Box::new(next_state))
        }
        Err(err) => {

            let error_state = ErrorState::new(Box::new(err), this.id);
            Some(Box::new(error_state)
                as Box<dyn Conversation + Send + Sync>)
        }
    }
}

// --- WAITING DEAD EXPLORER STATE DEFINITION ---

create_response_state!(
    state: WaitingDeadAdvResponse,
    conv: AdvDeadExplorer,
    priority: 1,
    timeout: Some(TIMEOUT),
    expected_msg: PlanetToOrchKind(PlanetToOrchestratorKind::OutgoingExplorerResponse),
    fields: {
        planet_id: ID,
    },
    entities_id_closure: |this: &AdvDeadExplorer<WaitingDeadAdvResponse>| { (Some(this.state.planet_id), None) },
    transition: wait_dead_adv_response_transition,
    methods_settings: {},
);

impl PlanetContext for WaitingDeadAdvResponse {
    fn get_planet_id(&self) -> ID {
        self.planet_id
    }
}


/// Transition Function for [`WaitingDeadAdvResponse`] state:
///
/// Returns:
///
/// [None] if the planet confirms the explorer departure, closing the conversation.
///
/// [`ErrorState`] if the planet returns an error response.
fn wait_dead_adv_response_transition(this: Box<AdvDeadExplorer<WaitingDeadAdvResponse>>, msg: Option<PossibleMessage>) -> Option<Box<dyn Conversation + Send + Sync>> {
    if let Some(PossibleMessage::PlanetToOrch(
                    PlanetToOrchestrator::OutgoingExplorerResponse {
                        planet_id,
                        explorer_id,
                        res,
                    },
                )) = msg
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
            Some(Box::new(error_state)
                as Box<dyn Conversation + Send + Sync>)
        };
    }

    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), this.id);
    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: ID = 10;
    const PLANET_ID: ID = 20;
    const EXPLORER_ID: ID = 30;

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

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(senders: PlanetSenders) -> Box<AdvDeadExplorer<SendingDeadExpAdv>> {
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let state = SendingDeadExpAdv::new(to_planet, EXPLORER_ID);
        Box::new(AdvDeadExplorer::<SendingDeadExpAdv>::new(CONV_ID, state))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv() -> Box<AdvDeadExplorer<WaitingDeadAdvResponse>> {
        let state = WaitingDeadAdvResponse::new(PLANET_ID);
        Box::new(AdvDeadExplorer::<WaitingDeadAdvResponse>::new(
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
        let state = SendingDeadExpAdv::new(to_planet, EXPLORER_ID);
        let conv = AdvDeadExplorer::<SendingDeadExpAdv>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 4);
    }

    #[test]
    fn wait_success() {
        let conv = make_wait_conv();
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
        let conv = make_wait_conv();
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
        let state = WaitingDeadAdvResponse::new(PLANET_ID);
        let conv = AdvDeadExplorer::<WaitingDeadAdvResponse>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::OutgoingExplorerResponse
            ))
        );
        assert_eq!(conv.get_priority(), 4);
    }

    #[test]
    fn wait_error_response() {
        let conv = make_wait_conv();
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
