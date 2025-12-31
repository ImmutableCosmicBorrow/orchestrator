use crate::galaxy_setup::PlanetMap;
use crate::orchestrator::conversations::{
    Conversation, PossibleExpectedKinds, PossibleMessage, SendersToExplorer, SendersToPlanet,
    ToExplorerStruct, ToPlanetStruct,
};
use crate::orchestrator::{ExplorerBag, PlanetExplorerChannels};

use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer::MoveToPlanet;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::protocols::planet_explorer::PlanetToExplorer;
use common_game::utils::ID;
use crossbeam_channel::Sender;

//TODO: Look for DashMap

struct WaitingTravelRequest {
    galaxy: PlanetMap,
    planet_explorer_channels: PlanetExplorerChannels,
    curr_planet_struct: ToPlanetStruct,
    dst_planet_struct: ToPlanetStruct,
    explorer_struct: ToExplorerStruct,
}

struct WaitMoveToPlanetResponse;

struct MoveToPlanetConversation<State> {
    id: ID,
    state: State,
    expected_message: Option<PossibleExpectedKinds>,
}

impl Conversation<ExplorerBag> for MoveToPlanetConversation<WaitingTravelRequest> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Result<
        Option<Box<dyn Conversation<ExplorerBag>>>,
        (Option<Box<dyn Conversation<ExplorerBag>>>, String),
    > {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::TravelToPlanetRequest {
                explorer_id,
                current_planet_id,
                dst_planet_id,
            },
        )) = msg_wrapped
        {
            if self.check_neighbors() {
                if let Some(sender) = self.get_new_explorer_sender(explorer_id) {
                    //returns new state if senidn message goes well or same state if channel fails
                    return self
                        .state
                        .dst_planet_struct
                        .to_planet(OrchestratorToPlanet::IncomingExplorerRequest {
                            explorer_id,
                            new_sender: sender,
                        })
                        .map(|_| {
                            Some(
                                Box::new(MoveToPlanetConversation::<WaitingIncomingResponse>::new(
                                    self.id,
                                    WaitingIncomingResponse {
                                        curr_planet_struct: self.state.curr_planet_struct,
                                        explorer_struct: self.state.explorer_struct,
                                    },
                                ))
                                    as Box<dyn Conversation<ExplorerBag>>,
                            )
                        })
                        .map_err(|e| {
                            (
                                Some(self as Box<dyn Conversation<ExplorerBag>>),
                                e.to_string(),
                            )
                        });
                }
                return Err((None, format!("sender to explorer {explorer_id} not found")));
            }

            //returns Ok with None sender to say that the planet is not a neighbor but the communication works,
            //or Err None to close the conversation because an error occured to the sender
            return self
                .state
                .explorer_struct
                .to_explorer(MoveToPlanet {
                    sender_to_new_planet: None,
                })
                .map(|_| {
                    Some(Box::new(
                        MoveToPlanetConversation::<WaitMoveToPlanetResponse>::new(),
                    ))
                })
                .map_err(|e| (None, e));
        }
        //Wrong Message, stay in same state
        Err((
            Some(self),
            "Got Wrong message, staying in same state".to_string(),
        ))
    }
}

impl MoveToPlanetConversation<WaitingTravelRequest> {
    fn check_neighbors(&self) -> bool {
        if let Some(curr_planet_ref) = self
            .state
            .galaxy
            .lock()
            .unwrap()
            .get(&self.state.curr_planet_struct.planet_id)
        {
            if let Some(dst_planet_ref) = self
                .state
                .galaxy
                .lock()
                .unwrap()
                .get(&self.state.dst_planet_struct.planet_id)
            {
                return curr_planet_ref.lock().unwrap().has_neighbor(dst_planet_ref);
            }
        }
        false
    }

    fn get_new_explorer_sender(&self, explorer_id: ID) -> Option<Sender<PlanetToExplorer>> {
        self.state
            .planet_explorer_channels
            .planet_to_explorer_senders
            .lock()
            .unwrap()
            .get(&explorer_id)
            .cloned()
    }

    fn new(id: ID, state: WaitingTravelRequest) -> Self {
        Self {
            id,
            state,
            expected_message: Some(ExplorerToOrchKind(
                ExplorerToOrchestratorKind::TravelToPlanetRequest,
            )),
        }
    }
}

//Waiting Incoming Response
struct WaitingIncomingResponse {
    curr_planet_struct: ToPlanetStruct,
    explorer_struct: ToExplorerStruct,
}

impl WaitingIncomingResponse {
    pub(crate) fn new(
        curr_planet_struct: ToPlanetStruct,
        explorer_struct: ToExplorerStruct,
    ) -> Self {
        Self {
            curr_planet_struct,
            explorer_struct,
        }
    }
}

impl Conversation<ExplorerBag> for MoveToPlanetConversation<WaitingIncomingResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Result<
        Option<Box<dyn Conversation<ExplorerBag>>>,
        (Option<Box<dyn Conversation<ExplorerBag>>>, String),
    > {
        if let Some(PossibleMessage::PlanetToOrch(
            PlanetToOrchestrator::IncomingExplorerResponse {
                planet_id,
                explorer_id,
                res,
            },
        )) = msg_wrapped
        {
            return  res.map(
                |_| {
                    self.state.curr_planet_struct.to_planet(
                        OrchestratorToPlanet::OutgoingExplorerRequest {
                            explorer_id,
                        }
                    ).map(
                        |_| {
                            let next_state = MoveToPlanetConversation::<WaitingOutgoingResponse>::new();
                            Some(Box::new(next_state) as Box<dyn Conversation<ExplorerBag>>)
                        }
                    ).map_err(|e| {
                        (None, e.to_string())
                    })
                })
                .map_err(|e| {
                    (
                        Some(self),
                        format!("destination planet {planet_id} failed to acquire explorer {explorer_id}: {e}"),
                        )
                });
        }
        Err((
            Some(self),
            "Got Wrong message, staying in same state".to_string(),
        ))
    }
}

impl MoveToPlanetConversation<WaitingIncomingResponse> {
    pub(crate) fn new(id: ID, state: WaitingIncomingResponse) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::PlanetToOrchKind(
                PlanetToOrchestratorKind::IncomingExplorerResponse,
            )),
            state,
        }
    }
}

struct WaitingOutgoingResponse {
    explorer_struct: ToExplorerStruct,
}

impl WaitingOutgoingResponse {
    pub(crate) fn new(explorer_struct: ToExplorerStruct) -> Self {
        Self { explorer_struct }
    }
}

impl Conversation<ExplorerBag> for MoveToPlanetConversation<WaitingOutgoingResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Result<
        Option<Box<dyn Conversation<ExplorerBag>>>,
        (Option<Box<dyn Conversation<ExplorerBag>>>, String),
    > {
        if let Some(PossibleMessage::PlanetToOrch(
            PlanetToOrchestrator::OutgoingExplorerResponse {
                planet_id,
                explorer_id,
                res,
            },
        )) = msg_wrapped
        {
            todo!()
        }
        todo!()
    }
}

impl MoveToPlanetConversation<WaitingOutgoingResponse> {
    pub(crate) fn new(id: ID, state: WaitingOutgoingResponse) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::PlanetToOrchKind(
                PlanetToOrchestratorKind::IncomingExplorerResponse,
            )),
            state,
        }
    }
}
