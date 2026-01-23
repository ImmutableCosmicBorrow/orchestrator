use crate::logging_utils::log_internal;
use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError, ToPlanetStruct,
};
use crate::payload;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;

///** Advertising Dead Explorer Conversation**
///
/// This module manages the process of a planet handling a dead explorer.
/// It uses a Finite State Machine (FSM) to send the request and wait for the planet's confirmation
/// of the elimination of the channel used to communicate with the dead explorer.
///
/// If successful, the conversation transitions back to a [`KillExplorersManager`] to continue
/// the killing of explores. If it fails, it transitions to an [`ErrorState`].
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

/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingOutgoingRequest`] state, which sends an
/// [`OrchestratorToPlanet::OutgoingExplorerRequest`] when the [`Conversation::transition`] method is called.
pub(crate) struct SendingDeadExpAdv {
    /// A struct containing fields to send messages to the planet.
    to_planet_struct: ToPlanetStruct,
    /// The ID of the explorer attempting to leave the planet.
    outgoing_explorer_id: ID,
}

impl SendingDeadExpAdv {
    /// Constructor for [`SendingDeadExpAdv`] state struct.
    pub(crate) fn new(to_planet_struct: ToPlanetStruct, outgoing_explorer_id: ID) -> Self {
        Self {
            to_planet_struct,
            outgoing_explorer_id,
        }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingDeadAdvResponse`] state, the conversation expects a
/// [`PlanetToOrchestrator::OutgoingExplorerResponse`] indicating that the planet has correctly eliminated the sender,
/// to the dead explorer
///
/// Depending on the response it either returns:
/// * [`ErrorState`] with [`FailedToHandleOutgoingExplorer`] if an error occurred while eliminating the channel or none to end the conversation
struct WaitingDeadAdvResponse {
    /// ID of the planet we are moving the explorer from
    planet_id: ID,
}

impl WaitingDeadAdvResponse {
    /// The constructor for [`WaitingDeadAdvResponse`] state struct.
    fn new(planet_id: ID) -> Self {
        Self { planet_id }
    }
}

/// Advertising Dead Explorer Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
pub(crate) struct AdvDeadExplorer<State> {
    /// Conversation ID.
    id: ID,
    /// Optional expected message to trigger the transition.
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM.
    state: State,
}

// SENDING OUTGOING REQUEST IMPLEMENTATION
impl Conversation<ExplorerBag> for AdvDeadExplorer<SendingDeadExpAdv> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (Some(self.state.to_planet_struct.planet_id), None)
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingDeadExpAdv`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] if the request failed to send to the planet or the sender to the planet is not found.
    ///
    /// [`AdvDeadExplorer<WaitingDeadAdvResponse>`] if the request was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::OutgoingExplorerRequest {
                explorer_id: self.state.outgoing_explorer_id,
            }) {
            Ok(()) => {
                let planet_id = self.state.to_planet_struct.planet_id;
                let state_struct = WaitingDeadAdvResponse::new(planet_id);
                let next_state =
                    AdvDeadExplorer::<WaitingDeadAdvResponse>::new(self.id, state_struct);
                Some(Box::new(next_state))
            }
            Err(err) => {
                let error = match err {
                    ToPlanetError::SendingMessageFailure(id) => {
                        CommonErrorTypes::MessageToPlanetFailed(id)
                    }
                    ToPlanetError::SenderNotFound(id) => CommonErrorTypes::PlanetSenderNotFound(id),
                };
                let error_state = ErrorState::new(Box::new(error), self.id);
                Some(Box::new(error_state))
            }
        }
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl AdvDeadExplorer<SendingDeadExpAdv> {
    /// The constructor for [`AdvDeadExplorer`] in the [`SendingDeadExpAdv`] state.
    pub(crate) fn new(id: ID, state: SendingDeadExpAdv) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING OUTGOING RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBag> for AdvDeadExplorer<WaitingDeadAdvResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (Some(self.state.planet_id), None)
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingDeadAdvResponse`] state:
    ///
    /// Returns:
    ///
    /// [`KillExplorersManager`] if the planet confirms the explorer departure.
    ///
    /// [`ErrorState`] if the planet returns an error response.
    ///
    /// Returns the manager anyway if a wrong message type is received (failsafe).
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::PlanetToOrch(
            PlanetToOrchestrator::OutgoingExplorerResponse {
                planet_id,
                explorer_id,
                res,
            },
        )) = msg_wrapped
        {
            return if res.is_ok() {
                log_internal(
                    Channel::Debug,
                    payload!(
                        action : "Planet correctly handled dead explorer, closing conversation",
                        planet_id : planet_id,
                        outgoing_explorer_id : explorer_id,
                        conversation_id : self.id,
                    ),
                );
                None
            } else {
                //Explorer is killed but channel in planet is there!
                let error = FailedToHandleOutgoingExplorer {
                    planet_id,
                    explorer_id,
                };
                let error_state = ErrorState::new(Box::new(error), self.id);
                Some(Box::new(error_state))
            };
        }

        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl AdvDeadExplorer<WaitingDeadAdvResponse> {
    /// The constructor for [`AdvDeadExplorer`] in the [`WaitingDeadAdvResponse`] state.
    pub(crate) fn new(id: ID, state: WaitingDeadAdvResponse) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::OutgoingExplorerResponse,
            )),
            state,
        }
    }
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
        let next_conv = conv
            .transition(Some(msg))
            .expect("Should return to manager");
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_error_details().is_none());
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
            .expect("Should return to manager as failsafe");
        assert!(next_conv.get_error_details().is_none());
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
