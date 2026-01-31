pub(crate) mod adv_dead_explorer;
pub mod asteroid_scenario;
pub mod internal_state_scenario;
pub mod kill_planet;
pub mod start_planet;
pub mod stop_planet;
pub mod sunray_scenario;

use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind;
use common_game::utils::ID;
use std::marker::PhantomData;

struct WaitingInternalStateResponse;
struct WaitingAsteroidAck;
struct Error;

///`InternalState` FSM
struct PlanetInternalStateConversation<S> {
    _state: PhantomData<S>,
    expected_message: PlanetToOrchestratorKind,
    id: ID,
}
