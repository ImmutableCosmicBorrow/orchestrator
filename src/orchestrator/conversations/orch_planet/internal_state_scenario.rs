use crate::logging_utils::log_internal;
use crate::orchestrator::ExplorerBagContent;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError, ToPlanetStruct,
};
use crate::payload;
use crate::ui::OrchestratorToUiUpdate;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
use crossbeam_channel::Sender;
///**Internal State Conversation**
///
/// This module manages the conversation between the Orchestrator and a Planet regarding its internal state.
/// It uses a Finite State Machine (FSM) to ensure that requests and responses are handled in the correct
/// order at compile time.
///
/// The conversation flow starts by sending a request and terminates once the planet's state
/// is received (intended for UI reporting).
///
/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingInternalStateRequest`] state, which sends an
/// [`OrchestratorToPlanet::InternalStateRequest`] when the [`Conversation::transition`] method is called.
pub struct SendingInternalStateRequest {
    /// A struct containing fields to send messages to the indicated planet
    to_planet_struct: ToPlanetStruct,
    /// Optional sender to forward planet state to UI
    ui_sender: Option<Sender<OrchestratorToUiUpdate>>,
}

impl SendingInternalStateRequest {
    /// Constructor for [`SendingInternalStateRequest`] state struct
    pub fn new(
        to_planet_struct: ToPlanetStruct,
        ui_sender: Option<Sender<OrchestratorToUiUpdate>>,
    ) -> Self {
        Self {
            to_planet_struct,
            ui_sender,
        }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingInternalStateResponse`] state, the conversation expects a
/// [`PlanetToOrchestrator::InternalStateResponse`] message to complete the state retrieval.
struct WaitingInternalStateResponse {
    /// ID of the planet we are waiting for
    planet_id: ID,
    /// Optional sender to forward planet state to UI
    ui_sender: Option<Sender<OrchestratorToUiUpdate>>,
}

impl WaitingInternalStateResponse {
    /// The constructor for [`WaitingInternalStateResponse`] state struct
    fn new(planet_id: ID, ui_sender: Option<Sender<OrchestratorToUiUpdate>>) -> Self {
        Self {
            planet_id,
            ui_sender,
        }
    }
}

/// Generic FSM struct for Internal State requests
pub struct InternalStateConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the conversation
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SENDING INTERNAL STATE REQUEST IMPLEMENTATION
impl Conversation<ExplorerBagContent> for InternalStateConversation<SendingInternalStateRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (Some(self.state.to_planet_struct.planet_id), None)
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingInternalStateRequest`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::MessageToPlanetFailed`] if the message has not been correctly sent to the planet
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::PlanetSenderNotFound`] if the sender to the planet is not in the [`SendersToPlanet`] list
    ///
    /// The next state: [`InternalStateConversation<WaitingInternalStateResponse>`] if the request was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::InternalStateRequest)
        {
            Ok(()) => {
                let next_state = InternalStateConversation::<WaitingInternalStateResponse>::new(
                    self.id,
                    self.state.to_planet_struct.planet_id,
                    self.state.ui_sender.clone(),
                );
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
                Some(Box::new(error_state)
                    as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
            }
        }
    }

    fn get_priority(&self) -> i32 {
        3
    }
}

impl InternalStateConversation<SendingInternalStateRequest> {
    pub fn new(id: ID, state: SendingInternalStateRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBagContent> for InternalStateConversation<WaitingInternalStateResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (Some(self.state.planet_id), None)
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingInternalStateResponse`] state:
    ///
    /// Returns:
    ///
    /// [None] if the state is successfully received and sent to the UI, closing the conversation
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different from the expected one [`PlanetToOrchestrator::InternalStateResponse`]
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::InternalStateResponse {
            planet_id,
            planet_state,
        })) = msg_wrapped
        {
            // Send planet state to UI if sender is available
            if let Some(ref sender) = self.state.ui_sender {
                let _ = sender.send(OrchestratorToUiUpdate::PlanetSnapshot(
                    planet_id,
                    planet_state.clone(),
                ));
            }

            println!(" --------- ID:{:?} {:?}", planet_id, planet_state.clone());

            log_internal(
                Channel::Debug,
                payload!(
                    action : "Planet sent its internal state",
                    planet_id : planet_id,
                    planet_state : format!("{planet_state:?}"),
                    conversation_id : self.id
                ),
            );
            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
    }

    fn get_priority(&self) -> i32 {
        3
    }
}

impl InternalStateConversation<WaitingInternalStateResponse> {
    fn new(id: ID, planet_id: ID, ui_sender: Option<Sender<OrchestratorToUiUpdate>>) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::InternalStateResponse,
            )),
            state: WaitingInternalStateResponse::new(planet_id, ui_sender),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_game::components::planet::DummyPlanetState;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: ID = 1;
    const PLANET_ID: ID = 2;

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
    fn make_send_conv(
        senders: PlanetSenders,
    ) -> Box<InternalStateConversation<SendingInternalStateRequest>> {
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let state = SendingInternalStateRequest::new(to_planet, None);
        Box::new(InternalStateConversation::<SendingInternalStateRequest>::new(CONV_ID, state))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv() -> Box<InternalStateConversation<WaitingInternalStateResponse>> {
        Box::new(
            InternalStateConversation::<WaitingInternalStateResponse>::new(
                CONV_ID, PLANET_ID, None,
            ),
        )
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
        let MakeSendersResult(senders, _rx) = make_senders_with(PLANET_ID);
        let conv = make_send_conv(senders);
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
        let senders = make_empty_senders();
        let conv = make_send_conv(senders);
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
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();
        drop(rx);
        let senders = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx)])));
        let conv = make_send_conv(senders);
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
        let MakeSendersResult(senders, _rx) = make_senders_with(PLANET_ID);
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let state = SendingInternalStateRequest::new(to_planet, None);
        let conv = InternalStateConversation::<SendingInternalStateRequest>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 3);
    }

    #[test]
    fn wait_correct_response() {
        let conv = make_wait_conv();
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::InternalStateResponse {
            planet_id: PLANET_ID,
            planet_state: make_dummy_planet_state(),
        });
        let result = conv.transition(Some(msg));
        assert!(result.is_none(), "Conversation should finish successfully");
    }

    #[test]
    fn wait_wrong_message() {
        let conv = make_wait_conv();
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
        let conv = InternalStateConversation::<WaitingInternalStateResponse>::new(
            CONV_ID, PLANET_ID, None,
        );
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
