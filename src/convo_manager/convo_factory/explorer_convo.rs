use crate::convo_manager::ConvoManager;
use crate::logging::{log_internal, LogTarget};
use crate::orchestrator::conversations::orch_explorer::lifecycle::kill_explorer::{KillExplorerConversation, SendingExplorerKill};
use crate::orchestrator::conversations::orch_explorer::lifecycle::reset_explorer::{ResetExplorerConversation, SendingExplorerReset};
use crate::orchestrator::conversations::orch_explorer::lifecycle::start_explorer::{SendingExplorerStart, StartExplorerConversation};
use crate::orchestrator::conversations::orch_explorer::lifecycle::stop_explorer::{SendingExplorerStop, StopExplorerConversation};
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::manual_move_to_planet::SendManualMoveRequest;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::wait_travel_request::WaitingTravelRequest;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::MoveToPlanetConversation;
use crate::orchestrator::conversations::orch_explorer::movement::neighbors_discovery::{NeighborsDiscoveryConversation, WaitingNeighborsRequest};
use crate::orchestrator::conversations::orch_explorer::resources::bag_content_scenario::{BagContentConversation, SendingBagContentRequest};
use crate::orchestrator::conversations::orch_explorer::resources::combine_resource::{CombineResourceConversation, SendingCombineResourceRequest};
use crate::orchestrator::conversations::orch_explorer::resources::craft_resource::{CraftResourceConversation, SendingCraftResourceRequest};
use crate::orchestrator::conversations::orch_explorer::resources::supported_combination::{SendingSupportedCombinationRequest, SupportedCombinationConversation};
use crate::orchestrator::conversations::orch_explorer::resources::supported_resources::{SendingSupportedResourcesRequest, SupportedResourcesConversation};
use crate::orchestrator::conversations;
use crate::{get_id_manager, payload};
use common_game::components::resource::{BasicResourceType, ComplexResourceType};
use common_game::logging::Channel;
use common_game::utils::ID;

