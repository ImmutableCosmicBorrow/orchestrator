//! Scheduler singleton, thread lifecycle, and orchestration loop.

use super::EventKind;
use super::control;
use super::dispatch;
use super::planning;
use super::state::SchedulerState;
use super::timing;
use crate::convo_manager::ConvoManager;
use crate::orchestrator::OrchContext;
use common_game::utils::ID;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const REGIME_STEP_INTERVAL: Duration = Duration::from_secs(30);

struct SchedulerController {
    handle: Mutex<Option<JoinHandle<()>>>,
}

static SCHEDULER_CTRL: OnceLock<SchedulerController> = OnceLock::new();

pub(super) fn init_background_event_scheduler(convo_manager: Arc<ConvoManager>) {
    let controller = scheduler_ctrl();
    let mut handle_guard = controller.handle.lock().unwrap();

    if handle_guard.is_some() {
        return;
    }

    control::reset_shutdown_flag();

    let handle = thread::spawn({ move || scheduler_loop(convo_manager) });

    *handle_guard = Some(handle);
}

pub(super) fn join_scheduler_thread() {
    let controller = scheduler_ctrl();
    let handle = controller.handle.lock().unwrap().take();

    if let Some(handle) = handle {
        let _ = handle.join();
    }
}

fn scheduler_loop(convo_manager: Arc<ConvoManager>) {
    let mut state = SchedulerState::new();

    let orch_context = &*convo_manager.get_orch_context();

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

        let planet_ids = snapshot_planet_ids(orch_context);
        schedule_missing_events(&mut state, &planet_ids, orch_context, Instant::now());

        let Some(deadline) = timing::next_deadline(&state) else {
            if timing::idle_wait() {
                break;
            }
            continue;
        };

        if timing::sleep_until(deadline) || control::stop_requested() {
            break;
        }

        dispatch_due_events(&mut state, Instant::now(), &*convo_manager);
    }
}

fn schedule_missing_events(
    state: &mut SchedulerState,
    planet_ids: &[ID],
    orch_context: &OrchContext,
    scheduled_at: Instant,
) {
    maybe_schedule_event(
        EventKind::Asteroid,
        state,
        planet_ids,
        orch_context,
        scheduled_at,
    );
    maybe_schedule_event(
        EventKind::Sunray,
        state,
        planet_ids,
        orch_context,
        scheduled_at,
    );
}

fn maybe_schedule_event(
    kind: EventKind,
    state: &mut SchedulerState,
    planet_ids: &[ID],
    world: &OrchContext,
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

fn dispatch_due_events(state: &mut SchedulerState, now: Instant, convo_manager: &ConvoManager) {
    dispatch_due_event(EventKind::Asteroid, state, now, convo_manager);
    dispatch_due_event(EventKind::Sunray, state, now, convo_manager);
}

fn dispatch_due_event(
    kind: EventKind,
    state: &mut SchedulerState,
    now: Instant,
    convo_manager: &ConvoManager,
) {
    if !state.is_enabled(kind) {
        return;
    }

    if let Some(event) = state.planner.take_due(kind, now) {
        dispatch::dispatch(event, convo_manager);
    }
}

fn snapshot_planet_ids(world: &OrchContext) -> Vec<ID> {
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
