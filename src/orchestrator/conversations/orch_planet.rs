pub(crate) mod adv_dead_explorer;
mod asteroid_scenario;
mod internal_state_scenario;
mod kill_planet;
mod start_planet;
mod stop_planet;
mod sunray_scenario;

pub(crate) use asteroid_scenario::{AsteroidConversation, SendingAsteroid};
pub(crate) use sunray_scenario::{SendSunray, SunrayConversation};

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
