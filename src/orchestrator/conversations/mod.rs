// Allowed attributes since we need to follow macro conventions and some patterns that may trigger clippy warnings but are intentional in this design.
#![allow(
    clippy::unnecessary_wraps,
    clippy::boxed_local,
    clippy::needless_pass_by_value,
    clippy::unnecessary_box_returns
)]

use crate::logging::{LogTarget, log_internal, log_msg_to};
use crate::orchestrator::{ChannelsManagerRef, ExplorerBagContent};
use crate::payload;
use crate::ui::OrchestratorToUiUpdate;
use common_game::logging::{ActorType, Channel, EventType};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
use std::fmt::Debug;
use std::hash::Hash;
use std::time::Duration;

pub mod macros;
pub(crate) mod orch_explorer;
pub(crate) mod orch_planet;
pub(crate) mod util;

///**The Conversation Trait**
///
/// This is the foundation of the FSM system. Every state in a conversation must implement this trait.
/// It defines how states identify themselves, what messages they expect, and how they transition
/// to the next state.
pub trait Conversation: Send + Sync {
    /// Returns the unique ID of the conversation instance.
    fn get_id(&self) -> ID;
    /// Returns the tuple (`planet_id`, `explorer_id`) that represent the entities involved by the conversation. One or both may be `None`.
    fn get_entities_ids(&self) -> EntitiesIDTuple;
    /// Returns the specific message type this state is waiting for, if any.
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds>;
    /// Consumes the current state and a message to produce the next state in the sequence.
    /// Returns `None` to close the conversation successfully.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage>,
    ) -> Option<Box<dyn Conversation + Send + Sync>>;
    /// Returns the execution priority (higher values are processed first).
    fn get_priority(&self) -> i32;

    //Helper to get error strings for testing purposes
    fn get_error_details(&self) -> Option<String> {
        None
    }

    ///Used to get the explorers to kill in case a planet is killed (None in any case but in killing planet scenario)
    fn get_kill_explorers_vec(&self) -> Option<(KillExplorersList, bool)> {
        None
    }

    /// Returns the timeout duration for this conversation state.
    /// The default timeout is `TIMEOUT` and does not depend on the game step.
    /// Override this in states that should not time out, or time out after a different period.
    fn get_timeout(&self) -> Option<Duration>;

    /// Returns the type name of the conversation implementation.
    fn conversation_type(&self) -> &str {
        let full_name = std::any::type_name::<Self>();
        full_name.split("::").last().unwrap_or(full_name)
    }

    /// Returns the type name of the conversation implementation.
    fn conversation_type(&self) -> &str {
        let full_name = std::any::type_name::<Self>();
        full_name.split("::").last().unwrap_or(full_name)
    }

    /// Called when the conversation times out.
    /// Default behavior is to panic - override this to implement custom timeout handling
    /// (e.g., logging, cleanup, graceful degradation).
    fn on_timeout(self: Box<Self>) {
        log_internal(
            LogTarget::Conversations,
            Channel::Error,
            payload!(
                error : format!(
                    "Conversation type: {}, ID: {} with IDs {:?} timed out waiting for {:?}",
                    self.conversation_type(),
                    self.get_id(),
                    self.get_entities_ids(),
                    self.get_expected_kind()
                ),
                conversation_id : self.get_id(),
            ),
        );
        panic!(
            "Conversation type: {}, ID: {} timed out waiting for {:?}",
            self.conversation_type(),
            self.get_id(),
            self.get_expected_kind()
        )
    }
}

/// **Entities ID Tuple**
/// Type that indicates the entities involved in a conversation
/// Composed of two optional ID as every conversation has 0 up to 2 entities which interact
/// first ID is always a planet ID, second is an explorer ID
pub(crate) type EntitiesIDTuple = (Option<ID>, Option<ID>);

/// **Expected Message Kinds**
///
/// A wrapper for the discriminant types of both Planet and Explorer protocols.
/// Used by the Orchestrator to route incoming messages to the correct conversation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum PossibleExpectedKinds {
    PlanetToOrchKind(PlanetToOrchestratorKind),
    ExplorerToOrchKind(ExplorerToOrchestratorKind),
}

