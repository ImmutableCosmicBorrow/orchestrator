use crate::logging_utils::log_internal;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::orch_explorer::kill_explorers_manager::KillExplorersManager;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    SendersToExplorer, SendersToPlanet, ToPlanetError, ToPlanetStruct,
};
use crate::orchestrator::{ExplorerBag, ExplorersLocationRef};
use crate::payload;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::KillPlanetResult;
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::utils::ID;

///**Kill Planet Conversation**
///
/// This module manages the complex process of destroying a planet.
/// It uses an FSM to send the kill command, wait for confirmation, and then
/// transition to a [`KillExplorersManager`] to handle the cleanup of any explorers
/// that were stationed on that planet.
/// Marker struct for FSM state
///
/// The conversation starts in the [`SendPlanetKill`] state, which sends an
/// [`OrchestratorToPlanet::KillPlanet`] message when the [`Conversation::transition`] method is called.
pub(crate) struct SendPlanetKill {
    /// A struct containing fields to send messages to the planet
    to_planet_struct: ToPlanetStruct,
    /// Struct to send messages to explorers (passed to subsequent cleanup states)
    explorers_senders: SendersToExplorer,
    /// Reference to the list of explorers locations to identify victims on the planet
    explorers_location_ref: ExplorersLocationRef,
}

impl SendPlanetKill {
    /// Constructor for [`SendPlanetKill`] state struct
    pub(crate) fn new(
        to_planet_struct: ToPlanetStruct,
        explorers_location_ref: ExplorersLocationRef,
        explorers_senders: SendersToExplorer,
    ) -> Self {
        Self {
            to_planet_struct,
            explorers_senders,
            explorers_location_ref,
        }
    }
}

/// Marker struct for FSM state
///
/// In the [`WaitingPlanetKillResult`] state, the conversation expects a [`PlanetToOrchestrator::KillPlanetResult`].
/// Once received, it identifies all explorers currently on that planet and transitions to the explorer cleanup phase
/// in [`KillExplorersManager`].
struct WaitingPlanetKillResult {
    /// ID of the planet marked for destruction
    planet_id: ID,
    /// Reference to the list of explorers locations
    explorers_location_ref: ExplorersLocationRef,
    /// Senders used to notify explorers of their termination
    explorers_senders: SendersToExplorer,
    /// Senders used to communicate with planets
    planet_senders: SendersToPlanet,
}

impl WaitingPlanetKillResult {
    /// The constructor for [`WaitingPlanetKillResult`] state struct
    fn new(
        planet_id: ID,
        explorers_location_ref: ExplorersLocationRef,
        explorers_senders: SendersToExplorer,
        planet_senders: SendersToPlanet,
    ) -> Self {
        Self {
            planet_id,
            explorers_location_ref,
            explorers_senders,
            planet_senders,
        }
    }
}

/// Kill Planet Conversation FSM
///
/// This is the generic FSM struct that takes the generic type `State` to ensure only methods
/// of that specific state can be called during the conversation.
pub(crate) struct KillPlanetConversation<State> {
    /// Conversation ID
    id: ID,
    /// Optional expected message to trigger the transition
    expected_message: Option<PossibleExpectedKinds>,
    /// State of the FSM
    state: State,
}

