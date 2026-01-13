#![allow(dead_code)]

use crate::galaxy_setup::{PlanetMap, galaxy_loader};
use common_game::components::forge::Forge;
use common_game::logging::{ActorType, Channel, EventType, LogEvent, Participant, Payload};
use common_game::protocols::orchestrator_explorer::{
    ExplorerToOrchestrator, OrchestratorToExplorer,
};
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::protocols::planet_explorer::{ExplorerToPlanet, PlanetToExplorer};
use common_game::utils::ID;
use crossbeam_channel::unbounded;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::fmt::Debug;

pub(crate) struct Orchestrator<T: Debug> {
    planets_senders: HashMap<ID, Sender<OrchestratorToPlanet>>,
    explorer_senders: HashMap<ID, Sender<OrchestratorToExplorer>>,
    planets_receiver: Receiver<PlanetToOrchestrator>,
    explorers_receiver: Receiver<ExplorerToOrchestrator<T>>,
    forge: Forge,
    explorer_bag: HashMap<ID, T>,
    galaxy: PlanetMap,
    planet_explorer_channels: PlanetExplorerChannels,
}

struct PlanetExplorerChannels {
    planet_to_explorer_senders: HashMap<ID, Sender<PlanetToExplorer>>, //here ID is explorer_id
    explorer_to_planet_senders: HashMap<ID, Sender<ExplorerToPlanet>>, //here ID is planet_id
}

impl PlanetExplorerChannels {
    pub fn new() -> Self {
        Self {
            planet_to_explorer_senders: HashMap::new(),
            explorer_to_planet_senders: HashMap::new(),
        }
    }

    pub fn add_plan_to_expl_sender(&mut self, explorer_id: ID, sender: Sender<PlanetToExplorer>) {
        self.planet_to_explorer_senders.insert(explorer_id, sender);
    }

    pub fn add_expl_to_plan_sender(&mut self, planet_id: ID, sender: Sender<ExplorerToPlanet>) {
        self.explorer_to_planet_senders.insert(planet_id, sender);
    }

    pub fn get_plan_to_expl_sender(&self, explorer_id: &ID) -> Option<&Sender<PlanetToExplorer>> {
        self.planet_to_explorer_senders.get(explorer_id)
    }

    pub fn get_expl_to_plan_sender(&self, planet_id: &ID) -> Option<&Sender<ExplorerToPlanet>> {
        self.explorer_to_planet_senders.get(planet_id)
    }
}

impl<T: Debug> Orchestrator<T> {
    pub fn new(file_path: &std::path::Path) -> Self {
        let mut planet_explorer_channels = PlanetExplorerChannels::new();

        let (galaxy, planets_receiver, orch_to_plan_senders, expl_to_plan_senders) =
            galaxy_loader(file_path);
        let (explorers_receiver, explorer_senders) =
            (unbounded::<ExplorerToOrchestrator<T>>().1, HashMap::new()); //TODO: save ExplorerToOrchestrator sender to pass it to new explorers 
        let forge = Forge::new().expect("Couldn't create forge!");
        let explorer_bag = HashMap::new();
        planet_explorer_channels.explorer_to_planet_senders = expl_to_plan_senders;

        Self {
            planets_senders: orch_to_plan_senders,
            explorer_senders,
            planets_receiver,
            explorers_receiver,
            forge,
            explorer_bag,
            galaxy,
            planet_explorer_channels,
        }
    }

    /// Sends an `OrchestratorToPlanet` to the correspondent `planet_id`. Returns nothing if successful, a String error otherwise
    fn to_planet(&self, planet_id: ID, msg: OrchestratorToPlanet) -> Result<(), String> {
        let mut payload = Payload::new();
        payload.insert("message".into(), format!("{msg:?}"));

        let result = self
            .planets_senders
            .get(&planet_id)
            .ok_or(format!("Planet {planet_id} not found"))?
            .send(msg)
            .map_err(|err| format!("Failed to send to Planet {planet_id}: {err}"));

        payload.insert("success".into(), result.is_ok().to_string());

        let mut channel = Channel::Trace;
        if let Err(ref error) = result {
            payload.insert("error".into(), error.to_string());
            channel = Channel::Error;
        }

        LogEvent::new(
            Some(Participant::new(ActorType::Orchestrator, 0u32)),
            Some(Participant::new(ActorType::Planet, planet_id)),
            EventType::MessageOrchestratorToPlanet,
            channel,
            payload,
        )
        .emit();

        result
    }

