//! Timing helpers for scheduler sleeps and deadline selection.

use super::EventKind;
use super::control;
use super::state::SchedulerState;
use std::thread;
use std::time::{Duration, Instant};

const SLEEP_GRANULARITY: Duration = Duration::from_millis(200);
const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub(super) fn idle_wait() -> bool {
    sleep_with_stop_check(IDLE_POLL_INTERVAL)
}

pub(super) fn sleep_until(deadline: Instant) -> bool {
    let now = Instant::now();
    if deadline <= now {
        false
    } else {
        sleep_with_stop_check(deadline - now)
    }
}

pub(super) fn next_deadline(state: &SchedulerState) -> Option<Instant> {
    [EventKind::Asteroid, EventKind::Sunray]
        .into_iter()
        .filter_map(|kind| state.planner.plan(kind).deadline())
        .min()
}

fn sleep_with_stop_check(total: Duration) -> bool {
    let mut remaining = total;

    while remaining > Duration::ZERO {
        if control::stop_requested() {
            return true;
        }

        let sleep_for = remaining.min(SLEEP_GRANULARITY);
        thread::sleep(sleep_for);
        remaining = remaining.saturating_sub(sleep_for);
    }

    false
}
