use crate::convo_manager::ConvoManager;
use crate::logging::LogTarget;
use crate::logging::log_internal;
use crate::orchestrator::conversations::PossibleMessage;
use crate::payload;
use common_game::logging::Channel;

impl ConvoManager {
    pub(crate) fn handle_message(&self, message: PossibleMessage) {
        let message_kind = message.to_kind_type();
        let entities_ids = message.get_entity_ids();
        let convo_id = self
            .convo_scheduler
            .find_matching_conversation(&message_kind, entities_ids)
            .or_else(|| self.try_create_conversation(&message, &message_kind, entities_ids));

        if let Some(id) = convo_id {
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
        } else {
            // No matching conversation found yet — buffer for later matching.
            // This handles the race where a response arrives between
            // transition() and add_conversation() on the processor thread.
            self.convo_scheduler.buffer_pending_message(message);
        }
    }
}
