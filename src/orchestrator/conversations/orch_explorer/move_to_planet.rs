mod errors;
mod wait_incoming_response;
mod wait_move_response;
mod wait_outgoing_response;
mod wait_travel_request;

use crate::galaxy_setup::PlanetMap;
use crate::orchestrator::conversations::{PossibleExpectedKinds, ToExplorerStruct, ToPlanetStruct};
use crate::orchestrator::{ExplorersLocationRef, PlanetExplorerChannels};
use common_game::utils::ID;

///**Move To Planet Conversation - State Container**
///
/// This generic struct acts as the container for the FSM. The `State` parameter
/// determines which `Conversation` trait implementation is active, effectively
/// controlling the available transitions and expected messages.
struct MoveToPlanetConversation<State> {
    /// Unique identifier for the conversation instance.
    id: ID,
    /// The current state data.
    state: State,
    /// The specific message type the Orchestrator should look for to advance this conversation.
    expected_message: Option<PossibleExpectedKinds>,
}

// --- States Definitions ---

/// **State 1: WaitingTravelRequest**
///
/// The initial state where the conversation waits for the Explorer to request movement.
/// It contains all necessary references to validate the move against the galaxy map.
pub(crate) struct WaitingTravelRequest {
    /// Reference to the galaxy structure to check planet connectivity.
    galaxy: PlanetMap,
    /// Channel map to resolve new communication paths between planets and explorers.
    planet_explorer_channels: PlanetExplorerChannels,
    /// Connection info for the current planet (source).
    curr_planet_struct: ToPlanetStruct,
    /// Connection info for the target planet (destination).
    dst_planet_struct: ToPlanetStruct,
    /// Connection info for the explorer performing the move.
    explorer_struct: ToExplorerStruct,
    /// Reference to the global explorer location list.
    explorers_location_ref: ExplorersLocationRef,
}

/// **State 2: WaitingIncomingResponse**
///
/// Set after the destination planet has been asked to accept the explorer.
/// Holds references to the current planet to initiate the "release" phase next.
pub(crate) struct WaitingIncomingResponse {
    curr_planet_struct: ToPlanetStruct,
    explorer_struct: ToExplorerStruct,
    dst_planet_id: ID,
    planet_explorer_channels: PlanetExplorerChannels,
    explorers_location_ref: ExplorersLocationRef,
}

impl WaitingIncomingResponse {
    pub(crate) fn new(
        curr_planet_struct: ToPlanetStruct,
        explorer_struct: ToExplorerStruct,
        dst_planet_id: ID,
        planet_explorer_channels: PlanetExplorerChannels,
        explorers_location_ref: ExplorersLocationRef,
    ) -> Self {
        Self {
            curr_planet_struct,
            explorer_struct,
            dst_planet_id,
            planet_explorer_channels,
            explorers_location_ref,
        }
    }
}

/// **State 3: WaitingOutgoingResponse**
///
/// Set after the destination has accepted and the source planet has been asked to
/// release the explorer. Once this resolves, the Orchestrator will hand the new
/// channel to the Explorer.
pub(crate) struct WaitingOutgoingResponse {
    explorer_struct: ToExplorerStruct,
    planet_explorer_channels: PlanetExplorerChannels,
    dst_planet_id: ID,
    explorers_location_ref: ExplorersLocationRef,
}

impl WaitingOutgoingResponse {
    pub(crate) fn new(
        explorer_struct: ToExplorerStruct,
        planet_explorer_channels: PlanetExplorerChannels,
        dst_planet_id: ID,
        explorers_location_ref: ExplorersLocationRef,
    ) -> Self {
        Self {
            explorer_struct,
            planet_explorer_channels,
            dst_planet_id,
            explorers_location_ref,
        }
    }
}

/// **State 4: WaitMoveToPlanetResponse**
///
/// The final confirmation state. It waits for the Explorer to acknowledge it
/// has switched to the new planet's channel.
pub(crate) struct WaitMoveToPlanetResponse {
    /// ID of the explorer.
    explorer_id: ID,
    /// Reference to the global list to be updated upon final success.
    explorers_location_ref: ExplorersLocationRef,
    /// Flag to determine if a location update is actually required (False if move was denied).
    is_explorer_moving: bool,
    /// The ID of the planet the explorer is moving to.
    dst_planet_id: ID,
}

impl WaitMoveToPlanetResponse {
    pub(crate) fn new(
        explorers_location_ref: ExplorersLocationRef,
        is_explorer_moving: bool,
        dst_planet_id: ID,
        explorer_id: ID,
    ) -> Self {
        Self {
            explorer_id,
            explorers_location_ref,
            is_explorer_moving,
            dst_planet_id,
        }
    }
}
