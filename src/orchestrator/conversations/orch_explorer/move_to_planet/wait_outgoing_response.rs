use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::MoveToPlanetConversation;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::errors::MoveToPlanetErrors;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::{
    WaitMoveToPlanetResponse, WaitingOutgoingResponse,
};
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, ErrorType, PossibleExpectedKinds, PossibleMessage,
    ToExplorerError,
};
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer::MoveToPlanet;
use common_game::protocols::orchestrator_planet::{PlanetToOrchestrator, PlanetToOrchestratorKind};
use common_game::protocols::planet_explorer::ExplorerToPlanet;
use common_game::utils::ID;
use crossbeam_channel::Sender;

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
                                let state_struct = WaitMoveToPlanetResponse::new(
                                    self.state.explorers_location_ref,
                                    true,
                                    self.state.dst_planet_id,
                                );
                                let next_state =
                                    MoveToPlanetConversation::<WaitMoveToPlanetResponse>::new(
                                        self.id,
                                        state_struct,
                                    );
                                Some(Box::new(next_state))
                            }
                            Err(err) => {
                                let error: Box<dyn ErrorType> = match err {
                                    ToExplorerError::SendingMessageFailure(id) => {
                                        Box::new(CommonErrorTypes::MessageToExplorerFailed(id))
                                    }
                                    ToExplorerError::SenderNotFound(id) => {
                                        Box::new(CommonErrorTypes::ExplorerSenderNotFound(id))
                                    }
                                };
                                let error_state = ErrorState::new(error, self.id);
                                Some(Box::new(error_state))
                            }
                        };
                    }
                    //sender to new planet not found!!, explorer has not changed channels but planets did, ATTENTION
                    let error_state = ErrorState::new(
                        Box::new(MoveToPlanetErrors::NewSenderToPlanetNotFound(
                            self.state.dst_planet_id,
                        )),
                        self.id,
                    );
                    Some(Box::new(error_state))
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

impl MoveToPlanetConversation<WaitingOutgoingResponse> {
    pub(crate) fn new(id: ID, state: WaitingOutgoingResponse) -> Self {
        Self {
            id,
            expected_message: Some(PossibleExpectedKinds::PlanetToOrchKind(
                PlanetToOrchestratorKind::OutgoingExplorerResponse,
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
