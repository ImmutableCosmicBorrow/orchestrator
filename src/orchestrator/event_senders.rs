//! Background event senders for asteroids and sunrays.
//!
//! - Singleton background scheduler thread
//! - External control only via thread-safe flags
//! - Graceful shutdown support

use crate::logging_utils::log_internal;
use crate::orchestrator::conversations::{self, SendersToExplorer, SendersToPlanet};
use crate::orchestrator::queue::ConvoScheduler;
use crate::orchestrator::{ExplorerBagContent, ExplorersLocationRef};
use crate::payload;
use crate::planet::PlanetMap;

use common_game::components::forge::Forge;
use common_game::logging::Channel;
use common_game::utils::ID;

use rand::Rng;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

//
// ──────────────────────────────────────────────────────────────────────────
// External control surface (flags only)
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
// Graceful shutdown
// ──────────────────────────────────────────────────────────────────────────
//

static SCHEDULER_STOP: AtomicBool = AtomicBool::new(false);

pub fn shutdown_background_events() {
    SCHEDULER_STOP.store(true, Ordering::Release);
    ASTEROID_ENABLED.store(false, Ordering::Release);
    SUNRAY_ENABLED.store(false, Ordering::Release);

    let ctrl = scheduler_ctrl();
    let handle = ctrl.handle.lock().unwrap().take();
    if let Some(h) = handle {
        let _ = h.join();
    }
}

pub struct BackgroundEventsGuard {
    _private: (),
}

impl Drop for BackgroundEventsGuard {
    fn drop(&mut self) {
        shutdown_background_events();
    }
}

pub fn background_events_guard() -> BackgroundEventsGuard {
    BackgroundEventsGuard { _private: () }
}

//
// ──────────────────────────────────────────────────────────────────────────
// Scheduler singleton
// ──────────────────────────────────────────────────────────────────────────
//

struct SchedulerController {
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl SchedulerController {
    const fn new() -> Self {
        Self {
            handle: Mutex::new(None),
        }
    }
}

static SCHEDULER_CTRL: OnceLock<SchedulerController> = OnceLock::new();

fn scheduler_ctrl() -> &'static SchedulerController {
    SCHEDULER_CTRL.get_or_init(SchedulerController::new)
}

fn sleep_with_stop_check(total: Duration) -> bool {
    let step = Duration::from_millis(200);
    let mut remaining = total;

    while remaining > Duration::ZERO {
        if SCHEDULER_STOP.load(Ordering::Acquire) {
            return true;
        }
        let s = if remaining > step { step } else { remaining };
        thread::sleep(s);
        remaining = remaining.saturating_sub(s);
    }
    false
}

//
// ──────────────────────────────────────────────────────────────────────────
// Delay logic structures
// ──────────────────────────────────────────────────────────────────────────
//

mod regimes {
    use crate::globals::get_game_step;
    use std::cmp::max;
    use std::time::Duration;

    struct RegimeValues {
        min_delay: f32,
        max_delay: f32,
    }

    // These values are multipliers, and they get applied to the game step.
    // For example, with a game_step of 1s, Calm regime goes from 5s minimum  to 10s maximum delay.
    static ASTEROID_REGIMES: [RegimeValues; 3] = [
        // Calm
        RegimeValues {
            min_delay: 5.0,
            max_delay: 15.0,
        },
        // Active
        RegimeValues {
            min_delay: 2.0,
            max_delay: 10.0,
        },
        // Frenzy
        RegimeValues {
            min_delay: 1.0,
            max_delay: 5.0,
        },
    ];

    // These values are multipliers, and they get applied to the game step.
    // For example, with a game_step of 1s, Calm regime goes from 5s minimum  to 10s maximum delay.
    static SUNRAY_REGIMES: [RegimeValues; 3] = [
        // Calm
        RegimeValues {
            min_delay: 1.0,
            max_delay: 5.0,
        },
        // Active
        RegimeValues {
            min_delay: 0.5,
            max_delay: 2.0,
        },
        // Frenzy
        RegimeValues {
            min_delay: 0.1,
            max_delay: 1.0,
        },
    ];

    pub struct CurrentRegime {
        asteroid_level: usize,
        sunray_level: usize,
    }

