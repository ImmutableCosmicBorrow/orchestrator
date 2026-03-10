use crate::orchestrator::conversations::ErrorType;
use common_game::utils::ID;

///**Move To Planet Errors**
///
/// This enum represents the various failure states that can occur during the
/// complex process of moving an explorer from one planet to another.
///
/// It covers network communication failures, business logic rejections from
/// planets, and internal state inconsistencies within the Orchestrator.
pub(crate) enum MoveToPlanetErrors {
    /// Failed to deliver the [`IncomingExplorerRequest`] to the destination planet.
    IncomingMessageFailed(ID),
    /// Failed to deliver the [`OutgoingExplorerRequest]` to the source planet.
    OutgoingMessageFailed(ID),
    /// The destination planet received the request but explicitly failed or
    /// rejected the acquisition of the explorer.
    DstPlanetFailed { planet_id: ID, explorer_id: ID },
    /// The current planet received the request but failed to successfully
    /// release the explorer.
    CurrPlanetFailed { planet_id: ID, explorer_id: ID },
    /// A critical inconsistency where the planets have updated their channels,
    /// but the Orchestrator cannot find the sender for the new destination.
    NewSenderToPlanetNotFound(ID),
}

impl ErrorType for MoveToPlanetErrors {
    /// Returns a descriptive string explaining the specific movement error,
    /// including relevant Planet and Explorer IDs.
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
        }
    }
}