// SEND PLANET KILL IMPLEMENTATION
impl Conversation<ExplorerBag> for KillPlanetConversation<SendPlanetKill> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.to_planet_struct.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`SendPlanetKill`] state:
    ///
    /// Returns:
    ///
    /// [`ErrorState`] if the message to the planet fails or the sender is not found.
    ///
    /// [`KillPlanetConversation<WaitingPlanetKillResult>`] if the kill command was sent successfully.
    fn transition(
        self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        match self
            .state
            .to_planet_struct
            .to_planet(OrchestratorToPlanet::KillPlanet)
        {
            Ok(()) => {
                let planet_id = self.state.to_planet_struct.planet_id;
                let state_struct = WaitingPlanetKillResult::new(
                    planet_id,
                    self.state.explorers_location_ref,
                    self.state.explorers_senders,
                    self.state.to_planet_struct.planets_senders,
                );
                let next_state =
                    KillPlanetConversation::<WaitingPlanetKillResult>::new(self.id, state_struct);
                Some(Box::new(next_state))
            }
            Err(err) => {
                let error = match err {
                    ToPlanetError::SendingMessageFailure(id) => {
                        CommonErrorTypes::MessageToPlanetFailed(id)
                    }
                    ToPlanetError::SenderNotFound(id) => CommonErrorTypes::PlanetSenderNotFound(id),
                };
                let error_state = ErrorState::new(Box::new(error), self.id);
                Some(Box::new(error_state))
            }
        }
    }

    fn get_priority(&self) -> i32 {
        5
    }
}

impl KillPlanetConversation<SendPlanetKill> {
    /// The constructor for [`KillPlanetConversation`] in the [`SendPlanetKill`] state
    pub(crate) fn new(id: ID, state: SendPlanetKill) -> Self {
        Self {
            id,
            expected_message: None,
            state,
        }
    }
}