    impl CurrentRegime {
        pub fn new() -> Self {
            Self {
                asteroid_level: 0,
                sunray_level: 0,
            }
        }

        pub fn asteroid_delay(&self) -> (Duration, Duration) {
            let regime = &ASTEROID_REGIMES[self.asteroid_level];
            // Ensure a reasonable step even when the game_step is zero, or very small
            let step = max(Duration::from_millis(500), get_game_step());
            (
                step.mul_f32(regime.min_delay),
                step.mul_f32(regime.max_delay),
            )
        }

        pub fn sunray_delay(&self) -> (Duration, Duration) {
            let regime = &SUNRAY_REGIMES[self.sunray_level];
            // Ensure a reasonable step even when the game_step is zero, or very small
            let step = max(Duration::from_millis(500), get_game_step());
            (
                step.mul_f32(regime.min_delay),
                step.mul_f32(regime.max_delay),
            )
        }

        pub fn increment_asteroid_level(&mut self) {
            if self.asteroid_level + 1 < ASTEROID_REGIMES.len() {
                self.asteroid_level += 1;
            }
        }

        pub fn increment_sunray_level(&mut self) {
            if self.sunray_level + 1 < SUNRAY_REGIMES.len() {
                self.sunray_level += 1;
            }
        }

        pub fn decrement_asteroid_level(&mut self) {
            if self.asteroid_level > 0 {
                self.asteroid_level -= 1;
            }
        }

        pub fn decrement_sunray_level(&mut self) {
            if self.sunray_level > 0 {
                self.sunray_level -= 1;
            }
        }
    }
}

struct AsteroidState {
    planning: bool,
    next_planet: ID,
    regime: regimes::CurrentRegime,
}

struct SunrayState {
    planning: bool,
    next_planet: ID,
    regime: regimes::CurrentRegime,
}

impl AsteroidState {
    fn new() -> Self {
        Self {
            planning: true,
            next_planet: 0,
            regime: regimes::CurrentRegime::new(),
        }
    }
}

impl SunrayState {
    fn new() -> Self {
        Self {
            planning: true,
            next_planet: 0,
            regime: regimes::CurrentRegime::new(),
        }
    }
}

thread_local! {
    static ASTEROID_STATE: RefCell<AsteroidState> = RefCell::new(AsteroidState::new());
    static SUNRAY_STATE: RefCell<SunrayState> = RefCell::new(SunrayState::new());
}

// Flipable tuning constants for weighting (baseline + per-explorer weight).
const DEFAULT_PLANET_WEIGHT: u64 = 1;
const EXPLORER_WEIGHT: u64 = 5;

//
// ──────────────────────────────────────────────────────────────────────────
// Delay logic
// ──────────────────────────────────────────────────────────────────────────
//

//TODO: change regime over time

fn asteroid_delay(
    planet_ids: &[ID],
    explorers_location: &ExplorersLocationRef,
) -> Option<(Duration, ID)> {
    // If there are no planets, return None immediately.
    if planet_ids.is_empty() {
        return None;
    }

    // Consult the thread-local ASTEROID_STATE.
    let mut handled: Option<Option<(Duration, ID)>> = None;
    ASTEROID_STATE.with(|state_cell| {
        let mut state = state_cell.borrow_mut();

        if state.planning {
            // Flip the planning flag off for next time.
            state.planning = false;

            // Initialize counts only for known planet_ids and count explorers located on them (DEFAULT_PLANET_WEIGHT for default baseline chance).
            let mut counts: HashMap<ID, u64> = planet_ids
                .iter()
                .map(|&id| (id, DEFAULT_PLANET_WEIGHT))
                .collect();
            {
                let explorers_loc = explorers_location.lock().unwrap();
                for &pid in explorers_loc.values() {
                    if let Some(c) = counts.get_mut(&pid) {
                        // Each explorer increases the chance of that planet being selected (by EXPLORER_WEIGHT).
                        *c = c.saturating_add(EXPLORER_WEIGHT);
                    }
                }
            }

            let mut rng = rand::rng();
            let total: u64 = counts.values().copied().sum();

            let mut pick = rng.random_range(0..total);
            for &pid in planet_ids {
                let cnt = *counts.get(&pid).unwrap_or(&0);
                if pick < cnt {
                    state.next_planet = pid;
                    let delay = state.regime.asteroid_delay();
                    // counts encoding: baseline DEFAULT_PLANET_WEIGHT, +EXPLORER_WEIGHT per explorer. Derive explorers count.
                    let explorers_on_pid = if cnt <= DEFAULT_PLANET_WEIGHT {
                        0
                    } else {
                        (cnt - DEFAULT_PLANET_WEIGHT) / EXPLORER_WEIGHT
                    };
                    // cap reduction so we don't eliminate delay entirely
                    let reduction_secs = explorers_on_pid.min(5);
                    let base_secs = rng.random_range(delay.0.as_secs()..=delay.1.as_secs());
                    let delay_secs = base_secs.saturating_sub(reduction_secs);
                    handled = Some(Some((Duration::from_secs(delay_secs), pid)));
                    return;
                }
                pick = pick.saturating_sub(cnt);
            }
            // No planet selected in loop -> handled stays None
        } else {
            // Flip the planning flag on for next time.
            state.planning = true;
            // Not planning: return the preplanned planet with zero delay.
            handled = Some(Some((Duration::from_secs(0), state.next_planet)));
        }
    });

    // Return whatever the thread-local handler decided (or None).
    handled.flatten()
}

