use crate::globals::get_id_manager;
use crate::orchestrator::conversations;
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
use crate::orchestrator::conversations::orch_planet::galaxy_events::asteroid_scenario::{AsteroidConversation, SendingAsteroid};
use crate::orchestrator::conversations::orch_planet::galaxy_events::sunray_scenario::{SendSunray, SunrayConversation};
use crate::orchestrator::conversations::orch_planet::lifecycle::internal_state_scenario::SendingInternalStateRequest;
use crate::orchestrator::conversations::orch_planet::lifecycle::kill_planet::{KillPlanetConversation, SendPlanetKill};
use crate::orchestrator::conversations::orch_planet::lifecycle::start_planet::{SendingPlanetStart, StartPlanetConversation};
use crate::orchestrator::conversations::orch_planet::lifecycle::stop_planet::{SendingPlanetStop, StopPlanetConversation};
use crate::orchestrator::conversations::orch_planet;
use crate::orchestrator::ExplorersLocationRef;
use crate::orchestrator::{log_internal, LogTarget};
use crate::orchestrator::{ChannelsManagerRef, ConvoScheduler};
use crate::payload;
use crate::planet::PlanetMap;
use crate::OrchestratorToUiUpdate;
use common_game::components::forge::Forge;
use common_game::logging::Channel;
use common_game::utils::ID;
use std::sync::Arc;

pub(crate) struct ConvoFactory {
    channels_manager: ChannelsManagerRef,
    convo_scheduler: Arc<ConvoScheduler>,
}

impl ConvoFactory {
    
    pub(crate) fn new(channels_manager: ChannelsManagerRef, convo_scheduler: Arc<ConvoScheduler>) -> Self {
        Self { channels_manager, convo_scheduler }
    }
    pub(crate) fn create_neighbors_request_conversation(
        &self,
        galaxy: &PlanetMap,
        explorer_id: ID,
    ) -> ID {
        let state =
            WaitingNeighborsRequest::new(
                self.channels_manager.clone(),
                explorer_id,
                galaxy.clone(),
            );

        let id = get_id_manager().get_next_conversation_id();
        let new_conv = NeighborsDiscoveryConversation::<WaitingNeighborsRequest>::new(id, state);
           

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

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
        explorers_location: &ExplorersLocationRef,
        explorer_id: ID,
        current_planet_id: Option<ID>,
        dst_planet_id: ID,
    ) -> ID {

        
        let state = SendManualMoveRequest::new(
            self.channels_manager.clone(),
            explorer_id,
            current_planet_id,
            dst_planet_id,
            explorers_location.clone(),
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
        galaxy: PlanetMap,
        explorers_location: &ExplorersLocationRef,
        explorer_id: ID,
        current_planet_id: ID,
        dst_planet_id: ID,
    ) -> ID {
        
        let state = WaitingTravelRequest::new(
            self.channels_manager.clone(),
            explorer_id,
            current_planet_id,
            galaxy,
            explorers_location.clone(),
        );

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

    pub(crate) fn create_internal_state_conversation(
        &self,
        planet_id: ID,
    ) -> ID {
        let id = get_id_manager().get_next_conversation_id();
        
        let state = SendingInternalStateRequest::new(self.channels_manager.clone(), planet_id);

        let new_conv = orch_planet::lifecycle::internal_state_scenario::InternalStateConversation::<
            SendingInternalStateRequest,
        >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
            event: "ScheduleConversation",
            conversation_id: id,
            kind: "InternalState",
            planet_id: planet_id
        ),
        );

        id
    }

