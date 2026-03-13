//! Runtime scheduler and planning state for background events.

use super::EventKind;
use super::control::ManualRegimeRequests;
use super::regimes::RegimeState;
use common_game::utils::ID;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PlannedEvent {
    pub(super) kind: EventKind,
    pub(super) planet_id: ID,
    pub(super) delay: Duration,
    pub(super) scheduled_at: Instant,
}

#[derive(Debug)]
pub(super) struct EventPlanState {
    pub(super) pending: Option<PlannedEvent>,
    pub(super) regime: RegimeState,
}

#[derive(Debug)]
pub(super) struct PlannerState {
    pub(super) asteroid: EventPlanState,
    pub(super) sunray: EventPlanState,
}

#[derive(Debug)]
pub(super) struct SchedulerState {
    pub(super) planner: PlannerState,
    asteroids_enabled: bool,
    sunrays_enabled: bool,
    last_regime_step_at: Instant,
}

impl PlannedEvent {
    pub(super) fn new(
        kind: EventKind,
        planet_id: ID,
        delay: Duration,
        scheduled_at: Instant,
    ) -> Self {
        Self {
            kind,
            planet_id,
            delay,
            scheduled_at,
        }
    }

    pub(super) fn deadline(self) -> Instant {
        self.scheduled_at + self.delay
    }
}

impl EventPlanState {
    pub(super) fn new() -> Self {
        Self {
            pending: None,
            regime: RegimeState::new(),
        }
    }

    pub(super) fn reset(&mut self) {
        self.pending = None;
        self.regime.reset();
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.pending.map(PlannedEvent::deadline)
    }

    pub(super) fn take_due(&mut self, now: Instant) -> Option<PlannedEvent> {
        let event = self.pending?;
        if event.deadline() <= now {
            self.pending = None;
            Some(event)
        } else {
            None
        }
    }
}

impl PlannerState {
    pub(super) fn new() -> Self {
        Self {
            asteroid: EventPlanState::new(),
            sunray: EventPlanState::new(),
        }
    }

    pub(super) fn plan(&self, kind: EventKind) -> &EventPlanState {
        match kind {
            EventKind::Asteroid => &self.asteroid,
            EventKind::Sunray => &self.sunray,
        }
    }

    pub(super) fn plan_mut(&mut self, kind: EventKind) -> &mut EventPlanState {
        match kind {
            EventKind::Asteroid => &mut self.asteroid,
            EventKind::Sunray => &mut self.sunray,
        }
    }

    pub(super) fn reset(&mut self, kind: EventKind) {
        self.plan_mut(kind).reset();
    }

    pub(super) fn take_due(&mut self, kind: EventKind, now: Instant) -> Option<PlannedEvent> {
        self.plan_mut(kind).take_due(now)
    }
}

impl SchedulerState {
    pub(super) fn new() -> Self {
        Self {
            planner: PlannerState::new(),
            asteroids_enabled: false,
            sunrays_enabled: false,
            last_regime_step_at: Instant::now(),
        }
    }

    pub(super) fn sync_enabled_flags(&mut self, asteroids_enabled: bool, sunrays_enabled: bool) {
        let was_any_enabled = self.asteroids_enabled || self.sunrays_enabled;

        if self.asteroids_enabled && !asteroids_enabled {
            self.planner.reset(EventKind::Asteroid);
        }
        if self.sunrays_enabled && !sunrays_enabled {
            self.planner.reset(EventKind::Sunray);
        }

        self.asteroids_enabled = asteroids_enabled;
        self.sunrays_enabled = sunrays_enabled;

        let is_any_enabled = self.asteroids_enabled || self.sunrays_enabled;
        if !was_any_enabled && is_any_enabled {
            self.last_regime_step_at = Instant::now();
        }
    }

    pub(super) fn is_enabled(&self, kind: EventKind) -> bool {
        match kind {
            EventKind::Asteroid => self.asteroids_enabled,
            EventKind::Sunray => self.sunrays_enabled,
        }
    }

    pub(super) fn maybe_advance_regimes(&mut self, now: Instant, step_interval: Duration) {
        if now.duration_since(self.last_regime_step_at) < step_interval {
            return;
        }

        if self.asteroids_enabled {
            self.planner.asteroid.regime.advance(EventKind::Asteroid);
        }
        if self.sunrays_enabled {
            self.planner.sunray.regime.advance(EventKind::Sunray);
        }

        self.last_regime_step_at = now;
    }

    pub(super) fn apply_manual_regime_requests(&mut self, requests: ManualRegimeRequests) {
        self.planner
            .asteroid
            .regime
            .advance_by(EventKind::Asteroid, requests.asteroid_increase);
        self.planner
            .asteroid
            .regime
            .relax_by(requests.asteroid_decrease);

        self.planner
            .sunray
            .regime
            .advance_by(EventKind::Sunray, requests.sunray_increase);
        self.planner
            .sunray
            .regime
            .relax_by(requests.sunray_decrease);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabling_an_event_clears_its_pending_plan_and_regime() {
        let mut state = SchedulerState::new();
        let scheduled_at = Instant::now();

        state.sync_enabled_flags(true, true);
        state.planner.asteroid.pending = Some(PlannedEvent::new(
            EventKind::Asteroid,
            7,
            Duration::from_secs(2),
            scheduled_at,
        ));
        state.planner.asteroid.regime.advance(EventKind::Asteroid);

        state.sync_enabled_flags(false, true);

        assert!(state.planner.asteroid.pending.is_none());
        assert_eq!(state.planner.asteroid.regime.level(), 0);
        assert!(state.is_enabled(EventKind::Sunray));
    }

    #[test]
    fn regimes_advance_when_step_interval_elapses() {
        let mut state = SchedulerState::new();

        state.sync_enabled_flags(true, true);
        let start = Instant::now();
        state.maybe_advance_regimes(start + Duration::from_secs(10), Duration::from_secs(5));

        assert_eq!(state.planner.asteroid.regime.level(), 1);
        assert_eq!(state.planner.sunray.regime.level(), 1);
    }

    #[test]
    fn manual_regime_requests_are_applied_per_event_kind() {
        let mut state = SchedulerState::new();

        state.apply_manual_regime_requests(ManualRegimeRequests {
            asteroid_increase: 2,
            asteroid_decrease: 1,
            sunray_increase: 1,
            sunray_decrease: 0,
        });

        assert_eq!(state.planner.asteroid.regime.level(), 1);
        assert_eq!(state.planner.sunray.regime.level(), 1);
    }
}
