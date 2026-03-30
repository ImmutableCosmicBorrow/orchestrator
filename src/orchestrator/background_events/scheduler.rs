//! Scheduler singleton, thread lifecycle, and orchestration loop.

use super::EventKind;
use super::context::{DispatchCtx, WorldCtx};
use super::control;
use super::dispatch;
use super::planning;
use super::state::SchedulerState;
use super::timing;
use crate::channels_manager::ChannelsManager;
use crate::convo_manager::queue::ConvoScheduler;
use crate::orchestrator::{ChannelsManagerRef, ExplorerBagContent, ExplorersLocationRef};
use crate::planet::PlanetMap;
use common_game::components::forge::Forge;
use common_game::utils::ID;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use crate::convo_manager::convo_factory::ConvoFactory;

const REGIME_STEP_INTERVAL: Duration = Duration::from_secs(30);

struct SchedulerController {
    handle: Mutex<Option<JoinHandle<()>>>,
}

static SCHEDULER_CTRL: OnceLock<SchedulerController> = OnceLock::new();

pub(super) fn init_background_event_scheduler(
    channels_manager: ChannelsManagerRef,
    forge: Arc<Forge>,
    explorers_location: ExplorersLocationRef,
    convo_factory: Arc<ConvoFactory>,
    galaxy: PlanetMap,
) {
    let controller = scheduler_ctrl();
    let mut handle_guard = controller.handle.lock().unwrap();

    if handle_guard.is_some() {
        return;
    }

    control::reset_shutdown_flag();

    let world = Arc::new(WorldCtx::new(galaxy, explorers_location));
    let dispatch_ctx = Arc::new(DispatchCtx::new(channels_manager, forge, convo_factory));

    let handle = thread::spawn({
        let world = Arc::clone(&world);
        let dispatch_ctx = Arc::clone(&dispatch_ctx);
        move || scheduler_loop(&world, &dispatch_ctx)
    });

    *handle_guard = Some(handle);
}

pub(super) fn join_scheduler_thread() {
    let controller = scheduler_ctrl();
    let handle = controller.handle.lock().unwrap().take();

    if let Some(handle) = handle {
        let _ = handle.join();
    }
}

fn scheduler_loop(world: &WorldCtx, dispatch_ctx: &DispatchCtx) {
    let mut state = SchedulerState::new();

    loop {
        if control::stop_requested() {
            break;
        }

        let flags = control::read_flags();
        state.sync_enabled_flags(flags.asteroids_enabled, flags.sunrays_enabled);
        state.apply_manual_regime_requests(control::consume_manual_regime_requests());

        if !flags.any_enabled() {
            if timing::idle_wait() {
                break;
            }
            continue;
        }

        if control::auto_regime_progression_enabled() {
            state.maybe_advance_regimes(Instant::now(), REGIME_STEP_INTERVAL);
        }

        let planet_ids = snapshot_planet_ids(world);
        schedule_missing_events(&mut state, &planet_ids, world, Instant::now());

        let Some(deadline) = timing::next_deadline(&state) else {
            if timing::idle_wait() {
                break;
            }
            continue;
        };

        if timing::sleep_until(deadline) || control::stop_requested() {
            break;
        }

        dispatch_due_events(&mut state, Instant::now(), world, dispatch_ctx);
    }
}

fn schedule_missing_events(
    state: &mut SchedulerState,
    planet_ids: &[ID],
    world: &WorldCtx,
    scheduled_at: Instant,
) {
    maybe_schedule_event(EventKind::Asteroid, state, planet_ids, world, scheduled_at);
    maybe_schedule_event(EventKind::Sunray, state, planet_ids, world, scheduled_at);
}

fn maybe_schedule_event(
    kind: EventKind,
    state: &mut SchedulerState,
    planet_ids: &[ID],
    world: &WorldCtx,
    scheduled_at: Instant,
) {
    if !state.is_enabled(kind) {
        return;
    }

    let plan_state = state.planner.plan_mut(kind);
    if plan_state.pending.is_some() {
        return;
    }

    let _ = planning::schedule_next_event(
        kind,
        planet_ids,
        &world.explorers_location,
        plan_state,
        scheduled_at,
    );
}

fn dispatch_due_events(
    state: &mut SchedulerState,
    now: Instant,
    world: &WorldCtx,
    dispatch_ctx: &DispatchCtx,
) {
    dispatch_due_event(EventKind::Asteroid, state, now, world, dispatch_ctx);
    dispatch_due_event(EventKind::Sunray, state, now, world, dispatch_ctx);
}

fn dispatch_due_event(
    kind: EventKind,
    state: &mut SchedulerState,
    now: Instant,
    world: &WorldCtx,
    dispatch_ctx: &DispatchCtx,
) {
    if !state.is_enabled(kind) {
        return;
    }

    if let Some(event) = state.planner.take_due(kind, now) {
        dispatch::dispatch(event, world, dispatch_ctx);
    }
}

fn snapshot_planet_ids(world: &WorldCtx) -> Vec<ID> {
    let galaxy = world.galaxy.read().unwrap();
    galaxy.keys().copied().collect()
}

impl SchedulerController {
    const fn new() -> Self {
        Self {
            handle: Mutex::new(None),
        }
    }
}

fn scheduler_ctrl() -> &'static SchedulerController {
    SCHEDULER_CTRL.get_or_init(SchedulerController::new)
}
