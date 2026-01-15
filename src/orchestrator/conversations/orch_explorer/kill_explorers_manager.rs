use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::orch_explorer::kill_explorer::{
    KillExplorerConversation, SendingKillExplorer,
};
use crate::orchestrator::conversations::{
    Conversation, PossibleExpectedKinds, PossibleMessage, SendersToExplorer, SendersToPlanet,
    ToExplorerStruct, ToPlanetStruct,
};
use common_game::utils::ID;

///**Kill Explorers Manager**
///
/// This module acts as a higher-level manager to coordinate the termination of multiple explorers.
/// It functions by recursively popping explorers from a "to-kill" list and transitioning into
/// a [`KillExplorerConversation`] for each one.
///
/// Once a single explorer's termination sequence (and optional planet notification) is complete,
/// the flow returns to this manager to process the next explorer until the list is empty.

#[derive(Clone)]
pub(crate) struct KillExplorersManager {
    /// Unique ID for this management sequence
    id: ID,
    /// Senders used to communicate with the explorers in the list
    explorers_senders: SendersToExplorer,
    /// A stack of tuples containing (Explorer ID, Planet ID) to be processed
    explorers_to_kill: Vec<(ID, ID)>,
    /// Senders used to communicate with planets for outgoing notifications
    planet_senders: SendersToPlanet,
    /// Whether individual kill conversations should notify the planet of an outgoing explorer
    pub(crate) handle_outgoing: bool,
}

impl KillExplorersManager {
    /// Constructor for the [`KillExplorersManager`].
    ///
    /// # Parameters
    /// * `id` - The conversation ID for this manager.
    /// * `explorers_senders` - Communication channel for explorers.
    /// * `planet_senders` - Communication channel for planets.
    /// * `handle_outgoing` - If true, the generated kill conversations will notify planets.
    /// * `explorers_to_kill` - The initial list of targets.
    pub(crate) fn new(
        id: ID,
        explorers_senders: SendersToExplorer,
        planet_senders: SendersToPlanet,
        handle_outgoing: bool,
        explorers_to_kill: Vec<(ID, ID)>,
    ) -> Self {
        Self {
            id,
            explorers_senders,
            explorers_to_kill,
            planet_senders,
            handle_outgoing,
        }
    }
}

impl Conversation<ExplorerBag> for KillExplorersManager {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entity_id(&self) -> ID {
        self.id
    }

    /// The manager itself does not wait for a specific network message;
    /// it acts on internal state during the transition.
    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        None
    }

    /// Transition Function for [`KillExplorersManager`]:
    ///
    /// Returns:
    ///
    /// * [`KillExplorerConversation<SendingKillExplorer>`] - If there are still explorers in the `explorers_to_kill` list.
    /// * [None] - If the list is empty, effectively ending the mass-termination sequence.
    fn transition(
        mut self: Box<Self>,
        _msg_wrapped: Option<PossibleMessage<ExplorerBag>>,
    ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
        if let Some((explorer_id, planet_id)) = self.explorers_to_kill.pop() {
            let conv_id = self.id;
            let to_explorer_struct = ToExplorerStruct {
                explorer_id,
                explorers_senders: self.explorers_senders.clone(),
            };
            let to_planet_struct = ToPlanetStruct {
                planet_id,
                planets_senders: self.planet_senders.clone(),
            };

            // Create the specific kill conversation and hand over 'self' as the return-manager
            let state_struct = SendingKillExplorer::new(
                to_explorer_struct,
                to_planet_struct,
                self.handle_outgoing,
                self,
            );

            let next_state =
                KillExplorerConversation::<SendingKillExplorer>::new(conv_id, state_struct);

            return Some(Box::new(next_state));
        }
        None
    }

    fn get_priority(&self) -> i32 {
        5
    }
}
