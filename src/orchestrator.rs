use common_game::components::forge::Forge;
use common_game::protocols::orchestrator_explorer::OrchestratorToExplorer;
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use common_game::utils::ID;

pub(crate) struct Orchestrator {
    planets_senders: HashMap<u32, Sender<OrchestratorToPlanet>>,
    explorer_senders: HashMap<u32, Sender<OrchestratorToExplorer>>,
    planets_receiver: Receiver<PlanetToOrchestrator>,
    explorers_receiver: Receiver<OrchestratorToExplorer>,
    forge: Forge,
}

impl Orchestrator {
    pub fn new() -> Self {
        todo!()
    }

    /// Sends an OrchestratorToPlanet to the correspondent planet_id. Returns nothing if successful, a String error otherwise
    fn to_planet(&self, planet_id : ID, msg : OrchestratorToPlanet) -> Result<(), String>{
        self
            .planets_senders
            .get(&planet_id)
            .ok_or(format!("Planet {} not found", planet_id))?
            .send(msg)
            .map_err(|err| format!("Failed to send to Planet {}: {}", planet_id, err))
    }

    /// Sends an OrchestratorToExplorer to the correspondent explorer_id. Returns nothing if successful, a String error otherwise
    fn to_explorer(&self, explorer_id : ID, msg : OrchestratorToExplorer) -> Result<(), String>{
        self
            .explorer_senders
            .get(&explorer_id)
            .ok_or(format!("Explorer {} not found", explorer_id))?
            .send(msg)
            .map_err(|err| format!("Failed to send to Explorer {}: {}", explorer_id, err))
    }

    ///This function handles the incoming messages from a planet
    ///Returns an optional tuple with the planet_id and the message to send to the planet as a response
    fn handle_planet_message(
        &mut self,
        message: PlanetToOrchestrator,
    ) -> Option<(u32, OrchestratorToPlanet)> {
        match message {
            PlanetToOrchestrator::Stopped { planet_id } => {
                println!("Planet {} AI is currently stopped", planet_id);
                None
            }

            PlanetToOrchestrator::KillPlanetResult { planet_id } => {
                //TODO: erase planet from map
                self.planets_senders.remove(&planet_id);
                println!("Planet {} has been killed", planet_id);
                None
            }

            PlanetToOrchestrator::StartPlanetAIResult { planet_id } => {
                println!("Planet {} has been correctly started", planet_id);
                None
            }

            PlanetToOrchestrator::StopPlanetAIResult { planet_id } => {
                println!("Planet {} has been correctly stopped", planet_id);
                None
            }

            PlanetToOrchestrator::SunrayAck { planet_id } => {
                println!("Planet {} received a sunray", planet_id);
                None
            }

            PlanetToOrchestrator::InternalStateResponse { .. } => {
                //TODO: send planet state to the UI
                None
            }

            PlanetToOrchestrator::IncomingExplorerResponse { planet_id, res, explorer_id } => {
                //TODO: Change when the new common crate version will be released
                match res {
                    Ok(_) => println!("Planet {} received incoming explorer {}", planet_id, explorer_id),
                    Err(s) => println!(
                        "Error with incoming explorer {} in planet {}: {}",
                        explorer_id, planet_id, s,
                    ),
                }
                None
            }

            PlanetToOrchestrator::OutgoingExplorerResponse { planet_id, res, explorer_id } => {
                //TODO: Change when the new common crate version will be released
                match res {
                    Ok(_) => println!("Explorer {} left planet {}", explorer_id, planet_id),
                    Err(s) => println!(
                        "Error with outgoing explorer {} in planet {}: {}",
                        explorer_id, planet_id, s
                    ),
                }
                None
            }

            PlanetToOrchestrator::AsteroidAck { planet_id, rocket } => {
                if rocket.is_some() {
                    println!("Planet {} defended from an asteroid", planet_id);
                    None
                } else {
                    println!("Planet {} is going to be destroyed", planet_id);
                    Some((planet_id, OrchestratorToPlanet::KillPlanet))
                }
            }
        }
    }
}
