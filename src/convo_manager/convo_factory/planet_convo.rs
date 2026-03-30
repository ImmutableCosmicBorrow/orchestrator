use std::sync::Arc;
use common_game::components::forge::Forge;
use common_game::logging::Channel;
use common_game::utils::ID;
use crate::convo_manager::ConvoManager;
use crate::{get_id_manager, payload};
use crate::logging_utils::{log_internal, LogTarget};
use crate::orchestrator::{conversations, ChannelsManagerRef, ExplorersLocationRef};
use crate::orchestrator::conversations::orch_explorer::resources::supported_combination::{SendingSupportedCombinationRequest, SupportedCombinationConversation};
use crate::orchestrator::conversations::orch_explorer::resources::supported_resources::{SendingSupportedResourcesRequest, SupportedResourcesConversation};
use crate::orchestrator::conversations::orch_planet;
use crate::orchestrator::conversations::orch_planet::galaxy_events::asteroid_scenario::{AsteroidConversation, SendingAsteroid};
use crate::orchestrator::conversations::orch_planet::galaxy_events::sunray_scenario::{SendSunray, SunrayConversation};
use crate::orchestrator::conversations::orch_planet::lifecycle::internal_state_scenario::SendingInternalStateRequest;
use crate::orchestrator::conversations::orch_planet::lifecycle::kill_planet::{KillPlanetConversation, SendPlanetKill};
use crate::orchestrator::conversations::orch_planet::lifecycle::start_planet::{SendingPlanetStart, StartPlanetConversation};
use crate::orchestrator::conversations::orch_planet::lifecycle::stop_planet::{SendingPlanetStop, StopPlanetConversation};
use crate::ui::OrchestratorToUiUpdate;

impl ConvoManager {
    pub(crate) fn create_internal_state_conversation(
        &self,
        planet_id: ID,
    ) -> ID {
        let id = get_id_manager().get_next_conversation_id();

        let state = SendingInternalStateRequest::new(self.orch_context.channels_manager.clone(), planet_id);

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

    pub(crate) fn create_start_planet_conversation(
        &self,
        planet_id: ID,
    ) -> ID {
        let state = SendingPlanetStart::new(self.orch_context.channels_manager.clone(), planet_id);
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
        let state = SendingPlanetStop::new(self.orch_context.channels_manager.clone(), planet_id);
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
        planet_id: ID,
    ) -> ID {
        let state = SendPlanetKill::new(
            self.orch_context.channels_manager.clone(),
            planet_id,
            self.orch_context.explorers_location.clone(),
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
    
    pub(crate) fn create_asteroid_conversation(
        &self,
        planet_id: ID,
    ) -> ID {
        let state = SendingAsteroid::new(
            self.orch_context.channels_manager.clone(),
            planet_id,
            self.orch_context.forge.clone(),
            self.orch_context.explorers_location.clone(),
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = AsteroidConversation::<
            SendingAsteroid,
        >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

        //TODO: IS THIS NEEDED? DO WE INSERT IN THE CONVOS AS THE OTHERS?
        self.orch_context.channels_manager.read().unwrap().get_ui_sender()
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
        planet_id: ID,
    ) -> ID {
        let state = SendSunray::new(
            self.orch_context.channels_manager.clone(),
            planet_id,
            self.orch_context.forge.clone(),
        );
        let id = get_id_manager().get_next_conversation_id();
        let new_conv = SunrayConversation::<
            SendSunray,
        >::new(id, state);

        self.convo_scheduler.add_conversation(Box::new(new_conv)
            as Box<dyn conversations::Conversation + Send + Sync>);

        //TODO: IS THIS NEEDED? DO WE INSERT IN THE CONVOS AS THE OTHERS?
        self.orch_context.channels_manager.read().unwrap().get_ui_sender()
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