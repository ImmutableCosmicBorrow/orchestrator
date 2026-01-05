use crate::galaxy_setup::PlanetMap;
use crate::orchestrator::conversations::{
    Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage, ToExplorerError,
    ToExplorerStruct, ToPlanetError, ToPlanetStruct,
};
use crate::orchestrator::{ExplorerBag, PlanetExplorerChannels};

use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestratorKind::MovedToPlanetResult;
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer::MoveToPlanet;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::protocols::planet_explorer::{ExplorerToPlanet, PlanetToExplorer};
use common_game::utils::ID;
use crossbeam_channel::Sender;

//TODO: Look for DashMap

pub(crate) enum MoveToPlanetErrors {
    ExplorerSenderNotFound(ID),
    MessageToExplorerFailed(ID),
    IncomingMessageFailed(ID),
    OutgoingMessageFailed(ID),
    PlanetSenderNotFound(ID),
    WrongMessage,
    DstPlanetFailed { planet_id: ID, explorer_id: ID },
    CurrPlanetFailed { planet_id: ID, explorer_id: ID },
    NewSenderToPlanetNotFound(ID),
}

impl ErrorType for MoveToPlanetErrors {
    fn stringify(&self) -> String {
        match self {
            MoveToPlanetErrors::ExplorerSenderNotFound(id) => {
                format!("sender to explorer {id} not found")
            }
            MoveToPlanetErrors::IncomingMessageFailed(id) => {
                format!("Failed to send Incoming message to destination planet {id}")
            }
            MoveToPlanetErrors::OutgoingMessageFailed(id) => {
                format!("Failed to send Outgoing message to current planet {id}")
            }
            MoveToPlanetErrors::WrongMessage => "Wrong Message Received".to_string(),
            MoveToPlanetErrors::PlanetSenderNotFound(id) => {
                format!("sender to planet {id} not found")
            }
            MoveToPlanetErrors::MessageToExplorerFailed(id) => {
                format!("Failed to send message to explorer {id}")
            }

            MoveToPlanetErrors::DstPlanetFailed {
                planet_id,
                explorer_id,
            } => format!(
                "Destination planet {planet_id} failed to acquire incoming explorer {explorer_id}"
            ),
            MoveToPlanetErrors::CurrPlanetFailed {
                planet_id,
                explorer_id,
            } => format!(
                "Current planet {planet_id} failed to let go of outgoing explorer {explorer_id}"
            ),
            MoveToPlanetErrors::NewSenderToPlanetNotFound(id) => format!(
                "sender to dest planet {id} not found, planets already changed explorer channels but explorer did not"
            ),
        }
    }
}

//States
struct WaitingTravelRequest {
    galaxy: PlanetMap,
    planet_explorer_channels: PlanetExplorerChannels,
    curr_planet_struct: ToPlanetStruct,
    dst_planet_struct: ToPlanetStruct,
    explorer_struct: ToExplorerStruct,
}

struct WaitingIncomingResponse {
    curr_planet_struct: ToPlanetStruct,
    explorer_struct: ToExplorerStruct,
    dst_planet_id: ID,
    planet_explorer_channels: PlanetExplorerChannels,
}

impl WaitingIncomingResponse {
    pub(crate) fn new(
        curr_planet_struct: ToPlanetStruct,
        explorer_struct: ToExplorerStruct,
        dst_planet_id: ID,
        planet_explorer_channels: PlanetExplorerChannels,
    ) -> Self {
        Self {
            curr_planet_struct,
            explorer_struct,
            planet_explorer_channels,
            dst_planet_id,
        }
    }
}

struct WaitingOutgoingResponse {
    explorer_struct: ToExplorerStruct,
    planet_explorer_channels: PlanetExplorerChannels,
    dst_planet_id: ID,
}