    /// Sends an `OrchestratorToExplorer` to the correspondent `explorer_id`. Returns nothing if successful, a String error otherwise
    fn to_explorer(&self, explorer_id: ID, msg: OrchestratorToExplorer) -> Result<(), String> {
        let mut payload = Payload::new();
        payload.insert("message".into(), format!("{msg:?}"));

        let result = self
            .explorer_senders
            .get(&explorer_id)
            .ok_or(format!("Explorer {explorer_id} not found"))?
            .send(msg)
            .map_err(|err| format!("Failed to send to Explorer {explorer_id}: {err}"));

        payload.insert("success".into(), result.is_ok().to_string());

        let mut channel = Channel::Trace;
        if let Err(ref error) = result {
            payload.insert("error".into(), error.to_string());
            channel = Channel::Error;
        }

        LogEvent::new(
            Some(Participant::new(ActorType::Orchestrator, 0u32)),
            Some(Participant::new(ActorType::Explorer, explorer_id)),
            EventType::MessageOrchestratorToExplorer,
            channel,
            payload,
        )
        .emit();

        result
    }

    ///This function handles the incoming messages from a planet
    ///Returns an optional tuple with the `planet_id` and the message to send to the planet as a response
    fn handle_planet_message(
        &mut self,
        message: PlanetToOrchestrator,
    ) -> Option<(ID, OrchestratorToPlanet)> {
        match message {
            PlanetToOrchestrator::Stopped { planet_id } => {
                let mut payload = Payload::new();
                payload.insert("event".into(), "Planet is stopped".into());
                payload.insert("planet_id".into(), planet_id.to_string());

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Warning,
                    payload,
                )
                .emit();
                None
            }

            PlanetToOrchestrator::KillPlanetResult { planet_id } => {
                //TODO: erase planet from map
                self.planets_senders.remove(&planet_id);

                let mut payload = Payload::new();
                payload.insert(
                    "event".to_string(),
                    "Planet has been correctly killed".into(),
                );
                payload.insert("planet_id".into(), planet_id.to_string());

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Info,
                    payload,
                )
                .emit();
                None
            }

