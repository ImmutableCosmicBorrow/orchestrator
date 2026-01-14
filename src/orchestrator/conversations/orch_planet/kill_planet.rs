use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::orch_explorer::kill_explorers_manager::KillExplorersManager;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, PossibleExpectedKinds, PossibleMessage,
    SendersToExplorer, SendersToPlanet, ToPlanetError, ToPlanetStruct,
};
use crate::orchestrator::{ExplorerBag, ExplorersLocationRef};
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
            println!("Killed Planet: {planet_id:?}");
            let to_kill_list = self.get_explorers_in_planet(planet_id);
            let next_state = KillExplorersManager::new(
                self.id,
                self.state.explorers_senders,
                self.state.planet_senders,
                false,
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
