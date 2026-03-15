use crate::OrchestratorToUiUpdate;
use crate::channels_manager::{OrchToExplorerSenders, OrchToPlanetSenders, PlanetExplorerChannels};
use crate::globals::get_id_manager;
use crate::orchestrator::ConvoScheduler;
use crate::orchestrator::ExplorersLocationRef;
use crate::orchestrator::conversations;
use crate::orchestrator::conversations::ToExplorerStruct;
use crate::orchestrator::conversations::ToPlanetStruct;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::MoveToPlanetConversation;
use crate::orchestrator::conversations::orch_explorer::movement::move_to_planet::SendManualMoveRequest;
use crate::orchestrator::conversations::orch_planet::lifecycle::internal_state_scenario::SendingInternalStateRequest;
use crate::orchestrator::conversations::{orch_explorer, orch_planet};
use crate::orchestrator::{LogTarget, log_internal};
use crate::payload;
use crate::planet::PlanetMap;
use common_explorer::ExplorerBagContent;
use common_game::components::forge::Forge;
use common_game::logging::Channel;
use common_game::utils::ID;
use crossbeam_channel::Sender;
use std::sync::Arc;

pub(crate) fn create_neighbors_request_conversation(
    galaxy: &PlanetMap,
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &OrchToExplorerSenders,
    explorer_id: ID,
) -> ID {
    let to_explorer_struct = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state =
        conversations::orch_explorer::movement::neighbors_discovery::WaitingExplorerNeighborsRequest::new(
            to_explorer_struct,
            galaxy.clone(),
        );

    let id = get_id_manager().get_next_conversation_id();
    let new_conv =
        conversations::orch_explorer::movement::neighbors_discovery::NeighborsDiscoveryConversation::<
            conversations::orch_explorer::movement::neighbors_discovery::WaitingExplorerNeighborsRequest,
        >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    planet_explorer_channels: &PlanetExplorerChannels,
    explorer_senders: &OrchToExplorerSenders,
    planets_senders: &OrchToPlanetSenders,
    explorers_location: &ExplorersLocationRef,
    explorer_id: ID,
    current_planet_id: Option<ID>,
    dst_planet_id: ID,
) -> ID {
    let to_explorer_struct = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let curr_planet_struct =
        current_planet_id.map(|id| ToPlanetStruct::new(planets_senders.clone(), id));

    let dst_planet_struct = ToPlanetStruct::new(planets_senders.clone(), dst_planet_id);
    let state = SendManualMoveRequest::new(
        explorers_location.clone(),
        curr_planet_struct,
        dst_planet_struct,
        to_explorer_struct,
        planet_explorer_channels.clone(),
    );

    let id = get_id_manager().get_next_conversation_id();
    let new_conv = MoveToPlanetConversation::<SendManualMoveRequest>::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv));

    log_internal(
        LogTarget::Conversations,
        Channel::Trace,
        payload!(
            event: "ScheduleConversation",
            conversation_id: id,
            kind: "MoveToPlanet",
            explorer_id: explorer_id,
            from_planet: format!("{current_planet_id:?}"),
            to_planet: dst_planet_id
        ),
    );

    id
}