/// **Wrapped Protocol Messages**
///
/// Container for the actual data payloads received from the network.
pub(crate) enum PossibleMessage {
    PlanetToOrch(PlanetToOrchestrator),
    ExplorerToOrch(ExplorerToOrchestrator<ExplorerBagContent>),
}

impl PossibleMessage {
    /// Extracts the kind (discriminant) from the message for matching against `PossibleExpectedKinds`.
    pub fn to_kind_type(&self) -> PossibleExpectedKinds {
        match self {
            PossibleMessage::PlanetToOrch(msg) => {
                PossibleExpectedKinds::PlanetToOrchKind(Into::<PlanetToOrchestratorKind>::into(msg))
            }
            PossibleMessage::ExplorerToOrch(msg) => PossibleExpectedKinds::ExplorerToOrchKind(
                Into::<ExplorerToOrchestratorKind>::into(msg),
            ),
        }
    }

    /// Retrieves the ID of the entity that sent the message.
    pub fn get_entity_ids(&self) -> (Option<ID>, Option<ID>) {
        match self {
            PossibleMessage::PlanetToOrch(
                PlanetToOrchestrator::IncomingExplorerResponse {
                    planet_id,
                    explorer_id,
                    ..
                }
                | PlanetToOrchestrator::OutgoingExplorerResponse {
                    planet_id,
                    explorer_id,
                    ..
                },
            )
            | PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::MovedToPlanetResult {
                planet_id,
                explorer_id,
                ..
            }) => (Some(*planet_id), Some(*explorer_id)),

            PossibleMessage::ExplorerToOrch(ExplorerToOrchestrator::TravelToPlanetRequest {
                current_planet_id,
                explorer_id,
                ..
            }) => (Some(*current_planet_id), Some(*explorer_id)),
            PossibleMessage::PlanetToOrch(msg) => (Some(msg.planet_id()), None),
            PossibleMessage::ExplorerToOrch(msg) => (None, Some(msg.explorer_id())),
        }
    }
}

// --- Communication Traits ---

pub(crate) type KillExplorersList = Vec<(ID, ID)>;

pub(crate) trait ChannelsContext {
    fn get_channels_manager(&self) -> ChannelsManagerRef;
}

pub(crate) trait PlanetCommunicator: ChannelsContext {
    fn to_planet(&self, planet_id: ID, msg: OrchestratorToPlanet) -> Result<(), CommonErrorTypes> {
        let sender = {
            self.get_channels_manager()
                .get_orch_to_planet_sender(planet_id)
        };

        if let Some(s) = sender {
            log_msg_to(
                LogTarget::Conversations,
                Channel::Trace,
                EventType::MessageOrchestratorToPlanet,
                (ActorType::Planet, planet_id),
                payload!(
                        message : format!("{:?}", msg)
                ),
            );
            s.send(msg)
                .map_err(|_| CommonErrorTypes::MessageToPlanetFailed(planet_id))
        } else {
            Err(CommonErrorTypes::PlanetSenderNotFound(planet_id))
        }
    }
}

pub(crate) trait ExplorerCommunicator: ChannelsContext {
    fn to_explorer(
        &self,
        explorer_id: ID,
        msg: OrchestratorToExplorer,
    ) -> Result<(), CommonErrorTypes> {
        let sender = {
            self.get_channels_manager()
                .get_orch_to_explorer_sender(explorer_id)
        };

        if let Some(s) = sender {
            log_msg_to(
                LogTarget::Conversations,
                Channel::Trace,
                EventType::MessageOrchestratorToExplorer,
                (ActorType::Explorer, explorer_id),
                payload!( message : format!("{:?}", msg) ),
            );
            s.send(msg)
                .map_err(|_| CommonErrorTypes::MessageToExplorerFailed(explorer_id))
        } else {
            Err(CommonErrorTypes::ExplorerSenderNotFound(explorer_id))
        }
    }
}

pub(crate) trait UiCommunicator: ChannelsContext {
    fn to_ui(&self, msg: OrchestratorToUiUpdate) -> Result<(), CommonErrorTypes> {
        let sender = self.get_channels_manager().get_ui_sender();
        sender
            .send(msg)
            .map_err(|_| CommonErrorTypes::MessageToUiFailed)
    }
}
//Implement PlanetCommunicator for every type that implements ChannelsContext using default implementation
impl<T: ChannelsContext> PlanetCommunicator for T {}

