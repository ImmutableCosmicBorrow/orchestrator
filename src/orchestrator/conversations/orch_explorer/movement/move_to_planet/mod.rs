mod errors;
pub(crate) mod incoming_explorer;
pub(crate) mod manual_move_to_planet;
pub(crate) mod move_explorer;
pub(crate) mod outgoing_explorer;
pub(crate) mod wait_travel_request;

use crate::define_conversation;

//**Move To Planet Conversation - State Container**
//
// This generic struct acts as the primary container for the Movement Finite State Machine (FSM).
// The `State` type parameter determines the current lifecycle phase of the movement,
// controlling valid transitions and defining which messages the conversation in this specific state expects to receive.

// --- MOVE TO PLANET CONVERSATION ---
define_conversation!(
    name: MoveToPlanetConversation
);