impl WaitingOutgoingResponse {
    pub(crate) fn new(
        explorer_struct: ToExplorerStruct,
        planet_explorer_channels: PlanetExplorerChannels,
        dst_planet_id: ID,
    ) -> Self {
        Self {
            explorer_struct,
            planet_explorer_channels,
            dst_planet_id,
        }
    }
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
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::TravelToPlanetRequest {
                explorer_id,
                current_planet_id: _current_planet_id,
                dst_planet_id: _dst_planet_id,
            },
        )) = msg_wrapped
        {
            if self.check_neighbors() {
                if let Some(sender) = self.get_new_explorer_sender(explorer_id) {
                    //returns new state if send message goes well or same state if channel fails
                    return match self.state.dst_planet_struct.to_planet(
                        OrchestratorToPlanet::IncomingExplorerRequest {
                            explorer_id,
                            new_sender: sender,
                        },
                    ) {
                        Ok(_) => {
                            let state_struct = WaitingIncomingResponse {
                                curr_planet_struct: self.state.curr_planet_struct,
                                explorer_struct: self.state.explorer_struct,
                                dst_planet_id: self.state.dst_planet_struct.planet_id,
                                planet_explorer_channels: self.state.planet_explorer_channels,
                            };
                            let new_state =
                                MoveToPlanetConversation::<WaitingIncomingResponse>::new(
                                    self.id,
                                    state_struct,
                                );
                            Some(Box::new(new_state))
                        }

                        Err(err) => {
                            let error = match err {
                                ToPlanetError::SenderNotFound(id) => {
                                    MoveToPlanetErrors::PlanetSenderNotFound(id)
                                }
                                ToPlanetError::SendingMessageFailure(id) => {
                                    MoveToPlanetErrors::IncomingMessageFailed(id)
                                }
                            };
                            let error_state = ErrorState::new(Box::new(error));
                            let next_state =
                                MoveToPlanetConversation::<ErrorState>::new(self.id, error_state);
                            Some(Box::new(next_state))
                        }
                    };
                }
                //The sender to explorer is not found, going to error state
                let error_state = ErrorState::new(Box::new(
                    MoveToPlanetErrors::ExplorerSenderNotFound(explorer_id),
                ));
                let next_state = MoveToPlanetConversation::<ErrorState>::new(self.id, error_state);
                return Some(Box::new(next_state));
            }

            //Tries to send a MoveToPlanet {none} to the explorer as he cannot move, or goes in error
            return match self.state.explorer_struct.to_explorer(MoveToPlanet {
                sender_to_new_planet: None,
            }) {
                Ok(_) => Some(Box::new(
                    MoveToPlanetConversation::<WaitMoveToPlanetResponse>::new(self.id),
                )),
                Err(err) => {
                    let error = match err {
                        ToExplorerError::SenderNotFound(id) => {
                            MoveToPlanetErrors::ExplorerSenderNotFound(id)
                        }
                        ToExplorerError::SendingMessageFailure(id) => {
                            MoveToPlanetErrors::IncomingMessageFailed(id)
                        }
                    };
                    let error_state = ErrorState::new(Box::new(error));
                    let next_state =
                        MoveToPlanetConversation::<ErrorState>::new(self.id, error_state);
                    Some(Box::new(next_state))
                }
            };
        }
        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(MoveToPlanetErrors::WrongMessage));
        let next_state = MoveToPlanetConversation::<ErrorState>::new(self.id, error_state);
        Some(Box::new(next_state))
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
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        if let Some(PossibleMessage::PlanetToOrch(
            PlanetToOrchestrator::IncomingExplorerResponse {
                planet_id,
                explorer_id,
                res,
            },
        )) = msg_wrapped
        {
            //if the incoming response is positive, tries to send the Outgoing request, otherwise terminates in error state
            return match res {
                Ok(_) => {
                    match self
                        .state
                        .curr_planet_struct
                        .to_planet(OrchestratorToPlanet::OutgoingExplorerRequest { explorer_id })
                    {
                        Ok(_) => {
                            let state_struct = WaitingOutgoingResponse::new(
                                self.state.explorer_struct,
                                self.state.planet_explorer_channels,
                                self.state.dst_planet_id,
                            );
                            let next_state =
                                MoveToPlanetConversation::<WaitingOutgoingResponse>::new(
                                    self.id,
                                    state_struct,
                                );
                            Some(Box::new(next_state))
                        }
                        Err(err) => {
                            let error = match err {
                                ToPlanetError::SendingMessageFailure(id) => {
                                    MoveToPlanetErrors::OutgoingMessageFailed(id)
                                }
                                ToPlanetError::SenderNotFound(id) => {
                                    MoveToPlanetErrors::PlanetSenderNotFound(id)
                                }
                            };
                            let error_state = ErrorState::new(Box::new(error));
                            let new_state =
                                MoveToPlanetConversation::<ErrorState>::new(self.id, error_state);
                            Some(Box::new(new_state))
                        }
                    }
                }

                Err(_) => {
                    let err_struct =
                        ErrorState::new(Box::new(MoveToPlanetErrors::DstPlanetFailed {
                            planet_id,
                            explorer_id,
                        }));
                    let next_state =
                        MoveToPlanetConversation::<ErrorState>::new(self.id, err_struct);
                    return Some(Box::new(next_state));
                }
            };
        }
        //Wrong message, closing Conversation
        let error_state = ErrorState::new(Box::new(MoveToPlanetErrors::WrongMessage));
        let next_state = MoveToPlanetConversation::<ErrorState>::new(self.id, error_state);
        Some(Box::new(next_state))
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

