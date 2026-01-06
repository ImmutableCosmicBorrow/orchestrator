use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    MoveToPlanetConversation, WaitingIncomingResponse, WaitingOutgoingResponse,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToPlanetError,
};
use common_game::protocols::orchestrator_planet::{
    OrchestratorToPlanet, PlanetToOrchestrator, PlanetToOrchestratorKind,
};
use common_game::utils::ID;

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
                                self.state.explorers_location_ref,
                            );
                            let next_state =
                                MoveToPlanetConversation::<WaitingOutgoingResponse>::new(
                                    self.id,
                                    state_struct,
                                );
                            Some(Box::new(next_state))
                        }
                        Err(err) => {
                            let error: Box<dyn ErrorType> = match err {
                                ToPlanetError::SendingMessageFailure(id) => {
                                    Box::new(MoveToPlanetErrors::OutgoingMessageFailed(id))
                                }
                                ToPlanetError::SenderNotFound(id) => {
                                    Box::new(CommonErrorTypes::PlanetSenderNotFound(id))
                                }
                            };
                            let error_state = ErrorState::new(error, self.id);
                            Some(Box::new(error_state))
                        }
                    }
                }

                Err(_) => {
                    let error_state = ErrorState::new(
                        Box::new(MoveToPlanetErrors::DstPlanetFailed {
                            planet_id,
                            explorer_id,
                        }),
                        self.id,
                    );
                    return Some(Box::new(error_state));
                }
            };
        }
        //Wrong message, closing Conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
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