pub(crate) fn create_internal_state_conversation(
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    planets_senders: &OrchToPlanetSenders,
    ui_sender: Sender<OrchestratorToUiUpdate>,
    planet_id: ID,
) -> ID {
    let id = get_id_manager().get_next_conversation_id();

    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);

    let state = SendingInternalStateRequest::new(to_planet_struct, Some(ui_sender));

    let new_conv = orch_planet::lifecycle::internal_state_scenario::InternalStateConversation::<
        SendingInternalStateRequest,
    >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &OrchToExplorerSenders,
    ui_sender: Sender<OrchestratorToUiUpdate>,
    explorer_id: ID,
) -> ID {
    let to_explorer = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state = orch_explorer::resources::bag_content_scenario::SendingBagContentRequest::new(
        to_explorer,
        Some(ui_sender),
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = orch_explorer::resources::bag_content_scenario::BagContentConversation::<
        orch_explorer::resources::bag_content_scenario::SendingBagContentRequest,
    >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &OrchToExplorerSenders,
    explorer_id: ID,
    resource_type: common_game::components::resource::BasicResourceType,
) -> ID {
    let to_explorer = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state = orch_explorer::resources::craft_resource::SendingCraftResourceRequest::new(
        to_explorer,
        resource_type,
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = orch_explorer::resources::craft_resource::CraftResourceConversation::<
        orch_explorer::resources::craft_resource::SendingCraftResourceRequest,
    >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &OrchToExplorerSenders,
    explorer_id: ID,
    resource_type: common_game::components::resource::ComplexResourceType,
) -> ID {
    let to_explorer = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state = orch_explorer::resources::combine_resource::SendingCombineResourceRequest::new(
        to_explorer,
        resource_type,
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = orch_explorer::resources::combine_resource::CombineResourceConversation::<
        orch_explorer::resources::combine_resource::SendingCombineResourceRequest,
    >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &OrchToExplorerSenders,
    explorer_id: ID,
) -> ID {
    let to_explorer_struct = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state = conversations::orch_explorer::lifecycle::start_explorer::SendingExplorerStart::new(
        to_explorer_struct,
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv =
        conversations::orch_explorer::lifecycle::start_explorer::StartExplorerConversation::<
            conversations::orch_explorer::lifecycle::start_explorer::SendingExplorerStart,
        >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &OrchToExplorerSenders,
    explorer_id: ID,
) -> ID {
    let to_explorer_struct = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state = conversations::orch_explorer::lifecycle::stop_explorer::SendingExplorerStop::new(
        to_explorer_struct,
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = conversations::orch_explorer::lifecycle::stop_explorer::StopExplorerConversation::<
        conversations::orch_explorer::lifecycle::stop_explorer::SendingExplorerStop,
    >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &OrchToExplorerSenders,
    planets_senders: &OrchToPlanetSenders,
    explorers_location: &ExplorersLocationRef,
    explorer_id: ID,
    planet_id: ID,
    handle_outgoing: bool,
) -> ID {
    let to_explorer_struct = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = orch_explorer::lifecycle::kill_explorer::SendingKillExplorer::new(
        to_explorer_struct,
        to_planet_struct,
        handle_outgoing,
        explorers_location.clone(),
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = orch_explorer::lifecycle::kill_explorer::KillExplorerConversation::<
        orch_explorer::lifecycle::kill_explorer::SendingKillExplorer,
    >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &OrchToExplorerSenders,
    explorer_id: ID,
) -> ID {
    let to_explorer_struct = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state = conversations::orch_explorer::lifecycle::reset_explorer::SendingExplorerReset::new(
        to_explorer_struct,
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv =
        conversations::orch_explorer::lifecycle::reset_explorer::ResetExplorerConversation::<
            conversations::orch_explorer::lifecycle::reset_explorer::SendingExplorerReset,
        >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    planets_senders: &OrchToPlanetSenders,
    planet_id: ID,
) -> ID {
    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = orch_planet::lifecycle::start_planet::SendingPlanetStart::new(to_planet_struct);
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = orch_planet::lifecycle::start_planet::StartPlanetConversation::<
        orch_planet::lifecycle::start_planet::SendingPlanetStart,
    >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    planets_senders: &OrchToPlanetSenders,
    planet_id: ID,
) -> ID {
    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = orch_planet::lifecycle::stop_planet::SendingPlanetStop::new(to_planet_struct);
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = orch_planet::lifecycle::stop_planet::StopPlanetConversation::<
        orch_planet::lifecycle::stop_planet::SendingPlanetStop,
    >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    planets_senders: &OrchToPlanetSenders,
    explorer_senders: &OrchToExplorerSenders,
    explorers_location: &ExplorersLocationRef,
    planet_id: ID,
) -> ID {
    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = orch_planet::lifecycle::kill_planet::SendPlanetKill::new(
        to_planet_struct,
        explorers_location.clone(),
        explorer_senders.clone(),
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = orch_planet::lifecycle::kill_planet::KillPlanetConversation::<
        orch_planet::lifecycle::kill_planet::SendPlanetKill,
    >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &OrchToExplorerSenders,
    ui_sender: Sender<OrchestratorToUiUpdate>,
    explorer_id: ID,
) -> ID {
    let to_explorer = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state =
        orch_explorer::resources::supported_resources::SendingSupportedResourcesRequest::new(
            to_explorer,
            Some(ui_sender),
        );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = orch_explorer::resources::supported_resources::SupportedResourcesConversation::<
        orch_explorer::resources::supported_resources::SendingSupportedResourcesRequest,
    >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    explorer_senders: &OrchToExplorerSenders,
    ui_sender: Sender<OrchestratorToUiUpdate>,
    explorer_id: ID,
) -> ID {
    let to_explorer = ToExplorerStruct::new(explorer_senders.clone(), explorer_id);
    let state =
        orch_explorer::resources::supported_combination::SendingSupportedCombinationRequest::new(
            to_explorer,
            Some(ui_sender),
        );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv =
        orch_explorer::resources::supported_combination::SupportedCombinationConversation::<
            orch_explorer::resources::supported_combination::SendingSupportedCombinationRequest,
        >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    planets_senders: &OrchToPlanetSenders,
    ui_sender: &Sender<OrchestratorToUiUpdate>,
    forge: &Arc<Forge>,
    explorers_location: &ExplorersLocationRef,
    explorer_senders: &OrchToExplorerSenders,
    planet_id: ID,
) -> ID {
    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = orch_planet::galaxy_events::asteroid_scenario::SendingAsteroid::new(
        to_planet_struct,
        forge.clone(),
        explorers_location.clone(),
        explorer_senders.clone(),
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = orch_planet::galaxy_events::asteroid_scenario::AsteroidConversation::<
        orch_planet::galaxy_events::asteroid_scenario::SendingAsteroid,
    >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

    ui_sender
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
    convo_scheduler: &ConvoScheduler<ExplorerBagContent>,
    planets_senders: &OrchToPlanetSenders,
    ui_sender: &Sender<OrchestratorToUiUpdate>,
    forge: &Arc<Forge>,
    planet_id: ID,
) -> ID {
    let to_planet_struct = ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = orch_planet::galaxy_events::sunray_scenario::SendSunray::new(
        to_planet_struct,
        forge.clone(),
    );
    let id = get_id_manager().get_next_conversation_id();
    let new_conv = orch_planet::galaxy_events::sunray_scenario::SunrayConversation::<
        orch_planet::galaxy_events::sunray_scenario::SendSunray,
    >::new(id, state);

    convo_scheduler.add_conversation(Box::new(new_conv)
        as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

    ui_sender
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
