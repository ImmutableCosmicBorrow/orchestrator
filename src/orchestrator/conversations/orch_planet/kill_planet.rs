use crate::logging_utils::log_internal;
use crate::orchestrator::conversations::PossibleExpectedKinds::PlanetToOrchKind;
use crate::orchestrator::conversations::{
    CommonErrorTypes, Conversation, ErrorState, KillExplorersList, PossibleExpectedKinds,
    PossibleMessage, SendersToExplorer, SendersToPlanet, ToPlanetError, ToPlanetStruct,
};
use crate::orchestrator::{ExplorerBagContent, ExplorersLocationRef};
use crate::payload;
use common_game::logging::Channel;
use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind::KillPlanetResult;
use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
use common_game::utils::ID;

///**Kill Planet Conversation**
///
/// This module manages the complex process of destroying a planet.
/// It uses an FSM to send the kill command, wait for confirmation, and then
/// sends via its method [`Conversation::get_kill_explorers_vec`] the IDs of the explorers on the planet
/// so that the Orchestrator can kill them
///
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
/// via [`Conversation::get_kill_explorers_vec`] closing the conversation.
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
impl Conversation<ExplorerBagContent> for KillPlanetConversation<SendPlanetKill> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (Some(self.state.to_planet_struct.planet_id), None)
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
        _msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
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
                Some(Box::new(error_state)
                    as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
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
impl Conversation<ExplorerBagContent> for KillPlanetConversation<WaitingPlanetKillResult> {
    fn get_id(&self) -> ID {
        self.id
    }

    fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
        (Some(self.state.planet_id), None)
    }

    fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
        self.expected_message.clone()
    }

    /// Transition Function for [`WaitingPlanetKillResult`] state:
    ///
    /// Returns:
    ///
    /// None to end the conversation if planet is killed correctly - NOTE: to kill explorers on this planet, we return the list of them
    /// through the dedicated method of the trait and let the Orchestrator take care of that
    ///
    /// [`ErrorState`] with [`CommonErrorTypes::WrongMessage`] if the trigger message is different then the expected [`PlanetToOrchestrator::KillPlanetResult`] .
    fn transition(
        self: Box<Self>,
        msg_wrapped: Option<PossibleMessage<ExplorerBagContent>>,
    ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
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

            return None;
        }

        //Wrong Message, close conversation
        let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
        Some(Box::new(error_state) as Box<dyn Conversation<ExplorerBagContent> + Send + Sync>)
    }

    fn get_priority(&self) -> i32 {
        5
    }

    fn get_kill_explorers_vec(&self) -> Option<(KillExplorersList, bool)> {
        //return the list of explorers to kill and a flag indicating that we don't need to advertise the death to planets (they're being killed)
        Some((self.get_explorers_in_planet(self.state.planet_id), false))
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

    type PlanetSenders = Arc<Mutex<HashMap<ID, crossbeam_channel::Sender<OrchestratorToPlanet>>>>;

    struct MakeSendersResult(
        PlanetSenders,
        crossbeam_channel::Receiver<OrchestratorToPlanet>,
    );

    // --- Helper functions ---
    fn make_senders_with(planet_id: ID) -> MakeSendersResult {
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();
        MakeSendersResult(Arc::new(Mutex::new(HashMap::from([(planet_id, tx)]))), rx)
    }

    fn make_empty_senders() -> PlanetSenders {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn make_to_planet_struct(planet_id: ID, senders: PlanetSenders) -> ToPlanetStruct {
        ToPlanetStruct {
            planet_id,
            planets_senders: senders,
        }
    }

    fn make_empty_explorer_refs() -> (ExplorersLocationRef, SendersToExplorer) {
        (
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashMap::new())),
        )
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_send_conv(senders: PlanetSenders) -> Box<KillPlanetConversation<SendPlanetKill>> {
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let (explorers_location, explorers_senders) = make_empty_explorer_refs();
        let state = SendPlanetKill::new(to_planet, explorers_location, explorers_senders);
        Box::new(KillPlanetConversation::<SendPlanetKill>::new(
            CONV_ID, state,
        ))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv() -> Box<KillPlanetConversation<WaitingPlanetKillResult>> {
        let (explorers_location, explorers_senders) = make_empty_explorer_refs();
        let planet_senders = make_empty_senders();
        let state = WaitingPlanetKillResult::new(
            PLANET_ID,
            explorers_location,
            explorers_senders,
            planet_senders,
        );
        Box::new(KillPlanetConversation::<WaitingPlanetKillResult>::new(
            CONV_ID, state,
        ))
    }

    #[allow(clippy::unnecessary_box_returns)]
    fn make_wait_conv_with_explorers() -> Box<KillPlanetConversation<WaitingPlanetKillResult>> {
        let explorers_location = Arc::new(Mutex::new(HashMap::from([
            (EXPLORER_ID_1, PLANET_ID),
            (EXPLORER_ID_2, PLANET_ID),
            (999, 888), // Explorer on a different planet (should be ignored)
        ])));
        let explorers_senders = Arc::new(Mutex::new(HashMap::new()));
        let planet_senders = make_empty_senders();
        let state = WaitingPlanetKillResult::new(
            PLANET_ID,
            explorers_location,
            explorers_senders,
            planet_senders,
        );
        Box::new(KillPlanetConversation::<WaitingPlanetKillResult>::new(
            CONV_ID, state,
        ))
    }

    // --- Tests ---

    #[test]
    fn send_success() {
        let MakeSendersResult(senders, _rx) = make_senders_with(PLANET_ID);
        let conv = make_send_conv(senders);
        let next_conv = conv
            .transition(None)
            .expect("Should transition to WaitingPlanetKillResult");
        assert_eq!(
            next_conv.get_expected_kind(),
            Some(PlanetToOrchKind(KillPlanetResult))
        );
        assert_eq!(next_conv.get_id(), CONV_ID);
        assert_eq!(next_conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert!(next_conv.get_error_details().is_none());
    }

    #[test]
    fn send_missing_sender() {
        let senders = make_empty_senders();
        let conv = make_send_conv(senders);
        let next_conv = conv
            .transition(None)
            .expect("Should transition to ErrorState");
        assert!(next_conv.get_error_details().is_some());
        assert_eq!(
            next_conv.get_error_details().unwrap(),
            format!("sender to planet {PLANET_ID} not found")
        );
    }

    #[test]
    fn send_message_failure() {
        let (tx, rx) = unbounded::<OrchestratorToPlanet>();
        drop(rx);
        let senders = Arc::new(Mutex::new(HashMap::from([(PLANET_ID, tx)])));
        let conv = make_send_conv(senders);
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
    fn send_getters() {
        let MakeSendersResult(senders, _rx) = make_senders_with(PLANET_ID);
        let to_planet = make_to_planet_struct(PLANET_ID, senders);
        let (explorers_location, explorers_senders) = make_empty_explorer_refs();
        let state = SendPlanetKill::new(to_planet, explorers_location, explorers_senders);
        let conv = KillPlanetConversation::<SendPlanetKill>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(conv.get_expected_kind(), None);
        assert_eq!(conv.get_priority(), 5);
    }

    #[test]
    fn wait_success_and_cleanup() {
        let conv = make_wait_conv_with_explorers();
        let msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::KillPlanetResult {
            planet_id: PLANET_ID,
        });
        let next_conv = conv.transition(Some(msg));
        // After planet kill, the conversation should end (return None)
        assert!(
            next_conv.is_none(),
            "Conversation should end and return None"
        );
    }

    #[test]
    fn wait_wrong_message() {
        let conv = make_wait_conv();
        let wrong_msg = PossibleMessage::PlanetToOrch(PlanetToOrchestrator::AsteroidAck {
            planet_id: PLANET_ID,
            rocket: None,
        });
        let next_conv = conv
            .transition(Some(wrong_msg))
            .expect("Should return an ErrorState");
        assert_eq!(
            next_conv.get_error_details(),
            Some("Wrong Message Received".to_string())
        );
    }

    #[test]
    fn wait_getters() {
        let (explorers_location, explorers_senders) = make_empty_explorer_refs();
        let planet_senders = make_empty_senders();
        let state = WaitingPlanetKillResult::new(
            PLANET_ID,
            explorers_location,
            explorers_senders,
            planet_senders,
        );
        let conv = KillPlanetConversation::<WaitingPlanetKillResult>::new(CONV_ID, state);
        assert_eq!(conv.get_id(), CONV_ID);
        assert_eq!(conv.get_entities_ids(), (Some(PLANET_ID), None));
        assert_eq!(
            conv.get_expected_kind(),
            Some(PlanetToOrchKind(KillPlanetResult))
        );
        assert_eq!(conv.get_priority(), 5);
    }
}