//WaitingOutgoingResponse Implementation
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
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        if let Some(PossibleMessage::PlanetToOrch(
            PlanetToOrchestrator::OutgoingExplorerResponse {
                planet_id,
                explorer_id,
                res,
            },
        )) = msg_wrapped
        {
            return match res {
                Ok(_) => {
                    if let Some(new_sender) = self.get_new_planet_sender() {
                        return match self.state.explorer_struct.to_explorer(MoveToPlanet {
                            sender_to_new_planet: Some(new_sender),
                        }) {
                            Ok(_) => {
                                let next_state =
                                    MoveToPlanetConversation::<WaitMoveToPlanetResponse>::new(
                                        self.id,
                                    );
                                Some(Box::new(next_state))
                            }
                            Err(err) => {
                                let error = match err {
                                    ToExplorerError::SendingMessageFailure(id) => {
                                        MoveToPlanetErrors::MessageToExplorerFailed(id)
                                    }
                                    ToExplorerError::SenderNotFound(id) => {
                                        MoveToPlanetErrors::ExplorerSenderNotFound(id)
                                    }
                                };
                                let error_state = ErrorState::new(Box::new(error));
                                let new_state = MoveToPlanetConversation::<ErrorState>::new(
                                    self.id,
                                    error_state,
                                );
                                Some(Box::new(new_state))
                            }
                        };
                    }
                    //sender to new planet not found!!, explorer has not changed channels but planets did, ATTENTION
                    let err_struct = ErrorState::new(Box::new(
                        MoveToPlanetErrors::NewSenderToPlanetNotFound(self.state.dst_planet_id),
                    ));
                    let next_state =
                        MoveToPlanetConversation::<ErrorState>::new(self.id, err_struct);
                    Some(Box::new(next_state))
                }

                Err(_) => {
                    let err_struct =
                        ErrorState::new(Box::new(MoveToPlanetErrors::DstPlanetFailed {
                            planet_id,
                            explorer_id,
                        }));
                    let next_state =
                        MoveToPlanetConversation::<ErrorState>::new(self.id, err_struct);
                    return Some(Box::new(next_state));
                }
            };
        }
        //Wrong message, closing Conversation
        let error_state = ErrorState::new(Box::new(MoveToPlanetErrors::WrongMessage));
        let next_state = MoveToPlanetConversation::<ErrorState>::new(self.id, error_state);
        Some(Box::new(next_state))
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

    fn get_new_planet_sender(&self) -> Option<Sender<ExplorerToPlanet>> {
        self.state
            .planet_explorer_channels
            .explorer_to_planet_senders
            .lock()
            .unwrap()
            .get(&self.state.dst_planet_id)
            .cloned()
    }
}

//WaitMoveToPlanetResponse Implementation
impl Conversation<ExplorerBag> for MoveToPlanetConversation<WaitMoveToPlanetResponse> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        if let Some(PossibleMessage::ExplorerToOrch(
            ExplorerToOrchestrator::MovedToPlanetResult { explorer_id },
        )) = msg_wrapped
        {
            println!("Explorer {explorer_id} moved correctly");
            return None;
        }

        //Wrong message, closing Conversation
        let error_state = ErrorState::new(Box::new(MoveToPlanetErrors::WrongMessage));
        let next_state = MoveToPlanetConversation::<ErrorState>::new(self.id, error_state);
        Some(Box::new(next_state))
    }
}

impl MoveToPlanetConversation<WaitMoveToPlanetResponse> {
    pub(crate) fn new(id: ID) -> Self {
        Self {
            id,
            expected_message: Some(ExplorerToOrchKind(MovedToPlanetResult)),
            state: WaitMoveToPlanetResponse,
        }
    }
}

//Error
impl Conversation<ExplorerBag> for MoveToPlanetConversation<ErrorState> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag>>> {
        println!(
            "Move To Planet reached an error {}, closing conversation!",
            self.state.error.stringify()
        );
        None
    }
}

impl MoveToPlanetConversation<ErrorState> {
    fn new(id: ID, state: ErrorState) -> Self {
        Self {
            id,
            state,
            expected_message: None,
        }
    }
}