impl ConvoManager {
    pub(crate) fn create_neighbors_request_conversation(&self, explorer_id: ID) -> ID {
        let state = WaitingNeighborsRequest::new(self.orch_context.clone(), explorer_id);

        let id = get_id_manager().get_next_conversation_id();
        let new_conv = NeighborsDiscoveryConversation::<WaitingNeighborsRequest>::new(id, state);

        self.convo_scheduler.add_conversation(
            Box::new(new_conv) as Box<dyn conversations::Conversation + Send + Sync>
        );

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "ScheduleConversation",
                conversation_id: id,
                kind: "NeighborsDiscovery",
                explorer_id: explorer_id
            ),
        );

        id
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_send_manual_move_conversation(
        &self,
        explorer_id: ID,
        current_planet_id: Option<ID>,
        dst_planet_id: ID,
    ) -> ID {
        let state = SendManualMoveRequest::new(
            self.orch_context.clone(),
            explorer_id,
            dst_planet_id,
            current_planet_id,
        );

        let id = get_id_manager().get_next_conversation_id();
        let new_conv = MoveToPlanetConversation::<SendManualMoveRequest>::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv));

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "ScheduleConversation",
                conversation_id: id,
                kind: "ManualMoveRequest",
                explorer_id: explorer_id,
                from_planet: format!("{current_planet_id:?}"),
                to_planet: dst_planet_id
            ),
        );

        id
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_waiting_travel_to_planet_request_conversation(
        &self,
        explorer_id: ID,
        current_planet_id: ID,
        dst_planet_id: ID,
    ) -> ID {
        let state =
            WaitingTravelRequest::new(self.orch_context.clone(), explorer_id, current_planet_id);

        let id = get_id_manager().get_next_conversation_id();
        let new_conv = MoveToPlanetConversation::<WaitingTravelRequest>::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv));

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "ScheduleConversation",
                conversation_id: id,
                kind: "WaitingMoveToPlanet",
                explorer_id: explorer_id,
                from_planet: format!("{current_planet_id:?}"),
                to_planet: dst_planet_id
            ),
        );

        id
    }

    pub(crate) fn create_bag_content_conversation(&self, explorer_id: ID) -> ID {
        let state = SendingBagContentRequest::new(self.orch_context.clone(), explorer_id);
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = BagContentConversation::<SendingBagContentRequest>::new(id, state);

        self.convo_scheduler.add_conversation(
            Box::new(new_conv) as Box<dyn conversations::Conversation + Send + Sync>
        );

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "ScheduleConversation",
                conversation_id: id,
                kind: "BagContent",
                explorer_id: explorer_id
            ),
        );

        id
    }

    pub(crate) fn create_generate_resource_conversation(
        &self,
        explorer_id: ID,
        resource_type: BasicResourceType,
    ) -> ID {
        let state =
            SendingCraftResourceRequest::new(self.orch_context.clone(), explorer_id, resource_type);
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = CraftResourceConversation::<SendingCraftResourceRequest>::new(id, state);

        self.convo_scheduler.add_conversation(
            Box::new(new_conv) as Box<dyn conversations::Conversation + Send + Sync>
        );

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "ScheduleConversation",
                conversation_id: id,
                kind: "CraftResource",
                explorer_id: explorer_id,
                resource_type: format!("{:?}", resource_type)
            ),
        );

        id
    }

    pub(crate) fn create_combine_resource_conversation(
        &self,
        explorer_id: ID,
        resource_type: ComplexResourceType,
    ) -> ID {
        let state = SendingCombineResourceRequest::new(
            self.orch_context.clone(),
            explorer_id,
            resource_type,
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = CombineResourceConversation::<SendingCombineResourceRequest>::new(id, state);

        self.convo_scheduler.add_conversation(
            Box::new(new_conv) as Box<dyn conversations::Conversation + Send + Sync>
        );

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "ScheduleConversation",
                conversation_id: id,
                kind: "CombineResource",
                explorer_id: explorer_id,
                resource_type: format!("{:?}", resource_type)
            ),
        );

        id
    }

    pub(crate) fn create_start_explorer_conversation(&self, explorer_id: ID) -> ID {
        let state = SendingExplorerStart::new(self.orch_context.clone(), explorer_id);
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = StartExplorerConversation::<SendingExplorerStart>::new(id, state);

        self.convo_scheduler.add_conversation(
            Box::new(new_conv) as Box<dyn conversations::Conversation + Send + Sync>
        );

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "ScheduleConversation",
                conversation_id: id,
                kind: "StartExplorer",
                explorer_id: explorer_id
            ),
        );

        id
    }

    pub(crate) fn create_stop_explorer_conversation(&self, explorer_id: ID) -> ID {
        let state = SendingExplorerStop::new(self.orch_context.clone(), explorer_id);
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = StopExplorerConversation::<SendingExplorerStop>::new(id, state);

        self.convo_scheduler.add_conversation(
            Box::new(new_conv) as Box<dyn conversations::Conversation + Send + Sync>
        );

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "ScheduleConversation",
                conversation_id: id,
                kind: "StopExplorer",
                explorer_id: explorer_id
            ),
        );

        id
    }

    pub(crate) fn create_kill_explorer_conversation(
        &self,
        explorer_id: ID,
        planet_id: ID,
        handle_outgoing: bool,
    ) -> ID {
        let state = SendingExplorerKill::new(
            self.orch_context.clone(),
            explorer_id,
            planet_id,
            handle_outgoing,
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = KillExplorerConversation::<SendingExplorerKill>::new(id, state);

        self.convo_scheduler.add_conversation(
            Box::new(new_conv) as Box<dyn conversations::Conversation + Send + Sync>
        );

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "ScheduleConversation",
                conversation_id: id,
                kind: "KillExplorer",
                explorer_id: explorer_id,
                planet_id: planet_id,
                handle_outgoing: handle_outgoing
            ),
        );

        id
    }

    pub(crate) fn create_reset_explorer_conversation(&self, explorer_id: ID) -> ID {
        let state = SendingExplorerReset::new(self.orch_context.clone(), explorer_id);
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = ResetExplorerConversation::<SendingExplorerReset>::new(id, state);

        self.convo_scheduler.add_conversation(
            Box::new(new_conv) as Box<dyn conversations::Conversation + Send + Sync>
        );

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "ScheduleConversation",
                conversation_id: id,
                kind: "ResetExplorer",
                explorer_id: explorer_id
            ),
        );

        id
    }

    pub(crate) fn create_supported_resources_conversation(&self, explorer_id: ID) -> ID {
        let state = SendingSupportedResourcesRequest::new(self.orch_context.clone(), explorer_id);
        let id = get_id_manager().get_next_conversation_id();
        let new_conv =
            SupportedResourcesConversation::<SendingSupportedResourcesRequest>::new(id, state);

        self.convo_scheduler.add_conversation(
            Box::new(new_conv) as Box<dyn conversations::Conversation + Send + Sync>
        );

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "ScheduleConversation",
                conversation_id: id,
                kind: "SupportedResources",
                explorer_id: explorer_id
            ),
        );

        id
    }

    pub(crate) fn create_supported_combinations_conversation(&self, explorer_id: ID) -> ID {
        let state = SendingSupportedCombinationRequest::new(self.orch_context.clone(), explorer_id);
        let id = get_id_manager().get_next_conversation_id();
        let new_conv =
            SupportedCombinationConversation::<SendingSupportedCombinationRequest>::new(id, state);

        self.convo_scheduler.add_conversation(
            Box::new(new_conv) as Box<dyn conversations::Conversation + Send + Sync>
        );

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "ScheduleConversation",
                conversation_id: id,
                kind: "SupportedCombination",
                explorer_id: explorer_id
            ),
        );

        id
    }
}
