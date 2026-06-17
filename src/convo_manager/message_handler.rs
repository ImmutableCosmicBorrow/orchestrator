use crate::convo_manager::ConvoManager;
use crate::logging::LogTarget;
use crate::logging::log_internal;
use crate::orchestrator::conversations::PossibleExpectedKinds;
use crate::orchestrator::conversations::PossibleMessage;
use crate::payload;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestratorKind::NeighborsRequest;
use common_game::protocols::orchestrator_explorer::ExplorerToOrchestratorKind::TravelToPlanetRequest;

impl ConvoManager {
    /// Handle an incoming message by matching it to an existing conversation,
    /// creating a new conversation, or buffering it for later matching.
    ///
    /// The matching and buffering for response messages is done atomically via
    /// [`ConvoScheduler::find_matching_or_buffer`] to prevent a TOCTOU race:
    /// without atomicity, `add_conversation`'s drain could complete between the
    /// "not found" check and the buffer, leaving the message stuck forever.
    pub(crate) fn handle_message(&self, message: PossibleMessage) {
        let message_kind = message.to_kind_type();
        let entities_ids = message.get_entity_ids();

        // Try to create a new conversation for this message type
        // (e.g., NeighborsRequest, TravelToPlanetRequest).
        // This is checked first because these initiating messages won't match
        // any existing inactive conversation.
        if (message_kind == PossibleExpectedKinds::ExplorerToOrchKind(NeighborsRequest)
            || message_kind == PossibleExpectedKinds::ExplorerToOrchKind(TravelToPlanetRequest))
            && let Some(id) = self.try_create_conversation(&message, &message_kind, entities_ids)
        {
            log_internal(
                LogTarget::Conversations,
                Channel::Trace,
                payload!(
                    event: "Message created new conversation",
                    conversation_id: id,
                    message_kind: format!("{:?}", message_kind),
                    from_planet: format!("{:?}", entities_ids.0),
                    from_explorer: format!("{:?}", entities_ids.1)
                ),
            );
            self.convo_scheduler.add_waiting_message(id, message);
            return;
        }

        // Atomically: find a matching inactive conversation OR buffer for later.
        // Holding the pending_msgs lock during find+buffer prevents
        // add_conversation's drain from missing this message.
        if let Some((id, message)) =
            self.convo_scheduler
                .find_matching_or_buffer(message, &message_kind, entities_ids)
        {
            log_internal(
                LogTarget::Conversations,
                Channel::Trace,
                payload!(
                    event: "Message matched conversation",
                    conversation_id: id,
                    message_kind: format!("{:?}", message_kind),
                    from_planet: format!("{:?}", entities_ids.0),
                    from_explorer: format!("{:?}", entities_ids.1)
                ),
            );
            self.convo_scheduler.add_waiting_message(id, message);
        }
        // If find_matching_or_buffer returned None, the message was buffered
        // and will be drained when a matching conversation is added.
    }
}
