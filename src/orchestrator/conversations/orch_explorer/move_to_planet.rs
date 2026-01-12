mod errors;
mod wait_incoming_response;
mod wait_move_response;
mod wait_outgoing_response;
mod wait_travel_request;

use crate::galaxy_setup::PlanetMap;
use crate::orchestrator::conversations::{PossibleExpectedKinds, ToExplorerStruct, ToPlanetStruct};
use crate::orchestrator::{ExplorersLocationRef, PlanetExplorerChannels};

use common_game::utils::ID;

struct MoveToPlanetConversation<State> {
    id: ID,
    state: State,
    expected_message: Option<PossibleExpectedKinds>,
}

//States
pub(crate) struct WaitingTravelRequest {
    galaxy: PlanetMap,
    planet_explorer_channels: PlanetExplorerChannels,
    curr_planet_struct: ToPlanetStruct,
    dst_planet_struct: ToPlanetStruct,
    explorer_struct: ToExplorerStruct,
    explorers_location_ref: ExplorersLocationRef,
}

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

pub(crate) struct WaitMoveToPlanetResponse {
    explorer_id: ID,
    explorers_location_ref: ExplorersLocationRef,
    is_explorer_moving: bool,
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
