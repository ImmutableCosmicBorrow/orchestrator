mod errors;
pub(crate) mod incoming_explorer;
pub(crate) mod manual_move_to_planet;
pub(crate) mod move_explorer;
pub(crate) mod outgoing_explorer;
pub(crate) mod wait_travel_request;

use crate::channels_manager::PlanetExplorerChannels;
use crate::define_conversation;
use crate::orchestrator::ExplorersLocationRef;
use crate::planet::PlanetMap;
use common_game::utils::ID;

///**Move To Planet Conversation - State Container**
///
/// This generic struct acts as the primary container for the Movement Finite State Machine (FSM).
/// The `State` type parameter determines the current lifecycle phase of the movement,
/// controlling valid transitions and defining which messages the conversation in this specific state expects to receive.

// --- MOVE TO PLANET CONVERSATION ---
define_conversation!(
    name: MoveToPlanetConversation
);
