//! Public thread-safe control surface for background events.

use super::scheduler;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static ASTEROID_ENABLED: AtomicBool = AtomicBool::new(false);
static SUNRAY_ENABLED: AtomicBool = AtomicBool::new(false);
static SCHEDULER_STOP: AtomicBool = AtomicBool::new(false);
static AUTO_REGIME_PROGRESS_ENABLED: AtomicBool = AtomicBool::new(true);
static ASTEROID_REGIME_INCREASE_REQUESTS: AtomicU64 = AtomicU64::new(0);
static ASTEROID_REGIME_DECREASE_REQUESTS: AtomicU64 = AtomicU64::new(0);
static SUNRAY_REGIME_INCREASE_REQUESTS: AtomicU64 = AtomicU64::new(0);
static SUNRAY_REGIME_DECREASE_REQUESTS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ControlFlags {
    pub(super) asteroids_enabled: bool,
    pub(super) sunrays_enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ManualRegimeRequests {
    pub(super) asteroid_increase: u64,
    pub(super) asteroid_decrease: u64,
    pub(super) sunray_increase: u64,
    pub(super) sunray_decrease: u64,
}

pub(super) struct BackgroundEventsGuard {
    _private: (),
}

pub(super) fn enable_asteroids() {
    ASTEROID_ENABLED.store(true, Ordering::Release);
}

pub(super) fn disable_asteroids() {
    ASTEROID_ENABLED.store(false, Ordering::Release);
}

pub(super) fn enable_sunrays() {
    SUNRAY_ENABLED.store(true, Ordering::Release);
}

pub(super) fn disable_sunrays() {
    SUNRAY_ENABLED.store(false, Ordering::Release);
}

pub(super) fn enable_auto_regime_progression() {
    AUTO_REGIME_PROGRESS_ENABLED.store(true, Ordering::Release);
}

pub(super) fn disable_auto_regime_progression() {
    AUTO_REGIME_PROGRESS_ENABLED.store(false, Ordering::Release);
}

pub(super) fn set_auto_regime_progression(enabled: bool) {
    AUTO_REGIME_PROGRESS_ENABLED.store(enabled, Ordering::Release);
}

pub(super) fn increase_asteroid_regime() {
    ASTEROID_REGIME_INCREASE_REQUESTS.fetch_add(1, Ordering::AcqRel);
}

pub(super) fn decrease_asteroid_regime() {
    ASTEROID_REGIME_DECREASE_REQUESTS.fetch_add(1, Ordering::AcqRel);
}

pub(super) fn increase_sunray_regime() {
    SUNRAY_REGIME_INCREASE_REQUESTS.fetch_add(1, Ordering::AcqRel);
}

pub(super) fn decrease_sunray_regime() {
    SUNRAY_REGIME_DECREASE_REQUESTS.fetch_add(1, Ordering::AcqRel);
}

pub(super) fn shutdown_background_events() {
    SCHEDULER_STOP.store(true, Ordering::Release);
    ASTEROID_ENABLED.store(false, Ordering::Release);
    SUNRAY_ENABLED.store(false, Ordering::Release);
    scheduler::join_scheduler_thread();
}

pub(super) fn background_events_guard() -> BackgroundEventsGuard {
    BackgroundEventsGuard { _private: () }
}

pub(super) fn read_flags() -> ControlFlags {
    ControlFlags {
        asteroids_enabled: ASTEROID_ENABLED.load(Ordering::Acquire),
        sunrays_enabled: SUNRAY_ENABLED.load(Ordering::Acquire),
    }
}

pub(super) fn stop_requested() -> bool {
    SCHEDULER_STOP.load(Ordering::Acquire)
}

pub(super) fn auto_regime_progression_enabled() -> bool {
    AUTO_REGIME_PROGRESS_ENABLED.load(Ordering::Acquire)
}

pub(super) fn consume_manual_regime_requests() -> ManualRegimeRequests {
    ManualRegimeRequests {
        asteroid_increase: ASTEROID_REGIME_INCREASE_REQUESTS.swap(0, Ordering::AcqRel),
        asteroid_decrease: ASTEROID_REGIME_DECREASE_REQUESTS.swap(0, Ordering::AcqRel),
        sunray_increase: SUNRAY_REGIME_INCREASE_REQUESTS.swap(0, Ordering::AcqRel),
        sunray_decrease: SUNRAY_REGIME_DECREASE_REQUESTS.swap(0, Ordering::AcqRel),
    }
}

pub(super) fn reset_shutdown_flag() {
    SCHEDULER_STOP.store(false, Ordering::Release);
}

impl ControlFlags {
    pub(super) fn any_enabled(self) -> bool {
        self.asteroids_enabled || self.sunrays_enabled
    }
}

impl Drop for BackgroundEventsGuard {
    fn drop(&mut self) {
        shutdown_background_events();
    }
}
