#![allow(dead_code)]

use crate::planet::{Alive, PlanetNode};
use common_game::components::forge::Forge;
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::protocols::planet_explorer::{ExplorerToPlanet, PlanetToExplorer};
use common_game::utils::ID;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::fmt::format;
use std::sync::{Arc, Mutex};

mod conversations;

pub(crate) struct Orchestrator<T> {
    planets_senders: Arc<Mutex<HashMap<ID, Sender<OrchestratorToPlanet>>>>,
    explorer_senders: Arc<Mutex<HashMap<ID, Sender<OrchestratorToExplorer>>>>,
    planets_receiver: Receiver<PlanetToOrchestrator>,
    explorers_receiver: Receiver<OrchestratorToExplorer>,
    forge: Forge,
    explorer_bag: HashMap<ID, T>,
    galaxy: Arc<Mutex<HashMap<ID, Arc<PlanetNode<Alive>>>>>,
    planet_explorer_channels: PlanetExplorerChannels,
}

struct PlanetExplorerChannels {
    planet_to_explorer_senders: HashMap<ID, Sender<PlanetToExplorer>>,
    explorer_to_planet_senders: HashMap<ID, Sender<ExplorerToPlanet>>,
}

impl<T> Orchestrator<T> {
    pub fn new() -> Self {
        todo!()
    }

    /// Sends an `OrchestratorToPlanet` to the correspondent `planet_id`. Returns nothing if successful, a String error otherwise
    fn to_planet(&self, planet_id: ID, msg: OrchestratorToPlanet) -> Result<(), String> {
        self.planets_senders
            .lock()
            .unwrap()
            .get(&planet_id)
            .ok_or(format!("Planet {planet_id} not found"))?
            .send(msg)
            .map_err(|err| format!("Failed to send to Planet {planet_id}: {err}"))
    }

    /// Sends an `OrchestratorToExplorer` to the correspondent `explorer_id`. Returns nothing if successful, a String error otherwise
    fn to_explorer(&self, explorer_id: ID, msg: OrchestratorToExplorer) -> Result<(), String> {
        self.explorer_senders
            .lock()
            .unwrap()
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
    ) -> Option<(ID, OrchestratorToPlanet)> {
        match message {
            PlanetToOrchestrator::Stopped { planet_id } => {
                println!("Planet {planet_id} AI is currently stopped");
                None
            }

            PlanetToOrchestrator::KillPlanetResult { planet_id } => {
                //TODO: erase planet from map
                self.planets_senders.lock().unwrap().remove(&planet_id);
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

            PlanetToOrchestrator::IncomingExplorerResponse {
                planet_id,
                res,
                explorer_id,
            } => {
                //TODO: Change when the new common crate version will be released
                match res {
                    Ok(()) => {
                        println!("Planet {planet_id} received incoming explorer {explorer_id}")
                    }
                    Err(s) => println!(
                        "Error with incoming explorer {explorer_id} in planet {planet_id}: {s}",
                    ),
                }
                None
            }

            PlanetToOrchestrator::OutgoingExplorerResponse {
                planet_id,
                res,
                explorer_id,
            } => {
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
    //This function handles the incoming messages from an Explorer
    //Returns an optional tuple with the `explorer_id` and the message to send to the planet as a response
    /*fn handle_explorer_message(
        &mut self,
        message: ExplorerToOrchestrator<T>,
    ) -> Option<(ID, OrchestratorToExplorer)> {
        match message {
            ExplorerToOrchestrator::CombineResourceResponse {
                explorer_id,
                generated,
            } => {
                match generated {
                    Ok(()) => {
                        println!(
                            "Explorer {explorer_id} successfully crafted the indicated complex resource"
                        )
                    }
                    Err(s) => {
                        println!("Error with explorer {explorer_id}, couldn't craft resource: {s}")
                    }
                }
                None
            }

            ExplorerToOrchestrator::GenerateResourceResponse {
                explorer_id,
                generated,
            } => {
                match generated {
                    Ok(()) => {
                        println!(
                            "Explorer {explorer_id} successfully crafted the indicated complex resource"
                        )
                    }
                    Err(s) => {
                        println!("Error with explorer {explorer_id}, couldn't craft resource: {s}")
                    }
                }
                None
            }

            ExplorerToOrchestrator::NeighborsRequest {
                explorer_id,
                current_planet_id,
            } => {
                let neighbors = self
                    .galaxy
                    .lock()
                    .unwrap()
                    .get(&current_planet_id)
                    .expect("Selected Planet not in galaxy")
                    .get_neighbors();
                Some((
                    explorer_id,
                    OrchestratorToExplorer::NeighborsResponse { neighbors },
                ))
            }

            ExplorerToOrchestrator::BagContentResponse {
                explorer_id,
                bag_content,
            } => {
                //TODO: SEND BAG DATA TO THE UI
                None
            }

            ExplorerToOrchestrator::SupportedCombinationResult {
                explorer_id,
                combination_list,
            } => {
                println!("Explorer {explorer_id} can currently craft:  {combination_list:?}");
                None
            }

            ExplorerToOrchestrator::SupportedResourceResult {
                explorer_id,
                supported_resources,
            } => {
                println!("Explorer {explorer_id} can currently craft:  {supported_resources:?}");
                None
            }

            ExplorerToOrchestrator::CurrentPlanetResult {
                explorer_id,
                planet_id,
            } => {
                println!("Explorer {explorer_id} is currently in planet:  {planet_id}");
                None
            }

            ExplorerToOrchestrator::StartExplorerAIResult { explorer_id } => {
                println!("Explorer {explorer_id} has been successfully started");
                None
            }

            ExplorerToOrchestrator::StopExplorerAIResult { explorer_id } => {
                println!("Explorer {explorer_id} has been successfully stopped");
                None
            }

            ExplorerToOrchestrator::ResetExplorerAIResult { explorer_id } => {
                println!("Explorer {explorer_id} has been successfully reset");
                None
            }

            ExplorerToOrchestrator::KillExplorerResult { explorer_id } => {
                println!("Explorer {explorer_id} has been successfully killed");
                None
            }

            ExplorerToOrchestrator::TravelToPlanetRequest {
                explorer_id,
                current_planet_id,
                dst_planet_id,
            } => {
                let dst_planet = self
                    .galaxy
                    .lock()
                    .unwrap()
                    .get(&current_planet_id)
                    .expect("Selected Planet not in galaxy");
                let is_neighbor = self
                    .galaxy
                    .lock()
                    .unwrap()
                    .get(&current_planet_id)
                    .expect("Selected Planet not in galaxy")
                    .has_neighbor(dst_planet);
                //TODO: HOW TO ASSIGN NEW SENDER? WE SHOULD PROBABLY KEEP THE VALID CHANNELS TO AND FROM EVERY ENTITY
                if is_neighbor {
                    Some((
                        explorer_id,
                        OrchestratorToExplorer::MoveToPlanet {
                            sender_to_new_planet: Some(
                                self.planet_explorer_channels
                                    .explorer_to_planet_senders
                                    .get(&dst_planet_id)
                                    .expect("No registered sender for this planet")
                                    .clone(),
                            ),
                        },
                    ))
                } else {
                    Some((
                        explorer_id,
                        OrchestratorToExplorer::MoveToPlanet {
                            sender_to_new_planet: None,
                        },
                    ))
                }
            }

            //TODO: MAYBE WE WANT TO SEND A ORCH_TO_PLANET_OUTGOING OUT OF THIS?
            ExplorerToOrchestrator::MovedToPlanetResult { explorer_id } => {
                println!("Explorer {explorer_id} moved to planet");
                None
            }
        }
    }
}*/
}