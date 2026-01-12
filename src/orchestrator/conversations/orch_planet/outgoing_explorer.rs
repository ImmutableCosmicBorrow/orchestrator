use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::orch_explorer::kill_explorers_manager::KillExplorersManager;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError, ToPlanetStruct,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;

///**Outgoing Explorer Conversation**
///
/// This module manages the process of a planet handling an outgoing explorer.
/// It uses a Finite State Machine (FSM) to send the request and wait for the planet's confirmation.
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
            "Planet {} failed to handle outgoing explorer {}",
            self.planet_id, self.explorer_id
        )
    }
}

/// Marker struct for FSM state
///
/// The conversation starts in the [`SendingOutgoingRequest`] state, which sends an
/// [`OrchestratorToPlanet::OutgoingExplorerRequest`] when the [`Conversation::transition`] method is called.
pub(crate) struct SendingOutgoingRequest {
    /// A struct containing fields to send messages to the planet.
    to_planet_struct: ToPlanetStruct,
    /// The ID of the explorer attempting to leave the planet.
    outgoing_explorer_id: ID,
    /// The manager to return to after the request is processed.
    kill_explorers_manager: Box<KillExplorersManager>,
}

impl SendingOutgoingRequest {
    /// Constructor for [`SendingOutgoingRequest`] state struct.
    pub(crate) fn new(
        to_planet_struct: ToPlanetStruct,
        outgoing_explorer_id: ID,
        kill_explorers_manager: Box<KillExplorersManager>,
    ) -> Self {
        Self {
            to_planet_struct,
            outgoing_explorer_id,
            kill_explorers_manager,
        }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingOutgoingResponse`] state, the conversation expects a
/// [`PlanetToOrchestrator::OutgoingExplorerResponse`]. Depending on the response result,
/// it either returns to the manager or enters an error state.
struct WaitingOutgoingResponse {
    /// ID of the planet we are moving the explorer from
    planet_id: ID,
    /// The manager to return to upon successful confirmation.
    kill_explorers_manager: Box<KillExplorersManager>,
}

impl WaitingOutgoingResponse {
    /// The constructor for [`WaitingOutgoingResponse`] state struct.
    fn new(planet_id: ID, kill_explorers_manager: Box<KillExplorersManager>) -> Self {
        Self {
            planet_id,
            kill_explorers_manager,
        }
    }
}

/// Outgoing Explorer Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
pub(crate) struct OutgoingExplorer<State> {
    /// Conversation ID.
    id: ID,
    /// Optional expected message to trigger the transition.
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM.
    state: State,
}

// SENDING OUTGOING REQUEST IMPLEMENTATION
impl Conversation<ExplorerBag> for OutgoingExplorer<SendingOutgoingRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_planet_struct.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendingOutgoingRequest`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] if the request failed to send to the planet or the sender to the planet is not found.
    ///
    /// [`OutgoingExplorer<WaitingOutgoingResponse>`] if the request was sent successfully.
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
                let state_struct =
                    WaitingOutgoingResponse::new(planet_id, self.state.kill_explorers_manager);
                let next_state =
                    OutgoingExplorer::<WaitingOutgoingResponse>::new(self.id, state_struct);
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

impl OutgoingExplorer<SendingOutgoingRequest> {
    /// The constructor for [`OutgoingExplorer`] in the [`SendingOutgoingRequest`] state.
    pub(crate) fn new(id: ID, state: SendingOutgoingRequest) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING OUTGOING RESPONSE IMPLEMENTATION
impl Conversation<ExplorerBag> for OutgoingExplorer<WaitingOutgoingResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingOutgoingResponse`] state:
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
                println!(
                    "Planet {planet_id} correctly handled outgoing explorer {explorer_id}, going back to manager"
                );
                Some(self.state.kill_explorers_manager)
            } else {
                let error = FailedToHandleOutgoingExplorer {
                    planet_id,
                    explorer_id,
                };
                let error_state = ErrorState::new(Box::new(error), self.id);
                Some(Box::new(error_state))
            };
        }

        // Wrong Message, return to manager as fallback
        Some(self.state.kill_explorers_manager)
    }

    fn get_priority(&self) -> i32 {
        4
    }
}

impl OutgoingExplorer<WaitingOutgoingResponse> {
    /// The constructor for [`OutgoingExplorer`] in the [`WaitingOutgoingResponse`] state.
    pub(crate) fn new(id: ID, state: WaitingOutgoingResponse) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(
                PlanetToOrchestratorKind::OutgoingExplorerResponse,
            )),
            state,
        }
    }
}