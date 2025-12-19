#![allow(dead_code)]

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

    /// Sends an `OrchestratorToPlanet` to the correspondent `planet_id`. Returns nothing if successful, a String error otherwise
    fn to_planet(&self, planet_id : ID, msg : OrchestratorToPlanet) -> Result<(), String>{
        self
            .planets_senders
            .get(&planet_id)
            .ok_or(format!("Planet {planet_id} not found"))?
            .send(msg)
            .map_err(|err| format!("Failed to send to Planet {planet_id}: {err}"))
    }

    /// Sends an `OrchestratorToExplorer` to the correspondent `explorer_id`. Returns nothing if successful, a String error otherwise
    fn to_explorer(&self, explorer_id : ID, msg : OrchestratorToExplorer) -> Result<(), String>{
        self
            .explorer_senders
            .get(&explorer_id)
            .ok_or(format!("Explorer {explorer_id} not found"))?
            .send(msg)
            .map_err(|err| format!("Failed to send to Explorer {explorer_id}: {err}"))
    }

    ///This function handles the incoming messages from a planet
    ///Returns an optional tuple with the `planet_id` and the message to send to the planet as a response
    fn handle_planet_message(
        &mut self,
        message: PlanetToOrchestrator,
    ) -> Option<(u32, OrchestratorToPlanet)> {
        match message {
            PlanetToOrchestrator::Stopped { planet_id } => {
                println!("Planet {planet_id} AI is currently stopped");
                None
            }

            PlanetToOrchestrator::KillPlanetResult { planet_id } => {
                //TODO: erase planet from map
                self.planets_senders.remove(&planet_id);
                println!("Planet {planet_id} has been killed");
                None
            }

            PlanetToOrchestrator::StartPlanetAIResult { planet_id } => {
                println!("Planet {planet_id} has been correctly started");
                None
            }

            PlanetToOrchestrator::StopPlanetAIResult { planet_id } => {
                println!("Planet {planet_id} has been correctly stopped");
                None
            }

            PlanetToOrchestrator::SunrayAck { planet_id } => {
                println!("Planet {planet_id} received a sunray");
                None
            }

            PlanetToOrchestrator::InternalStateResponse { .. } => {
                //TODO: send planet state to the UI
                None
            }

            PlanetToOrchestrator::IncomingExplorerResponse { planet_id, res, explorer_id } => {
                //TODO: Change when the new common crate version will be released
                match res {
                    Ok(()) => println!("Planet {planet_id} received incoming explorer {explorer_id}"),
                    Err(s) => println!(
                        "Error with incoming explorer {explorer_id} in planet {planet_id}: {s}",
                    ),
                }
                None
            }

            PlanetToOrchestrator::OutgoingExplorerResponse { planet_id, res, explorer_id } => {
                //TODO: Change when the new common crate version will be released
                match res {
                    Ok(()) => println!("Explorer {explorer_id} left planet {planet_id}"),
                    Err(s) => println!(
                        "Error with outgoing explorer {explorer_id} in planet {planet_id}: {s}",
                    ),
                }
                None
            }

            PlanetToOrchestrator::AsteroidAck { planet_id, rocket } => {
                if rocket.is_some() {
                    println!("Planet {planet_id} defended from an asteroid");
                    None
                } else {
                    println!("Planet {planet_id} is going to be destroyed");
                    Some((planet_id, OrchestratorToPlanet::KillPlanet))
                }
            }
        }
    }
}
