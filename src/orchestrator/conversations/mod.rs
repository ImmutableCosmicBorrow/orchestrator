use crate::galaxy_setup::OrchPlanSenderMap;
use crate::orchestrator::ExplorerBag;
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
use std::sync::{Arc, Mutex};

pub(crate) mod orch_explorer;
pub(crate) mod orch_planet;

pub(crate) trait Conversation<T: Debug>: Send + Sync {
    fn get_id(&self) -> ID;
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds>;
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<T>>,
    ) -> Option<Box<dyn Conversation<T> + Send + Sync>>;
}

#[derive(Debug, Clone)]
pub(crate) enum PossibleExpectedKinds {
    PlanetToOrchKind(PlanetToOrchestratorKind),
    ExplorerToOrchKind(ExplorerToOrchestratorKind),
}

pub(crate) enum PossibleMessage<T> {
    PlanetToOrch(PlanetToOrchestrator),
    ExplorerToOrch(ExplorerToOrchestrator<T>),
}

pub(crate) type SendersToPlanet = Arc<Mutex<OrchPlanSenderMap>>;
pub(crate) type SendersToExplorer = Arc<Mutex<HashMap<ID, Sender<OrchestratorToExplorer>>>>;
pub(crate) type ExplorersBagRef<T> = Arc<HashMap<ID, T>>;
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

    pub(crate) fn to_planet(&self, msg: OrchestratorToPlanet) -> Result<(), ToPlanetError> {
        let sender = {
            let lock = self.planets_senders.lock().unwrap();
            lock.get(&self.planet_id).cloned() // Clone the Sender handle
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

impl ToPlanetError {
    fn get_id(&self) -> ID {
        match self {
            Self::SendingMessageFailure(id) | Self::SenderNotFound(id) => *id,
        }
    }
}

pub(crate) enum ToExplorerError {
    SendingMessageFailure(ID),
    SenderNotFound(ID),
}

pub(crate) struct ToExplorerStruct {
    pub(crate) explorers_senders: SendersToExplorer,
    pub(crate) explorer_id: ID,
}

impl ToExplorerStruct {
    pub(crate) fn to_explorer(&self, msg: OrchestratorToExplorer) -> Result<(), ToExplorerError> {
        let sender = {
            let lock = self.explorers_senders.lock().unwrap();
            lock.get(&self.explorer_id).cloned() // Clone the Sender handle
        };

        if let Some(s) = sender {
            s.send(msg)
                .map_err(|_| ToExplorerError::SendingMessageFailure(self.explorer_id))
        } else {
            Err(ToExplorerError::SenderNotFound(self.explorer_id))
        }
    }
}

trait ErrorType {
    fn stringify(&self) -> String;
}

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

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        None
    }

    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        println!(
            "Conversation {} reached an error {}, closing conversation!",
            self.id,
            self.error.stringify()
        );
        None
    }
}