fn sunray_delay(
    planet_ids: &[ID],
    explorers_location: &ExplorersLocationRef,
) -> Option<(Duration, ID)> {
    // If there are no planets, return None immediately.
    if planet_ids.is_empty() {
        return None;
    }

    // Consult the thread-local SUNRAY_STATE.
    let mut handled: Option<Option<(Duration, ID)>> = None;
    SUNRAY_STATE.with(|state_cell| {
        let mut state = state_cell.borrow_mut();

        if state.planning {
            // Flip the planning flag off for next time.
            state.planning = false;

            // Initialize counts only for known planet_ids and count explorers located on them (DEFAULT_PLANET_WEIGHT for default baseline chance).
            let mut counts: HashMap<ID, u64> = planet_ids
                .iter()
                .map(|&id| (id, DEFAULT_PLANET_WEIGHT))
                .collect();
            {
                let explorers_loc = explorers_location.lock().unwrap();
                for &pid in explorers_loc.values() {
                    if let Some(c) = counts.get_mut(&pid) {
                        // Each explorer increases the chance of that planet being selected (by EXPLORER_WEIGHT).
                        *c = c.saturating_add(EXPLORER_WEIGHT);
                    }
                }
            }

            let mut rng = rand::rng();
            let total: u64 = counts.values().copied().sum();

            let mut pick = rng.random_range(0..total);
            for &pid in planet_ids {
                let cnt = *counts.get(&pid).unwrap_or(&0);
                if pick < cnt {
                    state.next_planet = pid;
                    let delay = state.regime.sunray_delay();
                    // counts encoding: baseline DEFAULT_PLANET_WEIGHT, +EXPLORER_WEIGHT per explorer. Derive explorers count.
                    let explorers_on_pid = if cnt <= DEFAULT_PLANET_WEIGHT {
                        0
                    } else {
                        (cnt - DEFAULT_PLANET_WEIGHT) / EXPLORER_WEIGHT
                    };
                    // cap reduction so we don't eliminate delay entirely
                    let reduction_secs = explorers_on_pid.min(5);
                    let base_secs = rng.random_range(delay.0.as_secs()..=delay.1.as_secs());
                    let delay_secs = base_secs.saturating_sub(reduction_secs);
                    handled = Some(Some((Duration::from_secs(delay_secs), pid)));
                    return;
                }
                pick = pick.saturating_sub(cnt);
            }
            // No planet selected in loop -> handled stays None
        } else {
            // Flip the planning flag on for next time.
            state.planning = true;
            // Not planning: return the preplanned planet with zero delay.
            handled = Some(Some((Duration::from_secs(0), state.next_planet)));
        }
    });

    handled.flatten()
}

//
// ──────────────────────────────────────────────────────────────────────────
// Focused shared structs (instead of one giant context)
// ──────────────────────────────────────────────────────────────────────────
//

struct UniverseCtx {
    galaxy: PlanetMap,
    explorers_location: ExplorersLocationRef,
}

