use crate::Orchestrator;
use crate::logging::{LogTarget, log_internal};
use crate::payload;
use crate::ui::OrchestratorToUiUpdate;
use crate::ui::UiToOrchestratorCommand as UiCmd;
use common_game::logging::Channel;

impl Orchestrator {
    /// Handles UI commands from the UI layer and creates appropriate conversations or performs direct actions.
    ///
    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    #[allow(clippy::too_many_lines)]
    pub fn handle_ui_message(&mut self, command: UiCmd) {
        match command {
            // Rendering/Query Commands - Direct responses without conversations
            UiCmd::GetGalaxy => {
                self.send_ui_msg(OrchestratorToUiUpdate::Galaxy(
                    self.orch_context_ref.galaxy.clone(),
                ));
            }
            UiCmd::GetExplorersPosition => {
                self.send_ui_msg(OrchestratorToUiUpdate::ExplorersPosition(
                    self.orch_context_ref.explorers_location.clone(),
                ));
            }
            UiCmd::GetPlanetSnapshot(planet_id) => {
                self.convo_manager
                    .create_internal_state_conversation(planet_id); //the conversation will send the update to UI
            }

            UiCmd::GetExplorerSnapshot(explorer_id) => {
                self.convo_manager
                    .create_bag_content_conversation(explorer_id); //the conversation will send the update to UI
            }

            UiCmd::AddPlanet(planet_kind, connected_planets) => {
                self.add_planet(planet_kind, connected_planets);
            }

            UiCmd::AddExplorer(explorer_type, into_planet) => {
                self.add_explorer(explorer_type, into_planet);
            }

            UiCmd::SwitchGameMode => {
                self.change_mode();
            }
            UiCmd::EndGame => {
                log_internal(
                    LogTarget::General,
                    Channel::Info,
                    payload!(
                        action : "Received EndGame command from UI. Shutting down orchestrator",
                    ),
                );
                self.shutdown_requested = true;
            }
            UiCmd::PauseGame => {
                Orchestrator::stop_background_event_senders();

                for explorer_id in self.orch_context_ref.channels_manager.get_explorer_list() {
                    self.convo_manager
                        .create_stop_explorer_conversation(explorer_id);
                }

                for planet_id in self.orch_context_ref.channels_manager.get_planet_list() {
                    self.convo_manager
                        .create_stop_planet_conversation(planet_id);
                }

                self.convo_manager.convo_scheduler.set_stopped(true);

                log_internal(
                    LogTarget::General,
                    Channel::Info,
                    payload!(
                        action : "Received PauseGame command from UI. Pausing background events and planet/explorer AIs",
                    ),
                );
            }
            UiCmd::ResumeGame => {
                self.start_background_event_senders();

                for explorer_id in self.orch_context_ref.channels_manager.get_explorer_list() {
                    self.convo_manager
                        .create_start_explorer_conversation(explorer_id);
                }

                for planet_id in self.orch_context_ref.channels_manager.get_planet_list() {
                    self.convo_manager
                        .create_start_planet_conversation(planet_id);
                }

                log_internal(
                    LogTarget::General,
                    Channel::Info,
                    payload!(
                        action : "Received ResumeGame command from UI. Resuming background events and planet/explorer AIs",
                    ),
                );
            }

            // Explorer Movement Commands
            UiCmd::ManualMoveExplorer(explorer_id, current_planet, dst_planet) => {
                self.convo_manager.create_send_manual_move_conversation(
                    explorer_id,
                    current_planet,
                    dst_planet,
                );
            }

            // Explorer Resource Commands
            UiCmd::ExplorerGenerateResource(explorer_id, resource_type) => {
                self.convo_manager
                    .create_generate_resource_conversation(explorer_id, resource_type);
            }
            UiCmd::ExplorerCombineResource(explorer_id, resource_type) => {
                self.convo_manager
                    .create_combine_resource_conversation(explorer_id, resource_type);
            }

            UiCmd::SupportedCombinations(explorer_id) => {
                //it automatically sends the update to UI
                self.convo_manager
                    .create_supported_combinations_conversation(explorer_id);
            }

            UiCmd::SupportedResources(explorer_id) => {
                //it automatically sends the update to UI
                self.convo_manager
                    .create_supported_resources_conversation(explorer_id);
            }

            // Asteroid/Sunray Commands
            UiCmd::SendManualAsteroid(planet_id) => {
                self.convo_manager.create_asteroid_conversation(planet_id);
            }

            UiCmd::SendManualSunray(planet_id) => {
                self.convo_manager.create_sunray_conversation(planet_id);
            }

            // Planet AI Control Commands
            UiCmd::StartPlanetAI(planet_id) => {
                self.convo_manager
                    .create_start_planet_conversation(planet_id);
            }
            UiCmd::StopPlanetAI(planet_id) => {
                self.convo_manager
                    .create_stop_planet_conversation(planet_id);
            }
            UiCmd::ResetPlanetAI(planet_id) => {
                // morally a stop + start
                self.convo_manager
                    .create_stop_planet_conversation(planet_id);
                self.convo_manager
                    .create_start_planet_conversation(planet_id);
            }
            UiCmd::KillPlanet(planet_id) => {
                self.convo_manager
                    .create_kill_planet_conversation(planet_id);
            }

            // Explorer AI Control Commands
            UiCmd::StartExplorerAI(explorer_id) => {
                self.convo_manager
                    .create_start_explorer_conversation(explorer_id);
            }
            UiCmd::StopExplorerAI(explorer_id) => {
                self.convo_manager
                    .create_stop_explorer_conversation(explorer_id);
            }
            UiCmd::ResetExplorerAI(explorer_id) => {
                self.convo_manager
                    .create_reset_explorer_conversation(explorer_id);
            }
            UiCmd::KillExplorer(explorer_id) => {
                self.convo_manager.create_kill_explorer_conversation(
                    explorer_id,
                    *self
                        .orch_context_ref
                        .explorers_location
                        .get(&explorer_id)
                        .expect("Explorer not found in explorers_location when trying to kill it"),
                    true,
                );
            }
        }
    }

    fn send_ui_msg(&self, msg: OrchestratorToUiUpdate) {
        let _ = self
            .orch_context_ref
            .channels_manager
            .get_ui_sender_ref()
            .send(msg);
    }
}
