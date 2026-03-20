use super::{ID, Orchestrator, convo_factory};

pub(crate) struct ConvoRouter<'a> {
    orch: &'a Orchestrator,
}

impl<'a> ConvoRouter<'a> {
    pub(crate) fn new(orch: &'a Orchestrator) -> Self {
        Self { orch }
    }

    /// for creating orchestrator conversations and controlling entities.
    pub fn ask_neighbors(&self, explorer_id: ID) {
        convo_factory::create_neighbors_request_conversation(
            &self.orch.galaxy,
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            explorer_id,
        );
    }

    /// Create travel-to-planet conversation.
    pub fn make_manual_travel_to_planet_request(
        &self,
        explorer_id: ID,
        current_planet_id: Option<ID>,
        dst_planet_id: ID,
    ) {
        //TODO: CHANGE THIS TO CREATE WAITING TRAVEL PLANET REQUEST
        convo_factory::create_send_manual_move_conversation(
            &self.orch.convo_scheduler,
            self.orch.channels_manager.get_planet_explorer_struct(),
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            self.orch
                .channels_manager
                .get_to_planet_senders_struct_ref(),
            &self.orch.explorers_location,
            explorer_id,
            current_planet_id,
            dst_planet_id,
        );
    }

    /// Create internal state conversation for a planet.
    pub fn ask_internal_state(&self, planet_id: ID) {
        convo_factory::create_internal_state_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_to_planet_senders_struct_ref(),
            self.orch.channels_manager.get_ui_sender(),
            planet_id,
        );
    }

    /// Create bag content conversation for an explorer.
    pub fn ask_bag_content(&self, explorer_id: ID) {
        convo_factory::create_bag_content_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            self.orch.channels_manager.get_ui_sender(),
            explorer_id,
        );
    }

    /// Create generate resource conversation.
    pub fn generate_resource(
        &self,
        explorer_id: ID,
        resource_type: common_game::components::resource::BasicResourceType,
    ) {
        convo_factory::create_generate_resource_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            explorer_id,
            resource_type,
        );
    }

    /// Create combine resource conversation.
    pub fn combine_resource(
        &self,
        explorer_id: ID,
        resource_type: common_game::components::resource::ComplexResourceType,
    ) {
        convo_factory::create_combine_resource_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            explorer_id,
            resource_type,
        );
    }

    /// Start explorer AI conversation.
    pub fn start_explorer_ai(&self, explorer_id: ID) {
        convo_factory::create_start_explorer_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            explorer_id,
        );
    }

    /// Stop explorer AI conversation.
    pub fn stop_explorer_ai(&self, explorer_id: ID) {
        convo_factory::create_stop_explorer_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            explorer_id,
        );
    }

    /// Kill explorer conversation.
    pub fn kill_explorer(&self, explorer_id: ID, planet_id: ID, handle_outgoing: bool) {
        convo_factory::create_kill_explorer_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            self.orch
                .channels_manager
                .get_to_planet_senders_struct_ref(),
            &self.orch.explorers_location,
            explorer_id,
            planet_id,
            handle_outgoing,
        );
    }

    /// Reset explorer conversation.
    pub fn reset_explorer(&self, explorer_id: ID) {
        convo_factory::create_reset_explorer_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            explorer_id,
        );
    }

    /// Start planet AI conversation.
    pub fn start_planet_ai(&self, planet_id: ID) {
        convo_factory::create_start_planet_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_to_planet_senders_struct_ref(),
            planet_id,
        );
    }

    /// Stop planet AI conversation.
    pub fn stop_planet_ai(&self, planet_id: ID) {
        convo_factory::create_stop_planet_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_to_planet_senders_struct_ref(),
            planet_id,
        );
    }

    pub fn kill_planet(&self, planet_id: ID) {
        convo_factory::create_kill_planet_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_to_planet_senders_struct_ref(),
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            &self.orch.explorers_location,
            planet_id,
        );
    }

    /// Supported resources conversation.
    pub fn ask_supported_resources(&self, explorer_id: ID) {
        convo_factory::create_supported_resources_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            self.orch.channels_manager.get_ui_sender(),
            explorer_id,
        );
    }

    pub fn ask_supported_combinations(&self, explorer_id: ID) {
        convo_factory::create_supported_combinations_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            self.orch.channels_manager.get_ui_sender(),
            explorer_id,
        );
    }

    pub fn send_asteroid(&self, planet_id: ID) {
        convo_factory::create_asteroid_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_to_planet_senders_struct_ref(),
            self.orch.channels_manager.get_ui_sender_ref(),
            &self.orch.forge,
            &self.orch.explorers_location,
            self.orch
                .channels_manager
                .get_orch_to_exp_senders_struct_ref(),
            planet_id,
        );
    }

    pub fn send_sunray(&self, planet_id: ID) {
        convo_factory::create_sunray_conversation(
            &self.orch.convo_scheduler,
            self.orch
                .channels_manager
                .get_to_planet_senders_struct_ref(),
            self.orch.channels_manager.get_ui_sender_ref(),
            &self.orch.forge,
            planet_id,
        );
    }
}
