use crate::OrchestratorToUiUpdate;
use crate::globals::get_id_manager;
use crate::orchestrator::ConvoScheduler;
use crate::orchestrator::ExplorersLocationRef;
use crate::orchestrator::PlanetExplorerChannels;
use crate::orchestrator::SendersToPlanet;
use crate::orchestrator::conversations;
use crate::orchestrator::conversations::SendersToExplorer;
use crate::orchestrator::conversations::ToExplorerStruct;
use crate::orchestrator::conversations::ToPlanetStruct;
use common_explorer::ExplorerBagContent;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::MoveToPlanetConversation;
use crate::orchestrator::conversations::orch_explorer::move_to_planet::WaitingTravelRequest;
use crate::orchestrator::conversations::orch_planet::internal_state_scenario::SendingInternalStateRequest;
use crate::planet::PlanetMap;
use common_game::components::forge::Forge;
use common_game::utils::ID;
use crossbeam_channel::Sender;
use std::sync::Arc;

pub(crate) fn create_neighbors_request_conversation(
    galaxy: &PlanetMap,
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &SendersToExplorer,
    explorer_id: ID,
) -> ID {
    let to_explorer_struct = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state =
        conversations::orch_explorer::neighbors_discovery::WaitingExplorerNeighborsRequest::new(
            to_explorer_struct,
            galaxy.clone(),
        );

    let id = get_id_manager().get_next_conversation_id();
    let new_conv =
        conversations::orch_explorer::neighbors_discovery::NeighborsDiscoveryConversation::<
            conversations::orch_explorer::neighbors_discovery::WaitingExplorerNeighborsRequest,
        >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );

    /*self.handle_message(PossibleMessage::ExplorerToOrch(
        ExplorerToOrchestrator::NeighborsRequest {
            explorer_id,
            current_planet_id,
        },
    ));*/

    id
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn create_travel_to_planet_request_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    galaxy: &PlanetMap,
    planet_explorer_channels: &PlanetExplorerChannels,
    explorer_senders: &SendersToExplorer,
    planets_senders: &SendersToPlanet,
    explorers_location: &ExplorersLocationRef,
    explorer_id: ID,
    current_planet_id: ID,
    dst_planet_id: ID,
) -> ID {
    let to_explorer_struct = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let curr_planet_struct = ToPlanetStruct::new(planets_senders.clone(), current_planet_id);
    let dst_planet_struct = ToPlanetStruct::new(planets_senders.clone(), dst_planet_id);
    let state = WaitingTravelRequest::new(
        galaxy.clone(),
        planet_explorer_channels.clone(),
        curr_planet_struct,
        dst_planet_struct,
        to_explorer_struct,
        explorers_location.clone(),
    );

    let id = get_id_manager().get_next_conversation_id();
    let new_conv = MoveToPlanetConversation::<WaitingTravelRequest>::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv));

    id
}

pub(crate) fn create_internal_state_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    planets_senders: &SendersToPlanet,
    ui_sender: Sender<OrchestratorToUiUpdate>,
    planet_id: ID,
) -> ID {
    let id = get_id_manager().get_next_conversation_id();

    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);

    let state = SendingInternalStateRequest::new(to_planet_struct, Some(ui_sender));

    let new_conv = conversations::orch_planet::internal_state_scenario::InternalStateConversation::<
        SendingInternalStateRequest,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );

    id
}

