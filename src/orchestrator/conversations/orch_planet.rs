mod asteroid_scenario;
mod internal_state_scenario;
mod kill_planet;
pub(crate) mod outgoing_explorer;
mod start_planet;
mod stop_planet;
mod sunray_scenario;

use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind;
use common_game::utils::ID;
use std::marker::PhantomData;

struct WaitingInternalStateResponse;
struct WaitingAsteroidAck;
struct Error;

//TODO: REWRITE THIS BETTER

///`InternalState` FSM
struct PlanetInternalStateConversation<S> {
    _state: PhantomData<S>,
    expected_message: PlanetToOrchestratorKind,
    id: ID,
}