//Implement ExplorerCommunicator for every type that implements ChannelsContext using default implementation
impl<T: ChannelsContext> ExplorerCommunicator for T {}

impl<T: ChannelsContext> UiCommunicator for T {}
pub(crate) enum ToPlanetError {
    SendingMessageFailure(ID),
    SenderNotFound(ID),
}

pub(crate) enum ToExplorerError {
    SendingMessageFailure(ID),
}

pub(crate) enum ToUiError {
    SendingMessageFailure,
    SenderNotFound,
}
// --- Error Handling ---

/// **Error Reporting Interface**
///
/// Trait for converting various internal errors into human-readable logs.
trait ErrorType {
    fn stringify(&self) -> String;
}

/// **Common Orchestration Errors**
pub(crate) enum CommonErrorTypes {
    WrongMessage,
    PlanetSenderNotFound(ID),
    ExplorerSenderNotFound(ID),
    MessageToExplorerFailed(ID),
    MessageToPlanetFailed(ID),
    MessageToUiFailed,
}
impl ErrorType for CommonErrorTypes {
    fn stringify(&self) -> String {
        match self {
            CommonErrorTypes::WrongMessage => "Wrong Message Received".to_string(),
            CommonErrorTypes::PlanetSenderNotFound(id) => {
                format!("sender to planet {id} not found")
            }
            CommonErrorTypes::ExplorerSenderNotFound(id) => {
                format!("sender to explorer {id} not found")
            }
            CommonErrorTypes::MessageToExplorerFailed(id) => {
                format!("failed to send message to explorer {id}")
            }
            CommonErrorTypes::MessageToPlanetFailed(id) => {
                format!("failed to send message to planet {id}")
            }
            CommonErrorTypes::MessageToUiFailed => "failed to send message to ui".to_string(),
        }
    }
}

/// **Timeout Error**
///
/// Error type for when a conversation times out waiting for a message.
pub(crate) struct TimeoutErrorType {
    pub(crate) conversation_id: ID,
    pub(crate) entity_ids: (Option<ID>, Option<ID>),
    pub(crate) expected_kind: Option<PossibleExpectedKinds>,
}

impl TimeoutErrorType {
    pub(crate) fn new(
        conversation_id: ID,
        entity_ids: (Option<ID>, Option<ID>),
        expected_kind: Option<PossibleExpectedKinds>,
    ) -> Self {
        Self {
            conversation_id,
            entity_ids,
            expected_kind,
        }
    }
}

impl ErrorType for TimeoutErrorType {
    fn stringify(&self) -> String {
        format!(
            "Conversation {} timed out waiting for {:?} (entities: {:?})",
            self.conversation_id, self.expected_kind, self.entity_ids
        )
    }
}

/// **Error State**
///
/// A terminal state for conversations that encounter an unrecoverable error.
/// Upon transition, it logs the error and returns `None` to end the conversation.
struct ErrorState {
    error: Box<dyn ErrorType + Send + Sync>,
    id: ID,
    expected_message: Option<PossibleExpectedKinds>,
}

impl ErrorState {
    fn new(error: Box<dyn ErrorType + Send + Sync>, id: ID) -> Self {
        Self {
            error,
            id,
            expected_message: None,
        }
    }
}

impl Conversation for ErrorState {
    fn get_id(&self) -> ID {
        self.id
    }
    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (None, None)
    }
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        None
    }

    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage>,
    ) -> Option<Box<dyn Conversation + Send + Sync>> {
        log_internal(
            LogTarget::Conversations,
            Channel::Warning,
            payload!(
                warning : "A Conversation reached an error and will be closed.",
                conversation_id : self.id,
                error : self.error.stringify(),
            ),
        );
        None
    }

    fn get_priority(&self) -> i32 {
        5
    }

    fn get_error_details(&self) -> Option<String> {
        Some(self.error.stringify())
    }

    fn get_timeout(&self) -> Option<Duration> {
        None
    }
}