pub(crate) fn create_bag_content_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &SendersToExplorer,
    ui_sender: Sender<OrchestratorToUiUpdate>,
    explorer_id: ID,
) -> ID {
    let to_explorer = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state = conversations::orch_explorer::bag_content_scenario::SendingBagContentRequest::new(
        to_explorer,
        Some(ui_sender),
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_explorer::bag_content_scenario::BagContentConversation::<
        conversations::orch_explorer::bag_content_scenario::SendingBagContentRequest,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_craft_resource_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &SendersToExplorer,
    explorer_id: ID,
    resource_type: common_game::components::resource::BasicResourceType,
) -> ID {
    let to_explorer = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state = conversations::orch_explorer::craft_resource::SendingCraftResourceRequest::new(
        to_explorer,
        resource_type,
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_explorer::craft_resource::CraftResourceConversation::<
        conversations::orch_explorer::craft_resource::SendingCraftResourceRequest,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_combine_resource_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &SendersToExplorer,
    explorer_id: ID,
    resource_type: common_game::components::resource::ComplexResourceType,
) -> ID {
    let to_explorer = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state = conversations::orch_explorer::combine_resource::SendingCombineResourceRequest::new(
        to_explorer,
        resource_type,
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_explorer::combine_resource::CombineResourceConversation::<
        conversations::orch_explorer::combine_resource::SendingCombineResourceRequest,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_start_explorer_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &SendersToExplorer,
    explorer_id: ID,
) -> ID {
    let to_explorer_struct = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state =
        conversations::orch_explorer::start_explorer::SendingExplorerStart::new(to_explorer_struct);
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_explorer::start_explorer::StartExplorerConversation::<
        conversations::orch_explorer::start_explorer::SendingExplorerStart,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_stop_explorer_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &SendersToExplorer,
    explorer_id: ID,
) -> ID {
    let to_explorer_struct = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state =
        conversations::orch_explorer::stop_explorer::SendingExplorerStop::new(to_explorer_struct);
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_explorer::stop_explorer::StopExplorerConversation::<
        conversations::orch_explorer::stop_explorer::SendingExplorerStop,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_kill_explorer_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &SendersToExplorer,
    planets_senders: &SendersToPlanet,
    explorers_location: &ExplorersLocationRef,
    explorer_id: ID,
    planet_id: ID,
    handle_outgoing: bool,
) -> ID {
    let to_explorer_struct = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = conversations::orch_explorer::kill_explorer::SendingKillExplorer::new(
        to_explorer_struct,
        to_planet_struct,
        handle_outgoing,
        explorers_location.clone(),
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_explorer::kill_explorer::KillExplorerConversation::<
        conversations::orch_explorer::kill_explorer::SendingKillExplorer,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_reset_explorer_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &SendersToExplorer,
    explorer_id: ID,
) -> ID {
    let to_explorer_struct = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state =
        conversations::orch_explorer::reset_explorer::SendingExplorerReset::new(to_explorer_struct);
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_explorer::reset_explorer::ResetExplorerConversation::<
        conversations::orch_explorer::reset_explorer::SendingExplorerReset,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_start_planet_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    planets_senders: &SendersToPlanet,
    planet_id: ID,
) -> ID {
    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = conversations::orch_planet::start_planet::SendingPlanetStart::new(to_planet_struct);
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_planet::start_planet::StartPlanetConversation::<
        conversations::orch_planet::start_planet::SendingPlanetStart,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_stop_planet_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    planets_senders: &SendersToPlanet,
    planet_id: ID,
) -> ID {
    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = conversations::orch_planet::stop_planet::SendingPlanetStop::new(to_planet_struct);
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_planet::stop_planet::StopPlanetConversation::<
        conversations::orch_planet::stop_planet::SendingPlanetStop,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_kill_planet_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    planets_senders: &SendersToPlanet,
    explorer_senders: &SendersToExplorer,
    explorers_location: &ExplorersLocationRef,
    planet_id: ID,
) -> ID {
    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = conversations::orch_planet::kill_planet::SendPlanetKill::new(
        to_planet_struct,
        explorers_location.clone(),
        explorer_senders.clone(),
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_planet::kill_planet::KillPlanetConversation::<
        conversations::orch_planet::kill_planet::SendPlanetKill,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_supported_resources_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &SendersToExplorer,
    ui_sender: Sender<OrchestratorToUiUpdate>,
    explorer_id: ID,
) -> ID {
    let to_explorer = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state =
        conversations::orch_explorer::supported_resources::SendingSupportedResourcesRequest::new(
            to_explorer,
            Some(ui_sender),
        );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv =
        conversations::orch_explorer::supported_resources::SupportedResourcesConversation::<
            conversations::orch_explorer::supported_resources::SendingSupportedResourcesRequest,
        >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_supported_combinations_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &SendersToExplorer,
    ui_sender: Sender<OrchestratorToUiUpdate>,
    explorer_id: ID,
) -> ID {
    let to_explorer = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state = conversations::orch_explorer::supported_combination::SendingSupportedCombinationRequest::new(to_explorer, Some(ui_sender));
    let id = get_id_manager().get_next_conversation_id();
    let new_conv =
        conversations::orch_explorer::supported_combination::SupportedCombinationConversation::<
            conversations::orch_explorer::supported_combination::SendingSupportedCombinationRequest,
        >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_asteroid_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    planets_senders: &SendersToPlanet,
    forge: &Arc<Forge>,
    explorers_location: &ExplorersLocationRef,
    explorer_senders: &SendersToExplorer,
    planet_id: ID,
) -> ID {
    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = conversations::orch_planet::asteroid_scenario::SendingAsteroid::new(
        to_planet_struct,
        forge.clone(),
        explorers_location.clone(),
        explorer_senders.clone(),
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_planet::asteroid_scenario::AsteroidConversation::<
        conversations::orch_planet::asteroid_scenario::SendingAsteroid,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}

pub(crate) fn create_sunray_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    planets_senders: &SendersToPlanet,
    forge: &Arc<Forge>,
    planet_id: ID,
) -> ID {
    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = conversations::orch_planet::sunray_scenario::SendSunray::new(
        to_planet_struct,
        forge.clone(),
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_planet::sunray_scenario::SunrayConversation::<
        conversations::orch_planet::sunray_scenario::SendSunray,
    >::new(id, state);

    convo_scheduler.add_conversation(
        Box::new(new_conv) as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>
    );
    id
}