    pub(crate) fn create_bag_content_conversation(
        &self,
        explorer_id: ID,
    ) -> ID {
        let state = SendingBagContentRequest::new(
            self.channels_manager.clone(),
            explorer_id,
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = BagContentConversation::<SendingBagContentRequest>::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

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
        resource_type: common_game::components::resource::BasicResourceType,
    ) -> ID {
        
        let state = SendingCraftResourceRequest::new(
            self.channels_manager.clone(),
            explorer_id,
            resource_type,
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = CraftResourceConversation::<
            SendingCraftResourceRequest,
        >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

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
        resource_type: common_game::components::resource::ComplexResourceType,
    ) -> ID {
        let state = SendingCombineResourceRequest::new(
            self.channels_manager.clone(),
            explorer_id,
            resource_type,
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = CombineResourceConversation::<
            SendingCombineResourceRequest,
        >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

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

    pub(crate) fn create_start_explorer_conversation(
        &self,
        explorer_id: ID,
    ) -> ID {
        let state = SendingExplorerStart::new(
            self.channels_manager.clone(),
            explorer_id,
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv =
            StartExplorerConversation::<
               SendingExplorerStart,
            >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

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

    pub(crate) fn create_stop_explorer_conversation(
        &self,
        explorer_id: ID,
    ) -> ID {
        let state = SendingExplorerStop::new(
            self.channels_manager.clone(),
            explorer_id,
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = StopExplorerConversation::<
            SendingExplorerStop,
        >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

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
        explorers_location: &ExplorersLocationRef,
        explorer_id: ID,
        planet_id: ID,
        handle_outgoing: bool,
    ) -> ID {
        let state = SendingExplorerKill::new(
            self.channels_manager.clone(),
            explorer_id,
            explorers_location.clone(),
            handle_outgoing,
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = KillExplorerConversation::<SendingExplorerKill>::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

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

    pub(crate) fn create_reset_explorer_conversation(
        &self,
        explorer_id: ID,
    ) -> ID {
        let state = SendingExplorerReset::new(
            self.channels_manager.clone(),
            explorer_id,
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv =
            ResetExplorerConversation::<
                SendingExplorerReset,
            >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

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

    pub(crate) fn create_start_planet_conversation(
        &self,
        planet_id: ID,
    ) -> ID {
        let state = SendingPlanetStart::new(self.channels_manager.clone(), planet_id);
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = StartPlanetConversation::<
            SendingPlanetStart,
        >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
            event: "ScheduleConversation",
            conversation_id: id,
            kind: "StartPlanet",
            planet_id: planet_id
        ),
        );

        id
    }

    pub(crate) fn create_stop_planet_conversation(
        &self,
        planet_id: ID,
    ) -> ID {
        let state = SendingPlanetStop::new(self.channels_manager.clone(), planet_id);
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = StopPlanetConversation::<
            SendingPlanetStop,
        >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
            event: "ScheduleConversation",
            conversation_id: id,
            kind: "StopPlanet",
            planet_id: planet_id
        ),
        );

        id
    }

    pub(crate) fn create_kill_planet_conversation(
        &self,
        explorers_location: &ExplorersLocationRef,
        planet_id: ID,
    ) -> ID {
        let state = SendPlanetKill::new(
            self.channels_manager.clone(),
            planet_id,
            explorers_location.clone(),
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = KillPlanetConversation::<
            SendPlanetKill,
        >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
            event: "ScheduleConversation",
            conversation_id: id,
            kind: "KillPlanet",
            planet_id: planet_id
        ),
        );

        id
    }

    pub(crate) fn create_supported_resources_conversation(
        &self,
        explorer_id: ID,
    ) -> ID {
        let state =
            SendingSupportedResourcesRequest::new(
                self.channels_manager.clone(),
                explorer_id,
            );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = SupportedResourcesConversation::<
            SendingSupportedResourcesRequest,
        >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

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

    pub(crate) fn create_supported_combinations_conversation(
        &self,
        explorer_id: ID,
    ) -> ID {
        let state =
           SendingSupportedCombinationRequest::new(
                self.channels_manager.clone(),
                explorer_id,
            );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv =
           SupportedCombinationConversation::<
                SendingSupportedCombinationRequest,
            >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

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

    pub(crate) fn create_asteroid_conversation(
        &self,
        forge: &Arc<Forge>,
        explorers_location: &ExplorersLocationRef,
        planet_id: ID,
    ) -> ID {
        let state = SendingAsteroid::new(
            self.channels_manager.clone(),
            planet_id,
            forge.clone(),
            explorers_location.clone(),
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = AsteroidConversation::<
            SendingAsteroid,
        >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);
        
        //TODO: IS THIS NEEDED? DO WE INSERT IN THE CONVOS AS THE OTHERS?
        self.channels_manager.read().unwrap().get_ui_sender()
            .send(OrchestratorToUiUpdate::SendAutoAsteroid(planet_id))
            .unwrap();

        log_internal(
            LogTarget::AsteroidsSunrays,
            Channel::Trace,
            payload!(
            event: "ScheduleConversation",
            conversation_id: id,
            kind: "Asteroid",
            planet_id: planet_id
        ),
        );
        id
    }

    pub(crate) fn create_sunray_conversation(
        &self,
        forge: &Arc<Forge>,
        planet_id: ID,
    ) -> ID {
        let state = SendSunray::new(
            self.channels_manager.clone(),
            planet_id,
            forge.clone(),
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = SunrayConversation::<
            SendSunray,
        >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);
        
        //TODO: IS THIS NEEDED? DO WE INSERT IN THE CONVOS AS THE OTHERS?
        self.channels_manager.read().unwrap().get_ui_sender()
            .send(OrchestratorToUiUpdate::SendAutoSunray(planet_id))
            .unwrap();

        // Log scheduling of sunray conversation
        log_internal(
            LogTarget::AsteroidsSunrays,
            Channel::Trace,
            payload!(
            event: "ScheduleConversation",
            conversation_id: id,
            kind: "Sunray",
            planet_id: planet_id
        ),
        );

        id
    }  
}


