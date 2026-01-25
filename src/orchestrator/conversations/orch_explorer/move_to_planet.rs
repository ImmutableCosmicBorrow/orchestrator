mod errors;
mod incoming_explorer;
mod manual_move_to_planet;
mod move_explorer;
mod outgoing_explorer;
mod wait_travel_request;

use crate::orchestrator::conversations::{PossibleExpectedKinds, ToExplorerStruct, ToPlanetStruct};
use crate::orchestrator::{ExplorersLocationRef, PlanetExplorerChannels};
use crate::planet::PlanetMap;
use common_game::utils::ID;

///**Move To Planet Conversation - State Container**
///
/// This generic struct acts as the primary container for the Movement Finite State Machine (FSM).
/// The `State` type parameter determines the current lifecycle phase of the movement,
/// controlling valid transitions and defining which messages the conversation in this specific state expects to receive.
struct MoveToPlanetConversation<State> {
    /// Unique identifier for this specific conversation instance.
    id: ID,
    /// The data and context specific to the current lifecycle state.
    state: State,
    /// The specific message type the Orchestrator polls for to advance this conversation.
    expected_message: Option<PossibleExpectedKinds>,
}

// --- States Definitions ---

/// **State 1: `WaitingTravelRequest`**
///
/// The entry point for explorer-initiated movement. The conversation remains in this state
/// while waiting for an Explorer to send a travel request.
pub(crate) struct WaitingTravelRequest {
    /// Reference to the galaxy map used to verify if the destination is a valid neighbor.
    galaxy: PlanetMap,
    /// Registry used to resolve and update communication channels between entities.
    planet_explorer_channels: PlanetExplorerChannels,
    /// Wrapper for communicating with the explorer's current planet.
    curr_planet_struct: ToPlanetStruct,
    /// Wrapper for communicating with the explorer's target planet.
    dst_planet_struct: ToPlanetStruct,
    /// Wrapper for communicating with the explorer entity itself.
    explorer_struct: ToExplorerStruct,
    /// Thread-safe reference to the global registry of explorer locations.
    explorers_location_ref: ExplorersLocationRef,
}

/// **Alternative Start State: `SendManualMoveRequest`**
///
/// An alternative entry point to the FSM. Used when the Orchestrator initiates
/// a move directly (e.g., via administrative command) rather than responding
/// to an explorer's request.
pub(crate) struct SendManualMoveRequest {
    /// Registry to resolve channels for the forced movement.
    planet_explorer_channels: PlanetExplorerChannels,
    /// Connection info for the current planet (source).
    curr_planet_struct: ToPlanetStruct,
    /// Connection info for the target planet (destination).
    dst_planet_struct: ToPlanetStruct,
    /// Connection info for the explorer performing the move.
    explorer_struct: ToExplorerStruct,
    /// Reference to the global explorer location registry.
    explorers_location_ref: ExplorersLocationRef,
}

impl SendManualMoveRequest {
    pub(crate) fn new(
        explorers_location_ref: ExplorersLocationRef,
        curr_planet_struct: ToPlanetStruct,
        dst_planet_struct: ToPlanetStruct,
        explorer_struct: ToExplorerStruct,
        planet_explorer_channels: PlanetExplorerChannels,
    ) -> Self {
        Self {
            planet_explorer_channels,
            curr_planet_struct,
            dst_planet_struct,
            explorer_struct,
            explorers_location_ref,
        }
    }
}
/// **State 2: `SendIncomingRequest`**
///
/// An action state where the Orchestrator prepares to notify the destination planet
/// of an incoming explorer.
pub(crate) struct SendIncomingRequest {
    /// Connection info for the source planet.
    curr_planet_struct: ToPlanetStruct,
    /// Connection info for the moving explorer.
    explorer_struct: ToExplorerStruct,
    /// Connection info for the destination planet.
    dst_planet_struct: ToPlanetStruct,
    /// Channel registry for resolving new entity paths.
    planet_explorer_channels: PlanetExplorerChannels,
    /// Global explorer location registry.
    explorers_location_ref: ExplorersLocationRef,
    /// Flag determining if the source planet needs to be notified of the departure.
    handle_outgoing: bool,
}

impl SendIncomingRequest {
    pub(crate) fn new(
        curr_planet_struct: ToPlanetStruct,
        explorer_struct: ToExplorerStruct,
        dst_planet_struct: ToPlanetStruct,
        planet_explorer_channels: PlanetExplorerChannels,
        explorers_location_ref: ExplorersLocationRef,
        handle_outgoing: bool,
    ) -> Self {
        Self {
            curr_planet_struct,
            explorer_struct,
            dst_planet_struct,
            planet_explorer_channels,
            explorers_location_ref,
            handle_outgoing,
        }
    }
}

