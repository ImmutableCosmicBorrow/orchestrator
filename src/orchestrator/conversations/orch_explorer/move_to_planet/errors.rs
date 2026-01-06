use crate::orchestrator::conversations::ErrorType;
use common_game::utils::ID;

pub(crate) enum MoveToPlanetErrors {
    IncomingMessageFailed(ID),
    OutgoingMessageFailed(ID),
    DstPlanetFailed { planet_id: ID, explorer_id: ID },
    CurrPlanetFailed { planet_id: ID, explorer_id: ID },
    NewSenderToPlanetNotFound(ID),
    ExplorerLocationNotFound(ID),
}

impl ErrorType for MoveToPlanetErrors {
    fn stringify(&self) -> String {
        match self {
            MoveToPlanetErrors::IncomingMessageFailed(id) => {
                format!("Failed to send Incoming message to destination planet {id}")
            }
            MoveToPlanetErrors::OutgoingMessageFailed(id) => {
                format!("Failed to send Outgoing message to current planet {id}")
            }
            MoveToPlanetErrors::DstPlanetFailed {
                planet_id,
                explorer_id,
            } => format!(
                "Destination planet {planet_id} failed to acquire incoming explorer {explorer_id}"
            ),
            MoveToPlanetErrors::CurrPlanetFailed {
                planet_id,
                explorer_id,
            } => format!(
                "Current planet {planet_id} failed to let go of outgoing explorer {explorer_id}"
            ),
            MoveToPlanetErrors::NewSenderToPlanetNotFound(id) => format!(
                "sender to dest planet {id} not found, planets already changed explorer channels but explorer did not"
            ),
            MoveToPlanetErrors::ExplorerLocationNotFound(id) => {
                format!("The location for explorer {id} is not found in the list")
            }
        }
    }
}
