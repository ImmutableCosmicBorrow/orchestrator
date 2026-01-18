use crate::galaxy_setup::OrchPlanSenderMap;
use crate::logging_utils::log_internal;
use crate::orchestrator::ExplorerBag;
use crate::payload;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;
use crossbeam_channel::Sender;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

pub(crate) mod orch_explorer;
pub(crate) mod orch_planet;
mod util;

///**The Conversation Trait**
///
/// This is the foundation of the FSM system. Every state in a conversation must implement this trait.
/// It defines how states identify themselves, what messages they expect, and how they transition
/// to the next state.
pub trait Conversation<T: Debug + Eq + Hash>: Send + Sync {
    /// Returns the unique ID of the conversation instance.
    fn get_id(&self) -> ID;
    /// Returns the ID of the entity (Planet or Explorer) this conversation is currently targeting.
    fn get_entity_id(&self) -> ID;
    /// Returns the specific message type this state is waiting for, if any.
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds>;
    /// Consumes the current state and a message to produce the next state in the sequence.
    /// Returns `None` to close the conversation successfully.
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<T>>,
    ) -> Option<Box<dyn Conversation<T> + Send + Sync>>;
    /// Returns the execution priority (higher values are processed first).
    fn get_priority(&self) -> i32;

    //Helper to get error strings for testing purposes
    fn get_error_details(&self) -> Option<String> {
        None
    }
}

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
pub(crate) enum PossibleMessage<T> {
    PlanetToOrch(PlanetToOrchestrator),
    ExplorerToOrch(ExplorerToOrchestrator<T>),
}

impl<T> PossibleMessage<T> {
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
    pub fn get_entity_id(&self) -> ID {
        match self {
            PossibleMessage::PlanetToOrch(msg) => msg.planet_id(),
            PossibleMessage::ExplorerToOrch(msg) => msg.explorer_id(),
        }
    }
}

// --- Communication Helpers ---

pub(crate) type SendersToPlanet = Arc<Mutex<OrchPlanSenderMap>>;
pub(crate) type SendersToExplorer = Arc<Mutex<HashMap<ID, Sender<OrchestratorToExplorer>>>>;

/// **Planet Messaging Context**
///
/// Utility struct used within states to facilitate sending messages to a specific planet.
#[derive(Clone)]
pub(crate) struct ToPlanetStruct {
    planets_senders: SendersToPlanet,
    planet_id: ID,
}

impl ToPlanetStruct {
    pub(crate) fn new(planets_senders: SendersToPlanet, planet_id: ID) -> Self {
        Self {
            planets_senders,
            planet_id,
        }
    }

    /// Sends a protocol message to the planet associated with this context.
    pub(crate) fn to_planet(&self, msg: OrchestratorToPlanet) -> Result<(), ToPlanetError> {
        let sender = {
            let lock = self.planets_senders.lock().unwrap();
            lock.get(&self.planet_id).cloned()
        };

        if let Some(s) = sender {
            s.send(msg)
                .map_err(|_| ToPlanetError::SendingMessageFailure(self.planet_id))
        } else {
            Err(ToPlanetError::SenderNotFound(self.planet_id))
        }
    }
}

pub(crate) enum ToPlanetError {
    SendingMessageFailure(ID),
    SenderNotFound(ID),
}

/// **Explorer Messaging Context**
///
/// Utility struct used within states to facilitate sending messages to a specific explorer.
#[derive(Clone)]
pub(crate) struct ToExplorerStruct {
    pub(crate) explorers_senders: SendersToExplorer,
    pub(crate) explorer_id: ID,
}

impl ToExplorerStruct {
    /// Sends a protocol message to the explorer associated with this context.
    pub(crate) fn to_explorer(&self, msg: OrchestratorToExplorer) -> Result<(), ToExplorerError> {
        let sender = {
            let lock = self.explorers_senders.lock().unwrap();
            lock.get(&self.explorer_id).cloned()
        };

        if let Some(s) = sender {
            s.send(msg)
                .map_err(|_| ToExplorerError::SendingMessageFailure(self.explorer_id))
        } else {
            Err(ToExplorerError::SenderNotFound(self.explorer_id))
        }
    }
}

pub(crate) enum ToExplorerError {
    SendingMessageFailure(ID),
    SenderNotFound(ID),
}

// --- Error Handling ---

/// **Error Reporting Interface**
///
/// Trait for converting various internal errors into human-readable logs.
trait ErrorType {
    fn stringify(&self) -> String;
}

/// **Common Orchestration Errors**
enum CommonErrorTypes {
    WrongMessage,
    PlanetSenderNotFound(ID),
    ExplorerSenderNotFound(ID),
    MessageToExplorerFailed(ID),
    MessageToPlanetFailed(ID),
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
        }
    }
}

/// **Error Sink State**
///
/// A terminal state for conversations that encounter an unrecoverable error.
/// Upon transition, it logs the error and returns `None` to purge the conversation.
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

impl Conversation<ExplorerBag> for ErrorState {
    fn get_id(&self) -> ID {
        self.id
    }
    fn get_entity_id(&self) -> ID {
        self.id
    }
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        None
    }

    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        log_internal(
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
}