/// **State 3: `WaitingIncomingResponse`**
///
/// A waiting state where the Orchestrator expects a response from the destination
/// planet regarding the acquisition of the explorer.
pub(crate) struct WaitingIncomingResponse {
    /// Context for the source planet to be used if acquisition is accepted.
    curr_planet_struct: ToPlanetStruct,
    /// Context for the moving explorer.
    explorer_struct: ToExplorerStruct,
    /// ID of the destination planet.
    dst_planet_id: ID,
    /// Channel registry for resolving new entity paths.
    planet_explorer_channels: PlanetExplorerChannels,
    /// Global explorer location registry.
    explorers_location_ref: ExplorersLocationRef,
    /// Flag indicating if the source planet release handshake is required.
    handle_outgoing: bool,
}

impl WaitingIncomingResponse {
    pub(crate) fn new(
        curr_planet_struct: ToPlanetStruct,
        explorer_struct: ToExplorerStruct,
        dst_planet_id: ID,
        planet_explorer_channels: PlanetExplorerChannels,
        explorers_location_ref: ExplorersLocationRef,
        handle_outgoing: bool,
    ) -> Self {
        Self {
            curr_planet_struct,
            explorer_struct,
            dst_planet_id,
            planet_explorer_channels,
            explorers_location_ref,
            handle_outgoing,
        }
    }
}

/// **State 4: `SendOutgoingRequest`**
///
/// An action state reached after the destination planet has accepted the explorer.
/// The Orchestrator now commands the source planet to "let go" of the explorer entity.
pub(crate) struct SendOutgoingRequest {
    /// Context for the source planet being commanded to release the explorer.
    curr_planet_struct: ToPlanetStruct,
    /// Context for the moving explorer.
    explorer_struct: ToExplorerStruct,
    /// Registry for channel management.
    planet_explorer_channels: PlanetExplorerChannels,
    /// ID of the destination planet the explorer is heading toward.
    dst_planet_id: ID,
    /// Global explorer location registry.
    explorers_location_ref: ExplorersLocationRef,
}

impl SendOutgoingRequest {
    pub(crate) fn new(
        curr_planet_struct: ToPlanetStruct,
        explorer_struct: ToExplorerStruct,
        planet_explorer_channels: PlanetExplorerChannels,
        dst_planet_id: ID,
        explorers_location_ref: ExplorersLocationRef,
    ) -> Self {
        Self {
            curr_planet_struct,
            explorer_struct,
            planet_explorer_channels,
            dst_planet_id,
            explorers_location_ref,
        }
    }
}

/// **State 5: `WaitingOutgoingResponse`**
///
/// A waiting state where the Orchestrator expects the source planet to confirm
/// that the explorer has been successfully released.
pub(crate) struct WaitingOutgoingResponse {
    /// Context for the moving explorer.
    explorer_struct: ToExplorerStruct,
    /// Registry for providing the explorer with new planet channels upon release.
    planet_explorer_channels: PlanetExplorerChannels,
    /// Target destination ID.
    dst_planet_id: ID,
    /// Global explorer location registry.
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

/// **State 6: `SendMoveRequest`**
///
/// Reached after both planets have acknowledged the move. The Orchestrator
/// now sends the final `MoveToPlanet` command to the explorer, including
/// the destination's communication channel if the move is authorized.
pub(crate) struct SendMoveRequest {
    /// Registry used to provide the destination's channel to the explorer.
    planet_explorer_channels: PlanetExplorerChannels,
    /// ID of the planet the explorer is moving to.
    dst_planet_id: ID,
    /// Context for the moving explorer.
    explorer_struct: ToExplorerStruct,
    /// Global explorer location registry.
    explorers_location_ref: ExplorersLocationRef,
    /// Boolean flag; if false, the explorer is notified that the move was denied.
    is_explorer_moving: bool,
}

impl SendMoveRequest {
    pub(crate) fn new(
        explorers_location_ref: ExplorersLocationRef,
        dst_planet_id: ID,
        explorer_struct: ToExplorerStruct,
        planet_explorer_channels: PlanetExplorerChannels,
        is_explorer_moving: bool,
    ) -> Self {
        Self {
            planet_explorer_channels,
            dst_planet_id,
            explorer_struct,
            explorers_location_ref,
            is_explorer_moving,
        }
    }
}

/// **State 7: `WaitMoveToPlanetResponse`**
///
/// The final terminal state. The Orchestrator waits for the explorer to confirm
/// that it has successfully transitioned to the new planet's channel.
pub(crate) struct WaitMoveToPlanetResponse {
    /// ID of the explorer entity.
    explorer_id: ID,
    /// Reference to the global list to be updated upon final transition success.
    explorers_location_ref: ExplorersLocationRef,
    /// Determines if the global location list should be updated (false if the move was rejected).
    is_explorer_moving: bool,
    /// The ID of the planet the explorer is arriving at.
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
