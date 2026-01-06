use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::PossibleExpectedKinds::ExplorerToOrchKind;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::MoveToPlanetConversation;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    WaitMoveToPlanetResponse, WaitingIncomingResponse, WaitingTravelRequest,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError, ToPlanetError,
};
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer::MoveToPlanet;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, ExplorerToOrchestratorKind,
};
use common_game::protocols::orchestrator_planet::OrchestratorToPlanet;
use common_game::protocols::planet_explorer::PlanetToExplorer;
use common_game::utils::ID;
use crossbeam_channel::Sender;

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
                                explorers_location_ref: self.state.explorers_location_ref,
                            };
                            let new_state =
                                MoveToPlanetConversation::<WaitingIncomingResponse>::new(
                                    self.id,
                                    state_struct,
                                );
                            Some(Box::new(new_state))
                        }

                        Err(err) => {
                            let error: Box<dyn ErrorType> = match err {
                                ToPlanetError::SenderNotFound(id) => {
                                    Box::new(CommonErrorTypes::PlanetSenderNotFound(id))
                                }
                                ToPlanetError::SendingMessageFailure(id) => {
                                    Box::new(MoveToPlanetErrors::IncomingMessageFailed(id))
                                }
                            };
                            let error_state = ErrorState::new(error, self.id);
                            Some(Box::new(error_state))
                        }
                    };
                }
                //The sender to explorer is not found, going to error state
                let error_state = ErrorState::new(
                    Box::new(CommonErrorTypes::ExplorerSenderNotFound(explorer_id)),
                    self.id,
                );
                return Some(Box::new(error_state));
            }

            //Tries to send a MoveToPlanet {none} to the explorer as he cannot move, or goes in error
            return match self.state.explorer_struct.to_explorer(MoveToPlanet {
                sender_to_new_planet: None,
            }) {
                Ok(_) => {
                    let state_struct = WaitMoveToPlanetResponse::new(
                        self.state.explorers_location_ref,
                        false,
                        self.state.dst_planet_struct.planet_id,
                    );
                    let next_state = MoveToPlanetConversation::<WaitMoveToPlanetResponse>::new(
                        self.id,
                        state_struct,
                    );
                    Some(Box::new(next_state))
                }
                Err(err) => {
                    let error: Box<dyn ErrorType> = match err {
                        ToExplorerError::SenderNotFound(id) => {
                            Box::new(CommonErrorTypes::ExplorerSenderNotFound(id))
                        }
                        ToExplorerError::SendingMessageFailure(id) => {
                            Box::new(MoveToPlanetErrors::IncomingMessageFailed(id))
                        }
                    };
                    let error_state = ErrorState::new(error, self.id);
                    Some(Box::new(error_state))
                }
            };
        }
        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
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