// WAITING PLANET KILL RESULT IMPLEMENTATION
impl Conversation<ExplorerBag> for KillPlanetConversation<WaitingPlanetKillResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.state.planet_id
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingPlanetKillResult`] state:
    ///
    /// Returns:
    ///
    /// [`KillExplorersManager`] state to begin cleaning up explorers on the destroyed planet.
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different then the expected [`PlanetToOrchestrator::KillPlanetResult`] .
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some(PossibleMessage::PlanetToOrch(PlanetToOrchestrator::KillPlanetResult {
            planet_id,
        })) = msg_wrapped
        {
            log_internal(
                Channel::Info,
                payload!(
                    action : "Killed Planet",
                    planet_id : planet_id,
                    conversation_id : self.id
                ),
            );

            let to_kill_list = self.get_explorers_in_planet(planet_id);
            let next_state = KillExplorersManager::new(
                self.id,
                self.state.explorers_senders,
                self.state.planet_senders,
                false, //we are already killing the planet so don't have to advertise that explorers are dying
                to_kill_list,
            );
            return Some(Box::new(next_state));
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state))
    }

    fn get_priority(&self) -> i32 {
        5
    }
}

impl KillPlanetConversation<WaitingPlanetKillResult> {
    /// The constructor for [`KillPlanetConversation`] in the [`WaitingPlanetKillResult`] state
    pub(crate) fn new(id: ID, state: WaitingPlanetKillResult) -> Self {
        Self {
            id,
            expected_message: Some(PlanetToOrchKind(KillPlanetResult)),
            state,
        }
    }

    /// Helper function to filter and collect all explorers currently located on the target planet
    fn get_explorers_in_planet(&self, target_planet: ID) -> Vec<(ID, ID)> {
        self.state
            .explorers_location_ref
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, planet_id)| **planet_id == target_planet)
            .map(|(explorer_id, planet_id)| (*explorer_id, *planet_id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const CONV_ID: ID = 100;
    const PLANET_ID: ID = 200;
    const EXPLORER_ID_1: ID = 301;
    const EXPLORER_ID_2: ID = 302;

    #[test]
    fn send_success() {
        let (tx, _rx) = unbounded::<OrchestratorToPlanet>();
        let senders = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx)])));

        let to_planet = ToPlanetStruct {
            planet_id: PLANET_ID,
            planets_senders: senders,
        };

        let explorers_location = Arc::new(Mutex::new(HashMap::new()));
        let explorers_senders = Arc::new(Mutex::new(HashMap::new()));

        let state = SendPlanetKill::new(to_planet, explorers_location, explorers_senders);
        let conv = Box::new(KillPlanetConversation::<SendPlanetKill>::new(
            CONV_ID, state,
        ));

        // Transition to Waiting state
        let next_conv = conv
            .transition(None)
            .expect("Should transition to WaitingPlanetKillResult");

        // Assert: correct expected message kind
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PlanetToOrchKind(KillPlanetResult))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_entity_id(), PLANET_ID);
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn send_missing_sender() {
        let senders = Arc::new(Mutex::new(HashMap::new())); // Empty map
        let to_planet = ToPlanetStruct {
            planet_id: PLANET_ID,
            planets_senders: senders,
        };

        let explorers_location = Arc::new(Mutex::new(HashMap::new()));
        let explorers_senders = Arc::new(Mutex::new(HashMap::new()));

        let state = SendPlanetKill::new(to_planet, explorers_location, explorers_senders);
        let conv = Box::new(KillPlanetConversation::<SendPlanetKill>::new(
            CONV_ID, state,
        ));

        // Transition: should lead to error
        let next_conv = conv
            .transition(None)
            .expect("Should transition to ErrorState");

        // ASSERT: Correct error type
        assert!(next_conv.get_error_details().is_some());
        assert_eq!(
            next_conv.get_error_details().unwrap(),
            format!("sender to planet {PLANET_ID} not found")
        );
    }

    #[test]
    fn send_message_failure() {
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();
        // Drop receiver to trigger SendError
        drop(rx);

        let senders = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx)])));
        let to_planet = ToPlanetStruct {
            planet_id: PLANET_ID,
            planets_senders: senders,
        };

        let state = SendPlanetKill::new(
            to_planet,
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashMap::new())),
        );
        let conv = Box::new(KillPlanetConversation::<SendPlanetKill>::new(
            CONV_ID, state,
        ));

        let next_conv = conv.transition(None).expect("Should return an ErrorState");

        let error_msg = next_conv
            .get_error_details()
            .expect("Should return an Error Details String");
        assert_eq!(
            error_msg,
            format!("failed to send message to planet {PLANET_ID}")
        );
    }

    #[test]
    fn wait_success_and_cleanup() {
        let explorers_senders = Arc::new(Mutex::new(HashMap::new()));
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));

        // Mock explorers on the planet
        let explorers_location = Arc::new(Mutex::new(HashMap::from([
            (EXPLORER_ID_1, PLANET_ID),
            (EXPLORER_ID_2, PLANET_ID),
            (999, 888), // Explorer on a different planet (should be ignored)
        ])));

        let state = WaitingPlanetKillResult::new(
            PLANET_ID,
            explorers_location,
            explorers_senders,
            planet_senders,
        );
        let conv = Box::new(KillPlanetConversation::<WaitingPlanetKillResult>::new(
            CONV_ID, state,
        ));

        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::KillPlanetResult {
            planet_id: PLANET_ID,
        });

        // Act: Transition to KillExplorersManager
        let next_conv = conv
            .transition(Some(msg))
            .expect("Should transition to KillExplorersManager");

        // Assert: It is the manager and not an error
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn wait_wrong_message() {
        let explorers_location = Arc::new(Mutex::new(HashMap::new()));
        let explorers_senders = Arc::new(Mutex::new(HashMap::new()));
        let planet_senders = Arc::new(Mutex::new(HashMap::new()));

        let state = WaitingPlanetKillResult::new(
            PLANET_ID,
            explorers_location,
            explorers_senders,
            planet_senders,
        );
        let conv = Box::new(KillPlanetConversation::<WaitingPlanetKillResult>::new(
            CONV_ID, state,
        ));

        // Send wrong message type (e.g., AsteroidAck instead of KillPlanetResult)
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::AsteroidAck {
            planet_id: PLANET_ID,
            rocket: None,
        });

        let next_conv = conv
            .transition(Some(wrong_msg))
            .expect("Should return an ErrorState");

        // Assert: FSM explicitly returns ErrorState for WrongMessage in this implementation
        assert_eq!(
            next_conv.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }
}
