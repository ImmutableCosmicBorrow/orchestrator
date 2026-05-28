use common_game::utils::ID;

use crate::convo_manager::convo_scheduler::ConvoScheduler;
pub(crate) use crate::orchestrator::{OrchContext, OrchContextRef};
use std::sync::Arc;

pub mod convo_factory;
mod convo_scheduler;
mod message_handler;
pub mod queue;

pub(crate) struct ConvoManager {
    pub(crate) convo_scheduler: ConvoScheduler,
    orch_context: OrchContextRef,
}

impl ConvoManager {
    pub(crate) fn new(orch_context: OrchContextRef) -> Self {
        Self {
            convo_scheduler: ConvoScheduler::new(),
            orch_context,
        }
    }

    // TODO: remove the allow dead_code once these getters are used in the convo states.
    #[allow(dead_code)]
    pub(crate) fn get_convo_scheduler(&self) -> &ConvoScheduler {
        &self.convo_scheduler
    }

    pub(crate) fn get_orch_context(&self) -> Arc<OrchContext> {
        self.orch_context.clone()
    }

    pub fn remove_convos_for_dead_entity(&self, id: ID) {
        self.convo_scheduler.remove_convos_for_dead_entity(id);
    }
}
