use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestrator;
use common_game::utils::ID;
use crate::convo_manager::ConvoManager;
use crate::logging_utils::{log_internal, LogTarget};
use crate::orchestrator::conversations::{PossibleExpectedKinds, PossibleMessage};
use crate::payload;

impl ConvoManager {
    pub(crate) fn try_create_conversation(
        &self,
        message: &PossibleMessage,
        message_kind: &PossibleExpectedKinds,
        entities_ids: (Option<ID>, Option<ID>),
    ) -> Option<ID> {
        match message {
            PossibleMessage::ExplorerToOrch(msg) => match msg {
                ExplorerToOrchestrator::NeighborsRequest {
                    explorer_id,
                    current_planet_id: _,
                } => Some(self.create_neighbors_request_conversation(
                    *explorer_id,
                )
                ),
                ExplorerToOrchestrator::TravelToPlanetRequest {
                    explorer_id,
                    current_planet_id,
                    dst_planet_id,
                } => Some(self.create_waiting_travel_to_planet_request_conversation(
                    *explorer_id,
                    *current_planet_id,
                    *dst_planet_id,
                )),
                _ => {
                    log_internal(
                        LogTarget::General,
                        Channel::Warning,
                        payload!(
                            action: "Received ExplorerToOrchestrator message that does not start a conversation. Ignoring.",
                            message_kind: format!("{:?}", message_kind),
                            from_explorer: entities_ids.1.unwrap(),
                        ),
                    );
                    None
                }
            },
            PossibleMessage::PlanetToOrch(_) => {
                log_internal(
                    LogTarget::General,
                    Channel::Warning,
                    payload!(
                        action: "Received PlanetToOrchestrator message that does not start a conversation. Ignoring.",
                        message_kind: format!("{:?}", message_kind),
                        from_planet: entities_ids.0.unwrap(),
                    ),
                );
                None
            }
        }
    }
}