struct ConversationCtx {
    planets_senders: SendersToPlanet,
    forge: Arc<Forge>,
    explorer_senders: SendersToExplorer,
    convo_scheduler: ConvoScheduler<ExplorerBagContent>,
}

//
// ──────────────────────────────────────────────────────────────────────────
// Conversation helpers (context + raw values)
// ──────────────────────────────────────────────────────────────────────────
//

fn send_asteroid(universe: &UniverseCtx, convo: &ConversationCtx, planet_id: ID) {
    let to_planet = conversations::ToPlanetStruct::new(convo.planets_senders.clone(), planet_id);

    let state = conversations::orch_planet::SendingAsteroid::new(
        to_planet,
        convo.forge.clone(),
        universe.explorers_location.clone(),
        convo.explorer_senders.clone(),
    );

    let convo_id = crate::globals::get_id_manager().get_next_conversation_id();
    let conversation = conversations::orch_planet::AsteroidConversation::new(convo_id, state);

    convo
        .convo_scheduler
        .add_conversation(Box::new(conversation)
            as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

    // Log scheduling of asteroid conversation
    log_internal(
        Channel::Trace,
        payload!(
            event: "ScheduleConversation",
            conversation_id: convo_id,
            kind: "Asteroid",
            planet_id: planet_id
        ),
    );
}

fn send_sunray(convo: &ConversationCtx, planet_id: ID) {
    let to_planet = conversations::ToPlanetStruct::new(convo.planets_senders.clone(), planet_id);

    let state = conversations::orch_planet::SendSunray::new(to_planet, convo.forge.clone());

    let convo_id = crate::globals::get_id_manager().get_next_conversation_id();
    let conversation = conversations::orch_planet::SunrayConversation::new(convo_id, state);

    convo
        .convo_scheduler
        .add_conversation(Box::new(conversation)
            as Box<dyn conversations::Conversation<ExplorerBagContent> + Send + Sync>);

    // Log scheduling of sunray conversation
    log_internal(
        Channel::Trace,
        payload!(
            event: "ScheduleConversation",
            conversation_id: convo_id,
            kind: "Sunray",
            planet_id: planet_id
        ),
    );
}

//
// ──────────────────────────────────────────────────────────────────────────
// Scheduler helpers (Clippy-friendly decomposition)
// ──────────────────────────────────────────────────────────────────────────
//

struct SchedState {
    next_asteroid_at: Option<Instant>,
    next_sunray_at: Option<Instant>,
}

impl SchedState {
    fn new() -> Self {
        Self {
            next_asteroid_at: None,
            next_sunray_at: None,
        }
    }

    fn clear(&mut self) {
        self.next_asteroid_at = None;
        self.next_sunray_at = None;
    }
}

fn read_flags() -> (bool, bool) {
    (
        ASTEROID_ENABLED.load(Ordering::Acquire),
        SUNRAY_ENABLED.load(Ordering::Acquire),
    )
}

fn snapshot_planet_ids(universe: &UniverseCtx) -> Vec<ID> {
    let g = universe.galaxy.read().unwrap();
    g.keys().copied().collect()
}

fn idle_when_disabled(state: &mut SchedState) -> bool {
    state.clear();
    sleep_with_stop_check(Duration::from_millis(250))
}

fn schedule_if_needed(
    state: &mut SchedState,
    now: Instant,
    planet_ids: &[ID],
    universe: &UniverseCtx,
    asteroids_on: bool,
    sunrays_on: bool,
) {
    if asteroids_on && state.next_asteroid_at.is_none() {
        state.next_asteroid_at = asteroid_delay(planet_ids, &universe.explorers_location)
            .map(|(d, _)| now + d)
            .or_else(|| Some(now + Duration::from_secs(1)));
    } else if !asteroids_on {
        state.next_asteroid_at = None;
    }

    if sunrays_on && state.next_sunray_at.is_none() {
        state.next_sunray_at = sunray_delay(planet_ids, &universe.explorers_location)
            .map(|(d, _)| now + d)
            .or_else(|| Some(now + Duration::from_secs(1)));
    } else if !sunrays_on {
        state.next_sunray_at = None;
    }
}

fn next_deadline(state: &SchedState) -> Option<Instant> {
    match (state.next_asteroid_at, state.next_sunray_at) {
        (Some(a), Some(s)) => Some(a.min(s)),
        (Some(a), None) => Some(a),
        (None, Some(s)) => Some(s),
        (None, None) => None,
    }
}

fn wait_until(deadline: Instant, now: Instant) -> bool {
    deadline > now && sleep_with_stop_check(deadline - now)
}

fn maybe_send_asteroid(
    state: &mut SchedState,
    now: Instant,
    planet_ids: &[ID],
    universe: &UniverseCtx,
    convo: &ConversationCtx,
) {
    let Some(t) = state.next_asteroid_at else {
        return;
    };
    if now < t {
        return;
    }

    if let Some((_delay, planet)) = asteroid_delay(planet_ids, &universe.explorers_location) {
        log_internal(
            Channel::Info,
            payload!(action: "Sending asteroid", planet_id: planet),
        );
        send_asteroid(universe, convo, planet);
    } else {
        log_internal(
            Channel::Warning,
            payload!(action: "No planets available to send asteroid to"),
        );
    }

    state.next_asteroid_at = None;
}

fn maybe_send_sunray(
    state: &mut SchedState,
    now: Instant,
    planet_ids: &[ID],
    universe: &UniverseCtx,
    convo: &ConversationCtx,
) {
    let Some(t) = state.next_sunray_at else {
        return;
    };
    if now < t {
        return;
    }

    if let Some((_delay, planet)) = sunray_delay(planet_ids, &universe.explorers_location) {
        log_internal(
            Channel::Info,
            payload!(action: "Sending sunray", planet_id: planet),
        );
        send_sunray(convo, planet);
    } else {
        log_internal(
            Channel::Warning,
            payload!(action: "No planets available to send sunray to"),
        );
    }

    state.next_sunray_at = None;
}

fn scheduler_loop(universe: &UniverseCtx, convo: &ConversationCtx) {
    let mut state = SchedState::new();

    loop {
        if SCHEDULER_STOP.load(Ordering::Acquire) {
            break;
        }

        let (asteroids_on, sunrays_on) = read_flags();

        if !asteroids_on && !sunrays_on {
            if idle_when_disabled(&mut state) {
                break;
            }
            continue;
        }

        let planet_ids = snapshot_planet_ids(universe);
        let now = Instant::now();

        schedule_if_needed(
            &mut state,
            now,
            &planet_ids,
            universe,
            asteroids_on,
            sunrays_on,
        );

        let Some(deadline) = next_deadline(&state) else {
            if sleep_with_stop_check(Duration::from_millis(250)) {
                break;
            }
            continue;
        };

        if wait_until(deadline, now) || SCHEDULER_STOP.load(Ordering::Acquire) {
            break;
        }

        let (asteroids_on, sunrays_on) = read_flags();
        let now = Instant::now();

        if asteroids_on {
            maybe_send_asteroid(&mut state, now, &planet_ids, universe, convo);
        } else {
            state.next_asteroid_at = None;
        }

        if sunrays_on {
            maybe_send_sunray(&mut state, now, &planet_ids, universe, convo);
        } else {
            state.next_sunray_at = None;
        }
    }
}

//
// ──────────────────────────────────────────────────────────────────────────
// Public initialization
// ──────────────────────────────────────────────────────────────────────────
//

pub fn init_background_event_scheduler(
    planets_senders: SendersToPlanet,
    forge: Arc<Forge>,
    explorers_location: ExplorersLocationRef,
    explorer_senders: SendersToExplorer,
    convo_scheduler: ConvoScheduler<ExplorerBagContent>,
    galaxy: PlanetMap,
) {
    let ctrl = scheduler_ctrl();
    let mut handle_guard = ctrl.handle.lock().unwrap();

    if handle_guard.is_some() {
        return;
    }

    SCHEDULER_STOP.store(false, Ordering::Release);

    let universe = UniverseCtx {
        galaxy,
        explorers_location,
    };

    let convo = ConversationCtx {
        planets_senders,
        forge,
        explorer_senders,
        convo_scheduler,
    };

    let universe = Arc::new(universe);
    let convo = Arc::new(convo);

    let handle = thread::spawn({
        let universe = Arc::clone(&universe);
        let convo = Arc::clone(&convo);
        move || {
            scheduler_loop(&universe, &convo);
        }
    });

    *handle_guard = Some(handle);
}
