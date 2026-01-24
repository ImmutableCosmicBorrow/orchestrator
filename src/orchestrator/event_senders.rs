//! Background event senders for asteroids and sunrays.
//!
//! Sender threads are singletons controlled exclusively via thread-safe flags.
//! External code may only enable or disable senders; threads manage themselves.

use crate::logging_utils::log_internal;
use crate::orchestrator::conversations::{self, SendersToExplorer, SendersToPlanet};
use crate::orchestrator::queue::ConvoScheduler;
use crate::orchestrator::{ExplorerBag, ExplorersLocationRef};
use crate::payload;
use crate::planet::PlanetMap;

use common_game::components::forge::Forge;
use common_game::logging::Channel;
use common_game::utils::ID;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};
use std::thread;
use std::time::Duration;

//
// ──────────────────────────────────────────────────────────────────────────
// Flags (the ONLY external control surface)
// ──────────────────────────────────────────────────────────────────────────
//

static ASTEROID_ENABLED: AtomicBool = AtomicBool::new(false);
static SUNRAY_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enable_asteroids() {
    ASTEROID_ENABLED.store(true, Ordering::Release);
}

pub fn disable_asteroids() {
    ASTEROID_ENABLED.store(false, Ordering::Release);
}

pub fn enable_sunrays() {
    SUNRAY_ENABLED.store(true, Ordering::Release);
}

pub fn disable_sunrays() {
    SUNRAY_ENABLED.store(false, Ordering::Release);
}

//
// ──────────────────────────────────────────────────────────────────────────
// Singleton thread guards
// ──────────────────────────────────────────────────────────────────────────
//

static ASTEROID_THREAD: OnceLock<()> = OnceLock::new();
static SUNRAY_THREAD: OnceLock<()> = OnceLock::new();

//
// ──────────────────────────────────────────────────────────────────────────
// Placeholder delay logic (unchanged by request)
// ──────────────────────────────────────────────────────────────────────────
//

fn asteroid_delay(
    planet_ids: &[ID],
    _explorers_location: &ExplorersLocationRef,
) -> Option<(Duration, ID)> {
    planet_ids.first().copied().map(|id| (Duration::from_secs(10), id))
}

fn sunray_delay(
    planet_ids: &[ID],
    _explorers_location: &ExplorersLocationRef,
) -> Option<(Duration, ID)> {
    planet_ids.first().copied().map(|id| (Duration::from_secs(10), id))
}

//
// ──────────────────────────────────────────────────────────────────────────
// Conversation send helpers
// ──────────────────────────────────────────────────────────────────────────
//

fn send_asteroid_to_planet(
    planet_id: ID,
    planets_senders: &SendersToPlanet,
    forge: &Arc<Forge>,
    explorers_location: &ExplorersLocationRef,
    explorer_senders: &SendersToExplorer,
    convo_scheduler: &ConvoScheduler<ExplorerBag>,
) {
    let to_planet = conversations::ToPlanetStruct::new(planets_senders.clone(), planet_id);

    let state = conversations::orch_planet::SendingAsteroid::new(
        to_planet,
        forge.clone(),
        explorers_location.clone(),
        explorer_senders.clone(),
    );

    let convo = conversations::orch_planet::AsteroidConversation::new(0, state);

    convo_scheduler.add_conversation(
        Box::new(convo)
            as Box<dyn conversations::Conversation<ExplorerBag> + Send + Sync>,
    );
}

fn send_sunray_to_planet(
    planet_id: ID,
    planets_senders: &SendersToPlanet,
    forge: &Arc<Forge>,
    convo_scheduler: &ConvoScheduler<ExplorerBag>,
) {
    let to_planet = conversations::ToPlanetStruct::new(planets_senders.clone(), planet_id);
    let state = conversations::orch_planet::SendSunray::new(to_planet, forge.clone());

    let convo = conversations::orch_planet::SunrayConversation::new(0, state);

    convo_scheduler.add_conversation(
        Box::new(convo)
            as Box<dyn conversations::Conversation<ExplorerBag> + Send + Sync>,
    );
}

//
// ──────────────────────────────────────────────────────────────────────────
// Thread bootstrap (idempotent, internal only)
// ──────────────────────────────────────────────────────────────────────────
//

pub fn init_asteroid_sender(
    planets_senders: SendersToPlanet,
    forge: Arc<Forge>,
    explorers_location: ExplorersLocationRef,
    explorer_senders: SendersToExplorer,
    convo_scheduler: ConvoScheduler<ExplorerBag>,
    galaxy: PlanetMap,
) {
    ASTEROID_THREAD.get_or_init(|| {
        thread::spawn(move || loop {
            if !ASTEROID_ENABLED.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(250));
                continue;
            }

            let planet_ids: Vec<ID> = {
                let g = galaxy.read().unwrap();
                g.keys().copied().collect()
            };

            if let Some((delay, planet)) =
                asteroid_delay(&planet_ids, &explorers_location)
            {
                thread::sleep(delay);

                if !ASTEROID_ENABLED.load(Ordering::Acquire) {
                    continue;
                }

                log_internal(
                    Channel::Info,
                    payload!(action: "Sending asteroid", planet_id: planet),
                );

                send_asteroid_to_planet(
                    planet,
                    &planets_senders,
                    &forge,
                    &explorers_location,
                    &explorer_senders,
                    &convo_scheduler,
                );
            } else {
                thread::sleep(Duration::from_secs(1));
            }
        });
    });
}

pub fn init_sunray_sender(
    planets_senders: SendersToPlanet,
    forge: Arc<Forge>,
    explorers_location: ExplorersLocationRef,
    convo_scheduler: ConvoScheduler<ExplorerBag>,
    galaxy: PlanetMap,
) {
    SUNRAY_THREAD.get_or_init(|| {
        thread::spawn(move || loop {
            if !SUNRAY_ENABLED.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(250));
                continue;
            }

            let planet_ids: Vec<ID> = {
                let g = galaxy.read().unwrap();
                g.keys().copied().collect()
            };

            if let Some((delay, planet)) =
                sunray_delay(&planet_ids, &explorers_location)
            {
                thread::sleep(delay);

                if !SUNRAY_ENABLED.load(Ordering::Acquire) {
                    continue;
                }

                log_internal(
                    Channel::Info,
                    payload!(action: "Sending sunray", planet_id: planet),
                );

                send_sunray_to_planet(
                    planet,
                    &planets_senders,
                    &forge,
                    &convo_scheduler,
                );
            } else {
                thread::sleep(Duration::from_secs(1));
            }
        });
    });
}
