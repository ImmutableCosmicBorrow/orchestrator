use crate::globals::get_game_step;
use std::time::Duration;

/// Centralized conversation parameters used across conversation modules.
/// Timeouts are computed proportional to the `game_step` (runtime value) to
/// make timings scale with simulation speed.
fn ms_from_step_mult(mult: u128) -> u128 {
    let step_ms = get_game_step().as_millis();
    step_ms.saturating_mul(mult)
}

fn dur_from_ms_u128(ms: u128) -> Duration {
    #[allow(clippy::cast_possible_truncation)]
    let ms_u64 = ms.min(u128::from(u64::MAX)) as u64;
    Duration::from_millis(ms_u64)
}

/// Asteroid ack timeout: proportional multiplier of the game step (default ~4x).
pub(crate) fn asteroid_ack_timeout() -> Duration {
    dur_from_ms_u128(ms_from_step_mult(4))
}

/// Sunray ack timeout: proportional multiplier of the game step (default ~2x).
pub(crate) fn sunray_ack_timeout() -> Duration {
    dur_from_ms_u128(ms_from_step_mult(2))
}

// --- Priority types ---
/// Priority levels for conversations/events, used to determine processing order in the scheduler.
///
/// Higher priority conversations are processed before lower ones. Assign priorities by
/// considering safety (does the event affect core correctness?), timeliness (is latency
/// observable or user-facing?), and impact (does the event block other work?).
///
/// Recommended decision rules:
/// - Safety & lifecycle: use `High`/`Max` for events that start/stop/kill critical entities
///   (planets, explorers) or which would leave the system in an inconsistent state if delayed.
/// - Latency-sensitive user-visible actions: use `High` for movement/transfer actions that
///   materially affect gameplay flow; prefer `Medium` for regular state requests/responses.
/// - Background bookkeeping: use `Low` or `Min` for non-essential periodic updates or
///   ephemeral visual effects where delay is acceptable.
/// - Resource and scenario processing: default to `Medium` unless the scenario can cause
///   cascading failures, in which case bump to `High`.
///
/// Numeric mapping (higher = processed earlier):
/// - `Max (5)`: immediate, critical system changes (use sparingly)
/// - `High (4)`: lifecycle and latency-sensitive operations
/// - `Medium (3)`: normal gameplay flows, resource handling, request/response
/// - `Low (2)`: background tasks and non-critical updates
/// - `Min (1)`: optional or purely cosmetic events
///
/// Examples:
/// - Planet `Kill/Start/Stop` -> `High`/`Max` (prevents inconsistent world state)
/// - Explorer `MoveExplorerHigh` -> `High` (player-visible movement)
/// - `RequestState`/`ResponseState` -> `Medium` (important but not urgent)
/// - `Sunray` gameplay events -> `Medium` (gameplay-relevant but not lifecycle-critical)
///
/// Keep priorities conservative: prefer `Medium` and raise only when you can justify
/// stricter ordering (safety or user-perceived latency). Document any deviations from
/// these rules in the conversation implementation to aid future reviewers.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub(crate) enum PriorityLevel {
    Min = 1,
    Low = 2,
    Medium = 3,
    High = 4,
    Max = 5,
}

impl PriorityLevel {
    pub(crate) fn as_i32(self) -> i32 {
        self as i32
    }
}

pub(crate) enum EventKind {
    // Planet lifecycle
    KillPlanet,
    StartPlanet,
    StopPlanet,

    // Planet internal scheduling/state
    InternalState,

    // Explorer lifecycle & movement
    KillExplorer,
    StartExplorer,
    StopExplorer,
    ResetExplorer,

    OutgoingExplorer,
    IncomingExplorer,
    MoveExplorerHigh,
    MoveExplorerLow,
    ManualMoveToPlanet,
    WaitTravelRequest,
    NeighborsDiscovery,

    // Resource-related conversation scenarios
    BagContentScenario,
    CombineResource,
    CraftResource,
    SupportedCombination,
    SupportedResources,

    // Galaxy/planet events
    AdvDeadExplorer,
    Asteroid,
    Sunray,

    // Generic request/response macros
    RequestState,
    ResponseState,
}

impl EventKind {
    pub(crate) fn priority(self) -> PriorityLevel {
        #[allow(clippy::match_same_arms)]
        match self {
            // Sunray events are important for gameplay but not lifecycle-critical, so we assign them a medium priority.
            EventKind::Sunray => PriorityLevel::Medium,

            // Planet/internal state
            EventKind::InternalState => PriorityLevel::Low,
            EventKind::RequestState | EventKind::ResponseState => PriorityLevel::Medium,

            // Planet lifecycle => critical (Max)
            EventKind::KillPlanet | EventKind::StartPlanet | EventKind::StopPlanet => {
                PriorityLevel::Max
            }

            // Explorer lifecycle: critical for start/kill/stop (Max), reset is medium
            EventKind::KillExplorer | EventKind::StartExplorer | EventKind::StopExplorer => {
                PriorityLevel::Max
            }
            EventKind::ResetExplorer => PriorityLevel::High,

            // Movement / explorer flow
            EventKind::MoveExplorerHigh => PriorityLevel::High,
            EventKind::MoveExplorerLow
            | EventKind::OutgoingExplorer
            | EventKind::IncomingExplorer
            | EventKind::ManualMoveToPlanet
            | EventKind::WaitTravelRequest => PriorityLevel::Medium,

            EventKind::NeighborsDiscovery => PriorityLevel::Medium,

            // Resource scenarios treated as medium
            EventKind::BagContentScenario
            | EventKind::CombineResource
            | EventKind::CraftResource
            | EventKind::SupportedCombination
            | EventKind::SupportedResources => PriorityLevel::Medium,

            // Galaxy/planet events: significant but not lifecycle-critical
            EventKind::AdvDeadExplorer | EventKind::Asteroid => PriorityLevel::High,
        }
    }

    pub(crate) fn priority_i32(self) -> i32 {
        self.priority().as_i32()
    }
}
