//! Background event senders for asteroids and sunrays.
//!
//! - Singleton background scheduler thread
//! - External control ONLY via thread-safe flags
//! - Graceful shutdown support
//! - Clippy-friendly (no mega-functions, reduced argument lists)

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
// Delay logic (placeholders)
// ──────────────────────────────────────────────────────────────────────────
//

fn asteroid_delay(
    planet_ids: &[ID],
    _explorers_location: &ExplorersLocationRef,
) -> Option<(Duration, ID)> {
    planet_ids
        .first()
        .copied()
        .map(|id| (Duration::from_secs(10), id))
}

fn sunray_delay(
    planet_ids: &[ID],
    _explorers_location: &ExplorersLocationRef,
) -> Option<(Duration, ID)> {
    planet_ids
        .first()
        .copied()
        .map(|id| (Duration::from_secs(10), id))
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
    convo_scheduler: ConvoScheduler<ExplorerBag>,
}

struct AsteroidCtx<'a> {
    universe: &'a UniverseCtx,
    convo: &'a ConversationCtx,
}

struct SunrayCtx<'a> {
    universe: &'a UniverseCtx,
    convo: &'a ConversationCtx,
}

//
// ──────────────────────────────────────────────────────────────────────────
// Conversation helpers (ctx + raw values)
// ──────────────────────────────────────────────────────────────────────────
//

fn send_asteroid(ctx: &AsteroidCtx<'_>, planet_id: ID) {
    let to_planet =
        conversations::ToPlanetStruct::new(ctx.convo.planets_senders.clone(), planet_id);

    let state = conversations::orch_planet::SendingAsteroid::new(
        to_planet,
        ctx.convo.forge.clone(),
        ctx.universe.explorers_location.clone(),
        ctx.convo.explorer_senders.clone(),
    );

    // TODO: Use proper conversation ID
    let convo = conversations::orch_planet::AsteroidConversation::new(0, state);

    ctx.convo.convo_scheduler.add_conversation(
        Box::new(convo) as Box<dyn conversations::Conversation<ExplorerBag> + Send + Sync>
    );
}

fn send_sunray(ctx: &SunrayCtx<'_>, planet_id: ID) {
    let to_planet =
        conversations::ToPlanetStruct::new(ctx.convo.planets_senders.clone(), planet_id);

    let state = conversations::orch_planet::SendSunray::new(to_planet, ctx.convo.forge.clone());

    // TODO: Use proper conversation ID
    let convo = conversations::orch_planet::SunrayConversation::new(0, state);

    ctx.convo.convo_scheduler.add_conversation(
        Box::new(convo) as Box<dyn conversations::Conversation<ExplorerBag> + Send + Sync>
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
    ctx: &AsteroidCtx<'_>,
) {
    let Some(t) = state.next_asteroid_at else {
        return;
    };
    if now < t {
        return;
    }

    if let Some((_delay, planet)) = asteroid_delay(planet_ids, &ctx.universe.explorers_location) {
        log_internal(
            Channel::Info,
            payload!(action: "Sending asteroid", planet_id: planet),
        );
        send_asteroid(ctx, planet);
    } else {
        log_internal(
            Channel::Warning,
            payload!(action: "No planets available to send asteroid to"),
        );
    }

    state.next_asteroid_at = None;
}

fn maybe_send_sunray(state: &mut SchedState, now: Instant, planet_ids: &[ID], ctx: &SunrayCtx<'_>) {
    let Some(t) = state.next_sunray_at else {
        return;
    };
    if now < t {
        return;
    }

    if let Some((_delay, planet)) = sunray_delay(planet_ids, &ctx.universe.explorers_location) {
        log_internal(
            Channel::Info,
            payload!(action: "Sending sunray", planet_id: planet),
        );
        send_sunray(ctx, planet);
    } else {
        log_internal(
            Channel::Warning,
            payload!(action: "No planets available to send sunray to"),
        );
    }

    state.next_sunray_at = None;
}

fn scheduler_loop(universe: &UniverseCtx, convo: &ConversationCtx) {
    let asteroid_ctx = AsteroidCtx { universe, convo };
    let sunray_ctx = SunrayCtx { universe, convo };

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
            maybe_send_asteroid(&mut state, now, &planet_ids, &asteroid_ctx);
        } else {
            state.next_asteroid_at = None;
        }

        if sunrays_on {
            maybe_send_sunray(&mut state, now, &planet_ids, &sunray_ctx);
        } else {
            state.next_sunray_at = None;
        }
    }
}

//
// ──────────────────────────────────────────────────────────────────────────
// Public init
// ──────────────────────────────────────────────────────────────────────────
//

pub fn init_background_event_scheduler(
    planets_senders: SendersToPlanet,
    forge: Arc<Forge>,
    explorers_location: ExplorersLocationRef,
    explorer_senders: SendersToExplorer,
    convo_scheduler: ConvoScheduler<ExplorerBag>,
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
