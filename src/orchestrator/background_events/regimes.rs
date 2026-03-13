//! Game-step-scaled delay regimes for background events.

use super::EventKind;
use crate::globals::get_game_step;
use std::cmp::max;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
struct RegimeValues {
    min_delay: f32,
    max_delay: f32,
}

const ASTEROID_REGIMES: [RegimeValues; 3] = [
    RegimeValues {
        min_delay: 20.0,
        max_delay: 150.0,
    },
    RegimeValues {
        min_delay: 2.0,
        max_delay: 10.0,
    },
    RegimeValues {
        min_delay: 1.0,
        max_delay: 5.0,
    },
];

const SUNRAY_REGIMES: [RegimeValues; 3] = [
    RegimeValues {
        min_delay: 1.0,
        max_delay: 5.0,
    },
    RegimeValues {
        min_delay: 0.5,
        max_delay: 2.0,
    },
    RegimeValues {
        min_delay: 0.1,
        max_delay: 1.0,
    },
];

#[derive(Clone, Debug)]
pub(super) struct RegimeState {
    level: usize,
}

impl RegimeState {
    pub(super) fn new() -> Self {
        Self { level: 0 }
    }

    pub(super) fn reset(&mut self) {
        self.level = 0;
    }

    pub(super) fn delay_range(&self, kind: EventKind) -> (Duration, Duration) {
        let regime = &regime_values(kind)[self.level];
        let step = scaled_game_step();
        (
            step.mul_f32(regime.min_delay),
            step.mul_f32(regime.max_delay),
        )
    }

    pub(super) fn advance(&mut self, kind: EventKind) {
        if self.level + 1 < regime_values(kind).len() {
            self.level += 1;
        }
    }

    pub(super) fn advance_by(&mut self, kind: EventKind, steps: u64) {
        let max_level = regime_values(kind).len().saturating_sub(1);
        let steps = usize::try_from(steps).unwrap_or(usize::MAX);
        self.level = self.level.saturating_add(steps).min(max_level);
    }

    pub(super) fn relax(&mut self) {
        self.level = self.level.saturating_sub(1);
    }

    pub(super) fn relax_by(&mut self, steps: u64) {
        let steps = usize::try_from(steps).unwrap_or(usize::MAX);
        self.level = self.level.saturating_sub(steps);
    }

    pub(super) fn level(&self) -> usize {
        self.level
    }
}

fn regime_values(kind: EventKind) -> &'static [RegimeValues] {
    match kind {
        EventKind::Asteroid => &ASTEROID_REGIMES,
        EventKind::Sunray => &SUNRAY_REGIMES,
    }
}

fn scaled_game_step() -> Duration {
    max(Duration::from_millis(500), get_game_step())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asteroid_delay_range_scales_with_current_game_step() {
        let regime = RegimeState::new();
        let step = max(Duration::from_millis(500), get_game_step());
        let delay_range = regime.delay_range(EventKind::Asteroid);

        assert_eq!(delay_range.0, step.mul_f32(20.0));
        assert_eq!(delay_range.1, step.mul_f32(150.0));
    }

    #[test]
    fn regime_level_stays_within_bounds() {
        let mut regime = RegimeState::new();

        regime.advance(EventKind::Sunray);
        regime.advance(EventKind::Sunray);
        regime.advance(EventKind::Sunray);
        regime.relax();

        assert_eq!(regime.level(), 1);
    }
}
