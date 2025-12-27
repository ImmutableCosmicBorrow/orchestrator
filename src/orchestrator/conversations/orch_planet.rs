use std::marker::PhantomData;
use std::sync::mpsc::Sender;
use common_game::components::planet::PlanetState;
use common_game::components::rocket::Rocket;
use common_game::protocols::orchestrator_planet::{PlanetToOrchestrator, PlanetToOrchestratorKind};
use common_game::utils::ID;

struct WaitingPlanetStartResult;
struct WaitingPlanetStopResult;
struct WaitingPlanetKillResult;
struct WaitingSunrayAck;
struct WaitingInternalStateResponse;
struct WaitingAsteroidAck;
struct Error;

enum ConversationStatus {
    Ongoing,
    Error,
}

//TODO: REWRITE THIS BETTER


///Start Planet Conversation FSM
struct StartPlanetConversation<S> {
    _state: PhantomData<S>,
    expected_message: Option<PlanetToOrchestratorKind>,
    status: ConversationStatus,
    id: ID
}

impl StartPlanetConversation<WaitingPlanetStartResult> {
    fn new(id: ID) -> Self {
        StartPlanetConversation {
            _state: PhantomData,
            expected_message: Some(PlanetToOrchestratorKind::StartPlanetAIResult),
            status: ConversationStatus::Ongoing,
            id
        }
    }



    fn receive_msg(self, msg: PlanetToOrchestrator) -> Result<ID, StartPlanetConversation<Error>> {
        match msg {
            PlanetToOrchestrator::StartPlanetAIResult { .. } => {
                Ok(self.id)
            }

            _ => {
                Err(
                    StartPlanetConversation {
                        id: self.id,
                        expected_message: None,
                        status: ConversationStatus::Error,
                        _state:  PhantomData
                    }
                )
            }
        }
    }

    fn get_id(&self) -> ID {
        self.id
    }
}




///Stop Planet Conversation FSM
struct StopPlanetConversation<S> {
    _state: PhantomData<S>,
    expected_message: Option<PlanetToOrchestratorKind>,
    status: ConversationStatus,
    id: ID
}

impl StopPlanetConversation<WaitingPlanetStopResult> {
    fn new(id: ID) -> Self {
        StopPlanetConversation {
            _state: PhantomData,
            expected_message: Some(PlanetToOrchestratorKind::StopPlanetAIResult),
            status: ConversationStatus::Ongoing,
            id
        }
    }



    fn receive_msg(self, msg: PlanetToOrchestrator) -> Result<ID, StopPlanetConversation<Error>> {
        match msg {
            PlanetToOrchestrator::StopPlanetAIResult { .. } => {
                Ok(self.id)
            }

            _ => {
                Err(
                    StopPlanetConversation {
                        id: self.id,
                        expected_message: None,
                        status: ConversationStatus::Error,
                        _state:  PhantomData
                    }
                )
            }
        }
    }

    fn get_id(&self) -> ID {
        self.id
    }
}

///Planet Kill Conversation
struct KillPlanetConversation<S> {
    _state: PhantomData<S>,
    expected_message: Option<PlanetToOrchestratorKind>,
    status: ConversationStatus,
    id: ID
}

impl KillPlanetConversation<WaitingPlanetKillResult> {
    fn new(id: ID) -> Self {
        KillPlanetConversation {
            _state: PhantomData,
            expected_message: Some(PlanetToOrchestratorKind::KillPlanetResult),
            status: ConversationStatus::Ongoing,
            id
        }
    }



    fn receive_msg(self, msg: PlanetToOrchestrator) -> Result<ID, KillPlanetConversation<Error>> {
        match msg {
            PlanetToOrchestrator::KillPlanetResult { .. } => {
                Ok(self.id)
            }

            _ => {
                Err(
                    KillPlanetConversation {
                        id: self.id,
                        expected_message: None,
                        status: ConversationStatus::Error,
                        _state:  PhantomData
                    }
                )
            }
        }
    }

    fn get_id(&self) -> ID {
        self.id
    }
}


struct SendSunrayConversation<S> {
    _state: PhantomData<S>,
    expected_message: Option<PlanetToOrchestratorKind>,
    status: ConversationStatus,
    id: ID
}

impl SendSunrayConversation<WaitingSunrayAck> {
    fn new(id: ID) -> Self {
        SendSunrayConversation {
            _state: PhantomData,
            expected_message: Some(PlanetToOrchestratorKind::SunrayAck),
            status: ConversationStatus::Ongoing,
            id
        }
    }



    fn receive_msg(self, msg: PlanetToOrchestrator) -> Result<ID, SendSunrayConversation<Error>> {
        match msg {
            PlanetToOrchestrator::SunrayAck { .. } => {
                Ok(self.id)
            }

            _ => {
                Err(
                    SendSunrayConversation {
                        id: self.id,
                        expected_message: None,
                        status: ConversationStatus::Error,
                        _state:  PhantomData
                    }
                )
            }
        }
    }

    fn get_id(&self) -> ID {
        self.id
    }
}

///InternalState FSM
struct PlanetInternalStateConversation<S> {
    _state: PhantomData<S>,
    expected_message: Option<PlanetToOrchestratorKind>,
    status: ConversationStatus,
    id: ID
}

impl PlanetInternalStateConversation<WaitingInternalStateResponse> {
    fn new(id: ID) -> Self {
        PlanetInternalStateConversation {
            _state: PhantomData,
            expected_message: Some(PlanetToOrchestratorKind::InternalStateResponse),
            status: ConversationStatus::Ongoing,
            id
        }
    }



    fn receive_msg(self, msg: PlanetToOrchestrator) -> Result<ID, PlanetInternalStateConversation<Error>> {
        match msg {
            PlanetToOrchestrator::InternalStateResponse { .. } => {
                Ok(self.id)
            }

            _ => {
                Err(
                    PlanetInternalStateConversation {
                        id: self.id,
                        expected_message: None,
                        status: ConversationStatus::Error,
                        _state:  PhantomData
                    }
                )
            }
        }
    }

    fn get_id(&self) -> ID {
        self.id
    }
}


///Asteroid Conversation
/// if the FSM returns Some(rocket), the orchestrator initiates another conversation to kill the planet
struct AsteroidConversation<S> {
    _state: PhantomData<S>,
    expected_message: Option<PlanetToOrchestratorKind>,
    status: ConversationStatus,
    id: ID
}

impl AsteroidConversation<WaitingAsteroidAck> {
    fn new(id: ID) -> Self {
        AsteroidConversation {
            _state: PhantomData,
            expected_message: Some(PlanetToOrchestratorKind::AsteroidAck),
            status: ConversationStatus::Ongoing,
            id
        }
    }



    fn receive_msg(self, msg: PlanetToOrchestrator) -> Result<(ID, Option<Rocket>), AsteroidConversation<Error>> {
        match msg {
            PlanetToOrchestrator::AsteroidAck { rocket, .. } => {
                Ok((self.id, rocket))
            }

            _ => {
                Err(
                    AsteroidConversation {
                        id: self.id,
                        expected_message: None,
                        status: ConversationStatus::Error,
                        _state:  PhantomData
                    }
                )
            }
        }
    }

    fn get_id(&self) -> ID {
        self.id
    }
}