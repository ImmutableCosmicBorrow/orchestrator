//! Delay and target selection policy for background events.

use super::EventKind;
use super::regimes::RegimeState;
use super::state::{EventPlanState, PlannedEvent};
use crate::orchestrator::ExplorersLocationRef;
use common_game::utils::ID;
use rand::Rng;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const DEFAULT_PLANET_WEIGHT: u64 = 1;
const EXPLORER_WEIGHT: u64 = 5;
const MAX_DELAY_REDUCTION_SECS: u64 = 5;

pub(super) fn schedule_next_event(
    kind: EventKind,
    planet_ids: &[ID],
    explorers_location: &ExplorersLocationRef,
    plan_state: &mut EventPlanState,
    scheduled_at: Instant,
) -> Option<PlannedEvent> {
    if let Some(existing) = plan_state.pending {
        if planet_ids.contains(&existing.planet_id) {
            return Some(existing);
        }

        // The target planet was killed/removed after the plan was created.
        // Drop the stale plan so the scheduler can pick a new alive target.
        plan_state.pending = None;
    }

    let (planet_id, weight) = select_weighted_planet(planet_ids, explorers_location)?;
    let explorers_on_planet = explorer_count_from_weight(weight);
    let delay = compute_delay(kind, &plan_state.regime, explorers_on_planet);
    let event = PlannedEvent::new(kind, planet_id, delay, scheduled_at);
    plan_state.pending = Some(event);
    Some(event)
}

fn select_weighted_planet(
    planet_ids: &[ID],
    explorers_location: &ExplorersLocationRef,
) -> Option<(ID, u64)> {
    let weights = build_planet_weights(planet_ids, explorers_location);
    let mut rng = rand::rng();
    select_weighted_planet_with_rng(planet_ids, &weights, &mut rng)
}

fn build_planet_weights(
    planet_ids: &[ID],
    explorers_location: &ExplorersLocationRef,
) -> HashMap<ID, u64> {
    let mut weights: HashMap<ID, u64> = planet_ids
        .iter()
        .map(|&planet_id| (planet_id, DEFAULT_PLANET_WEIGHT))
        .collect();

    for entry in explorers_location {
        let planet_id = entry.value();
        if let Some(weight) = weights.get_mut(planet_id) {
            *weight = weight.saturating_add(EXPLORER_WEIGHT);
        }
    }

    weights
}

fn select_weighted_planet_with_rng<R>(
    planet_ids: &[ID],
    weights: &HashMap<ID, u64>,
    rng: &mut R,
) -> Option<(ID, u64)>
where
    R: Rng + ?Sized,
{
    if planet_ids.is_empty() {
        return None;
    }

    let total_weight: u64 = weights.values().copied().sum();
    if total_weight == 0 {
        return None;
    }

    let mut pick = rng.random_range(0..total_weight);
    for &planet_id in planet_ids {
        let weight = *weights.get(&planet_id).unwrap_or(&0);
        if pick < weight {
            return Some((planet_id, weight));
        }
        pick = pick.saturating_sub(weight);
    }

    None
}

fn compute_delay(kind: EventKind, regime: &RegimeState, explorers_on_planet: u64) -> Duration {
    let (min_delay, max_delay) = regime.delay_range(kind);
    let min_millis = duration_to_millis(min_delay);
    let max_millis = duration_to_millis(max_delay).max(min_millis);
    let mut rng = rand::rng();
    let base_millis = if min_millis == max_millis {
        min_millis
    } else {
        rng.random_range(min_millis..=max_millis)
    };

    apply_delay_reduction(
        Duration::from_millis(base_millis),
        explorers_on_planet.min(MAX_DELAY_REDUCTION_SECS),
    )
}

fn apply_delay_reduction(delay: Duration, explorers_on_planet: u64) -> Duration {
    delay.saturating_sub(Duration::from_secs(explorers_on_planet))
}

fn explorer_count_from_weight(weight: u64) -> u64 {
    if weight <= DEFAULT_PLANET_WEIGHT {
        0
    } else {
        (weight - DEFAULT_PLANET_WEIGHT) / EXPLORER_WEIGHT
    }
}

fn duration_to_millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashmap::DashMap;

    #[test]
    fn build_planet_weights_counts_baseline_and_explorers() {
        let explorers_location = DashMap::from_iter([(10, 1), (11, 1), (12, 2)]);
        let weights = build_planet_weights(&[1, 2, 3], &explorers_location);

        assert_eq!(
            weights.get(&1),
            Some(&(DEFAULT_PLANET_WEIGHT + 2 * EXPLORER_WEIGHT))
        );
        assert_eq!(
            weights.get(&2),
            Some(&(DEFAULT_PLANET_WEIGHT + EXPLORER_WEIGHT))
        );
        assert_eq!(weights.get(&3), Some(&DEFAULT_PLANET_WEIGHT));
    }

    #[test]
    fn schedule_next_event_keeps_single_available_planet() {
        let explorers_location = DashMap::new();
        let mut plan_state = EventPlanState::new();
        let scheduled_at = Instant::now();

        let event = schedule_next_event(
            EventKind::Asteroid,
            &[42],
            &explorers_location,
            &mut plan_state,
            scheduled_at,
        )
        .expect("expected a plan for the only planet");

        assert_eq!(event.kind, EventKind::Asteroid);
        assert_eq!(event.planet_id, 42);
        assert_eq!(plan_state.pending, Some(event));
    }

    #[test]
    fn apply_delay_reduction_saturates_at_zero() {
        let reduced = apply_delay_reduction(Duration::from_millis(750), 2);

        assert_eq!(reduced, Duration::ZERO);
    }
}
