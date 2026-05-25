pub(crate) mod galaxy_events;
pub(crate) mod lifecycle;
pub(crate) mod test_utils;
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