            PlanetToOrchestrator::StartPlanetAIResult { planet_id } => {
                let mut payload = Payload::new();
                payload.insert("event".into(), "Planet has been correctly started".into());
                payload.insert("planet_id".into(), planet_id.to_string());
                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Info,
                    payload,
                )
                .emit();
                None
            }

            PlanetToOrchestrator::StopPlanetAIResult { planet_id } => {
                let mut payload = Payload::new();
                payload.insert("event".into(), "Planet has been correctly stopped".into());
                payload.insert("planet_id".into(), planet_id.to_string());

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Info,
                    payload,
                )
                .emit();
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
                        println!("Planet {planet_id} received incoming explorer {explorer_id}");
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
    ///This function handles the incoming messages from an Explorer
    ///Returns an optional tuple with the `explorer_id` and the message to send to the planet as a response
    fn handle_explorer_message(
        &mut self,
        message: ExplorerToOrchestrator<T>,
    ) -> Option<(ID, OrchestratorToExplorer)> {
        match message {
            ExplorerToOrchestrator::CombineResourceResponse {
                explorer_id,
                generated,
            }
            | ExplorerToOrchestrator::GenerateResourceResponse {
                explorer_id,
                generated,
            } => {
                match generated {
                    Ok(()) => {
                        let mut payload = Payload::new();
                        payload.insert(
                            "event".into(),
                            "Explorer successfully crafted the requested complex resource".into(),
                        );
                        payload.insert("explorer_id".into(), explorer_id.to_string());
                        LogEvent::system(
                            EventType::InternalOrchestratorAction,
                            Channel::Debug,
                            payload,
                        )
                        .emit();
                    }
                    Err(s) => {
                        let mut payload = Payload::new();
                        payload.insert(
                            "event".into(),
                            "Explorer could not craft resource".to_string(),
                        );
                        payload.insert("explorer_id".into(), explorer_id.to_string());
                        payload.insert("resource".into(), s.to_string());

                        LogEvent::system(
                            EventType::InternalOrchestratorAction,
                            Channel::Debug,
                            payload,
                        )
                        .emit();
                    }
                }
                None
            }

            ExplorerToOrchestrator::NeighborsRequest {
                explorer_id,
                current_planet_id,
            } => {
                let galaxy_guard = self.galaxy.lock().expect("Failed to lock galaxy mutex");
                let neighbors = galaxy_guard
                    .get(&current_planet_id)
                    .expect("Selected Planet not in galaxy")
                    .lock()
                    .unwrap()
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
                let mut payload = Payload::new();
                payload.insert("event".into(), "Explorer's bag content".into());
                payload.insert("explorer_id".to_string(), explorer_id.to_string());
                payload.insert("bag_content".to_string(), format!("{bag_content:?}"));

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Debug,
                    payload,
                )
                .emit();
                None
            }

            ExplorerToOrchestrator::SupportedCombinationResult {
                explorer_id,
                combination_list,
            } => {
                let mut payload = Payload::new();
                payload.insert("event".into(), "Explorer's supported combinations".into());
                payload.insert("explorer_id".into(), explorer_id.to_string());
                payload.insert("combination_list".into(), format!("{combination_list:?}"));

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Debug,
                    payload,
                )
                .emit();
                None
            }

            ExplorerToOrchestrator::SupportedResourceResult {
                explorer_id,
                supported_resources,
            } => {
                let mut payload = Payload::new();
                payload.insert("event".into(), "Explorer's supported resources'".into());
                payload.insert("explorer_id".into(), explorer_id.to_string());
                payload.insert(
                    "supported_resources".into(),
                    format!("{supported_resources:?}"),
                );

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Debug,
                    payload,
                )
                .emit();
                None
            }

            ExplorerToOrchestrator::CurrentPlanetResult {
                explorer_id,
                planet_id,
            } => {
                let mut payload = Payload::new();
                payload.insert("event".into(), "Explorer position".to_string());
                payload.insert("explorer_id".into(), explorer_id.to_string());
                payload.insert("current_planet_id".into(), planet_id.to_string());

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Debug,
                    payload,
                )
                .emit();
                None
            }

            ExplorerToOrchestrator::StartExplorerAIResult { explorer_id } => {
                let mut payload = Payload::new();
                payload.insert(
                    "event".into(),
                    "Explorer has been successfully started".into(),
                );
                payload.insert("explorer_id".into(), explorer_id.to_string());

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Info,
                    payload,
                )
                .emit();
                None
            }

            ExplorerToOrchestrator::StopExplorerAIResult { explorer_id } => {
                let mut payload = Payload::new();
                payload.insert(
                    "event".into(),
                    "Explorer has been successfully stopped".into(),
                );
                payload.insert("explorer_id".into(), explorer_id.to_string());

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Info,
                    payload,
                )
                .emit();
                None
            }

            ExplorerToOrchestrator::ResetExplorerAIResult { explorer_id } => {
                let mut payload = Payload::new();
                payload.insert(
                    "event".into(),
                    "Explorer has been successfully reset".into(),
                );
                payload.insert("explorer_id".into(), explorer_id.to_string());

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Debug,
                    payload,
                )
                .emit();
                None
            }

            ExplorerToOrchestrator::KillExplorerResult { explorer_id } => {
                let mut payload = Payload::new();
                payload.insert(
                    "event".into(),
                    "Explorer has been successfully killed".into(),
                );
                payload.insert("explorer_id".into(), explorer_id.to_string());

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Info,
                    payload,
                )
                .emit();
                None
            }

            ExplorerToOrchestrator::TravelToPlanetRequest {
                explorer_id,
                current_planet_id,
                dst_planet_id,
            } => {
                let mut payload = Payload::new();
                payload.insert("event".into(), "Explorer wants to move".into());
                payload.insert("explorer_id".into(), explorer_id.to_string());
                payload.insert("current_planet_id".into(), current_planet_id.to_string());
                payload.insert("destination_planet_id".into(), dst_planet_id.to_string());

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Debug,
                    payload,
                )
                .emit();
                None
            }

            //TODO: MAYBE WE WANT TO SEND A ORCH_TO_PLANET_OUTGOING OUT OF THIS?
            ExplorerToOrchestrator::MovedToPlanetResult { explorer_id } => {
                let mut payload = Payload::new();
                payload.insert("event".to_string(), "Explorer moved to Planet".into());
                payload.insert("explorer_id".into(), explorer_id.to_string());

                LogEvent::system(
                    EventType::InternalOrchestratorAction,
                    Channel::Info,
                    payload,
                )
                .emit();
                None
            }
        }
    }

    fn add_explorer(&mut self, explorer_id: ID, planet_id: ID) {
        //to add a new explorer for the first time inside the game
        let (tx_expl_out, rx_expl_out) = unbounded::<PlanetToExplorer>();
        self.planet_explorer_channels
            .add_plan_to_expl_sender(planet_id, tx_expl_out.clone());

        let mut explorer: dummy_explorer::Explorer<T> = dummy_explorer::Explorer::new();
        //TODO: set explorer - orchestrator channels
        explorer.set_planet_channels(
            rx_expl_out,
            self.planet_explorer_channels
                .get_expl_to_plan_sender(&planet_id)
                .expect("Failed to get explorer to planet sender")
                .clone(),
        );

        let msg = OrchestratorToPlanet::IncomingExplorerRequest {
            explorer_id,
            new_sender: tx_expl_out.clone(),
        };
        self.to_planet(planet_id, msg).expect(
            format!(
                "Failed to send IncomingExplorerRequest to planet {}",
                planet_id
            )
            .as_str(),
        );

        // Emit log event
        let mut payload = Payload::new();
        payload.insert("event".to_string(), "Explorer creation".to_string());
        payload.insert("explorer_id".to_string(), explorer_id.to_string());
        payload.insert("into_planet_id".to_string(), planet_id.to_string());

        LogEvent::system(
            EventType::InternalOrchestratorAction,
            Channel::Info,
            payload,
        )
        .emit();
    }
}
