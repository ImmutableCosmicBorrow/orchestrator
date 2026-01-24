//! Background event senders for asteroids and sunrays.
//!
//! This module contains the logic for spawning background threads that
//! periodically send asteroids and sunrays to random planets in the galaxy.

use crate::logging_utils::log_internal;
use crate::orchestrator::conversations::{self, SendersToExplorer, SendersToPlanet};
use crate::orchestrator::queue::ConvoScheduler;
use crate::orchestrator::{ExplorerBag, ExplorersLocationRef};
use crate::payload;
use crate::planet::PlanetMap;
use common_game::components::forge::Forge;
use common_game::logging::Channel;
use common_game::utils::ID;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Returns the delay and target planet for the next asteroid event.
///
/// Returns `None` if there are no available planets.
#[allow(unused_variables)]
fn asteroid_delay(
    planet_ids: &[ID],
    explorers_location: &ExplorersLocationRef,
) -> Option<(Duration, ID)> {
    if planet_ids.is_empty() {
        return None;
    }
    //TODO: Implement proper logic
    planet_ids.first().map(|&id| (Duration::from_secs(10), id))
}

/// Returns the delay and target planet for the next sunray event.
///
/// Returns `None` if there are no available planets.
#[allow(unused_variables)]
fn sunray_delay(
    planet_ids: &[ID],
    explorers_location: &ExplorersLocationRef,
) -> Option<(Duration, ID)> {
    if planet_ids.is_empty() {
        return None;
    }
    //TODO: Implement proper logic
    planet_ids.first().map(|&id| (Duration::from_secs(10), id))
}

/// Sends an asteroid to the specified planet.
fn send_asteroid_to_planet(
    planet_id: ID,
    planets_senders: &SendersToPlanet,
    forge: &Arc<Forge>,
    explorers_location: &ExplorersLocationRef,
    explorer_senders: &SendersToExplorer,
    convo_scheduler: &ConvoScheduler<ExplorerBag>,
) {
    let to_planet_struct = conversations::ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = conversations::orch_planet::SendingAsteroid::new(
        to_planet_struct,
        forge.clone(),
        explorers_location.clone(),
        explorer_senders.clone(),
    );
    // TODO: Use proper conversation ID
    let conversation = conversations::orch_planet::AsteroidConversation::<
        conversations::orch_planet::SendingAsteroid,
    >::new(0, state);
    convo_scheduler
        .add_conversation(Box::new(conversation)
            as Box<dyn conversations::Conversation<ExplorerBag> + Send + Sync>);
}

/// Sends a sunray to the specified planet.
fn send_sunray_to_planet(
    planet_id: ID,
    planets_senders: &SendersToPlanet,
    forge: &Arc<Forge>,
    convo_scheduler: &ConvoScheduler<ExplorerBag>,
) {
    let to_planet_struct = conversations::ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = conversations::orch_planet::SendSunray::new(to_planet_struct, forge.clone());
    // TODO: Use proper conversation ID
    let conversation = conversations::orch_planet::SunrayConversation::<
        conversations::orch_planet::SendSunray,
    >::new(0, state);
    convo_scheduler
        .add_conversation(Box::new(conversation)
            as Box<dyn conversations::Conversation<ExplorerBag> + Send + Sync>);
}

/// Spawns a background thread that periodically sends asteroids to random planets.
pub fn start_asteroid_sender(
    planets_senders: SendersToPlanet,
    forge: Arc<Forge>,
    explorers_location: ExplorersLocationRef,
    explorer_senders: SendersToExplorer,
    convo_scheduler: ConvoScheduler<ExplorerBag>,
    galaxy: PlanetMap,
) {
    thread::spawn(move || -> ! {
        loop {
            // Get list of planet IDs
            let planet_ids: Vec<ID> = {
                let galaxy_lock = galaxy.read().unwrap();
                galaxy_lock.keys().copied().collect()
            };

            // Get delay and target planet from the delay function
            if let Some((delay, target_planet)) = asteroid_delay(&planet_ids, &explorers_location) {
                thread::sleep(delay);

                log_internal(
                    Channel::Info,
                    payload!(
                        action : "Sending asteroid to planet",
                        planet_id : target_planet
                    ),
                );

                send_asteroid_to_planet(
                    target_planet,
                    &planets_senders,
                    &forge,
                    &explorers_location,
                    &explorer_senders,
                    &convo_scheduler,
                );
            } else {
                log_internal(
                    Channel::Warning,
                    payload!(
                        action : "No planets available to send asteroid to"
                    ),
                );
                // Wait a bit before retrying
                thread::sleep(Duration::from_secs(1));
            }
        }
    });
}

/// Spawns a background thread that periodically sends sunrays to random planets.
pub fn start_sunray_sender(
    planets_senders: SendersToPlanet,
    forge: Arc<Forge>,
    explorers_location: ExplorersLocationRef,
    convo_scheduler: ConvoScheduler<ExplorerBag>,
    galaxy: PlanetMap,
) {
    thread::spawn(move || -> ! {
        loop {
            // Get list of planet IDs
            let planet_ids: Vec<ID> = {
                let galaxy_lock = galaxy.read().unwrap();
                galaxy_lock.keys().copied().collect()
            };

            // Get delay and target planet from the delay function
            if let Some((delay, target_planet)) = sunray_delay(&planet_ids, &explorers_location) {
                thread::sleep(delay);

                log_internal(
                    Channel::Info,
                    payload!(
                        action : "Sending sunray to planet",
                        planet_id : target_planet
                    ),
                );

                send_sunray_to_planet(target_planet, &planets_senders, &forge, &convo_scheduler);
            } else {
                log_internal(
                    Channel::Warning,
                    payload!(
                        action : "No planets available to send sunray to"
                    ),
                );
                // Wait a bit before retrying
                thread::sleep(Duration::from_secs(1));
            }
        }
    });
}
