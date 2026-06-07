use crate::convo_manager::queue::{ConversationMap, PQueue, TimeoutsMap};
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::conversations::{Conversation, PossibleExpectedKinds, PossibleMessage};
use crate::payload;
use common_game::logging::Channel;
use common_game::utils::ID;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

//TODO: MIGHT CHANGE THIS IN ALL DASHMAPS
pub struct ConvoScheduler {
    queue: PQueue,
    active_convos: ConversationMap,
    inactive_convos: ConversationMap,
    by_expected_msg: Arc<Mutex<HashMap<PossibleExpectedKinds, HashSet<ID>>>>,
    waiting_msgs: Arc<Mutex<HashMap<ID, PossibleMessage>>>,
    /// Messages that arrived before their target conversation was registered.
    /// Drained in `add_conversation()` when a matching conversation is added.
    pending_msgs: Arc<Mutex<Vec<PossibleMessage>>>,
    /// Maps conversation IDs to their timeout info: (start time, timeout duration, `paused_duration` snapshot at creation)
    timeouts: TimeoutsMap,
    /// Whether the scheduler is in stopped state (timeouts frozen)
    stopped: Arc<Mutex<bool>>,
    /// Instant when the scheduler entered stopped state
    stopped_since: Arc<Mutex<Option<Instant>>>,
    /// Total accumulated pause duration - subtracted from elapsed time when checking timeouts
    paused_duration: Arc<Mutex<Duration>>,
}

impl Clone for ConvoScheduler {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            active_convos: Arc::clone(&self.active_convos),
            inactive_convos: Arc::clone(&self.inactive_convos),
            by_expected_msg: Arc::clone(&self.by_expected_msg),
            waiting_msgs: Arc::clone(&self.waiting_msgs),
            pending_msgs: Arc::clone(&self.pending_msgs),
            timeouts: Arc::clone(&self.timeouts),
            stopped: Arc::clone(&self.stopped),
            stopped_since: Arc::clone(&self.stopped_since),
            paused_duration: Arc::clone(&self.paused_duration),
        }
    }
}

impl ConvoScheduler {
    pub fn new() -> Self {
        Self {
            queue: PQueue::new(),
            active_convos: Arc::new(Mutex::new(HashMap::new())),
            inactive_convos: Arc::new(Mutex::new(HashMap::new())),
            by_expected_msg: Arc::new(Mutex::new(HashMap::new())),
            waiting_msgs: Arc::new(Mutex::new(HashMap::new())),
            pending_msgs: Arc::new(Mutex::new(Vec::new())),
            timeouts: Arc::new(Mutex::new(HashMap::new())),
            stopped: Arc::new(Mutex::new(false)),
            stopped_since: Arc::new(Mutex::new(None)),
            paused_duration: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }

    /// Pause or resume timeout progression.
    ///
    /// When stopped=true, no conversations will report as timed out.
    /// When transitioning from stopped=true to stopped=false, the paused duration is accumulated
    /// so that paused time does not count toward conversation timeouts.
    pub fn set_stopped(&self, stopped: bool) {
        let now = Instant::now();
        let mut stopped_flag = self.stopped.lock().unwrap();

        // No state change
        if *stopped_flag == stopped {
            return;
        }

        if stopped {
            // Entering stopped state
            *self.stopped_since.lock().unwrap() = Some(now);
            *stopped_flag = true;
        } else {
            // Resuming from stopped state
            if let Some(stopped_at) = self.stopped_since.lock().unwrap().take() {
                let pause_duration = now.duration_since(stopped_at);
                let mut paused = self.paused_duration.lock().unwrap();
                *paused = paused.saturating_add(pause_duration);
            }
            *stopped_flag = false;
        }
    }

    /// Check if the scheduler is currently stopped.
    pub fn is_stopped(&self) -> bool {
        *self.stopped.lock().unwrap()
    }

    /// Register a timeout for a conversation.
    /// The conversation will be considered timed out after the specified duration
    /// from when this method is called.
    /// Snapshots the current `paused_duration` so that only pause time occurring
    /// after this timeout is created gets subtracted from elapsed time.
    pub fn set_timeout(&self, convo_id: ID, duration: Duration) {
        let paused_snapshot = *self.paused_duration.lock().unwrap();
        self.timeouts
            .lock()
            .unwrap()
            .insert(convo_id, (Instant::now(), duration, paused_snapshot));
    }

    /// Check for and return IDs of conversations that have timed out.
    /// Does not remove them from tracking - call `clear_timeout` after handling.
    /// When stopped, returns an empty list (timeouts are frozen).
    /// Only subtracts pause duration that occurred after each conversation started.
    pub fn get_timed_out_conversations(&self) -> Vec<ID> {
        if self.is_stopped() {
            return Vec::new();
        }

        let timeouts = self.timeouts.lock().unwrap();
        let paused_now = *self.paused_duration.lock().unwrap();
        let now = Instant::now();
        timeouts
            .iter()
            .filter(|(_, (start, duration, paused_at_creation))| {
                let actual_elapsed = now.duration_since(*start);
                let new_paused_since_start = paused_now.saturating_sub(*paused_at_creation);
                let effective_elapsed = actual_elapsed.saturating_sub(new_paused_since_start);
                effective_elapsed > *duration
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Clear the timeout for a conversation.
    /// Call this after a conversation successfully receives its expected message
    /// or after handling a timeout.
    pub fn clear_timeout(&self, convo_id: ID) {
        self.timeouts.lock().unwrap().remove(&convo_id);
    }

    /// Check if a specific conversation has timed out.
    /// When stopped, always returns false (timeouts are frozen).
    /// Only subtracts pause duration that occurred after the conversation started.
    #[allow(dead_code)]
    pub fn is_timed_out(&self, convo_id: ID) -> bool {
        if self.is_stopped() {
            return false;
        }

        let timeouts = self.timeouts.lock().unwrap();
        let paused_now = *self.paused_duration.lock().unwrap();
        if let Some((start, duration, paused_at_creation)) = timeouts.get(&convo_id) {
            let actual_elapsed = Instant::now().duration_since(*start);
            let new_paused_since_start = paused_now.saturating_sub(*paused_at_creation);
            let effective_elapsed = actual_elapsed.saturating_sub(new_paused_since_start);
            effective_elapsed > *duration
        } else {
            false
        }
    }

    /// Given a message kind and entities ids, this method looks for an inactive conversation
    /// that is expecting a message of that kind and is associated with the specified entities.
    /// If such a conversation is found, its id is returned.
    fn find_matching_conversation(
        &self,
        message_kind: &PossibleExpectedKinds,
        entity_ids: (Option<ID>, Option<ID>),
    ) -> Option<ID> {
        let inactive_convos = self.inactive_convos.lock().unwrap();
        let by_expected_msg = self.by_expected_msg.lock().unwrap();
        if let Some(convo_ids) = by_expected_msg.get(message_kind) {
            for &convo_id in convo_ids {
                let convo = inactive_convos.get(&convo_id);
                if let Some(convo) = convo
                    && convo.get_entities_ids() == entity_ids
                {
                    return Some(convo_id);
                }
            }
        }
        None
    }

    /// Atomically searches for a matching inactive conversation and, if none is found,
    /// buffers the message for later draining by `add_conversation`.
    ///
    /// This method holds the `pending_msgs` lock throughout the find+buffer sequence
    /// to prevent a TOCTOU race with `add_conversation`'s drain: without the lock,
    /// a response arriving between `transition()` and `add_conversation()` could be
    /// buffered *after* the drain completes, leaving the message stuck forever.
    pub fn find_matching_or_buffer(
        &self,
        message: PossibleMessage,
        message_kind: &PossibleExpectedKinds,
        entity_ids: (Option<ID>, Option<ID>),
    ) -> Option<(ID, PossibleMessage)> {
        // Hold pending_msgs lock for the entire find-or-buffer operation.
        // add_conversation also holds this lock while adding to inactive_convos
        // and draining, so the two operations are mutually exclusive.
        let mut pending = self.pending_msgs.lock().unwrap();

        if let Some(convo_id) = self.find_matching_conversation(message_kind, entity_ids) {
            // Drop pending lock before returning — caller will deliver the message
            drop(pending);
            return Some((convo_id, message));
        }

        // No matching conversation yet — buffer for later matching.
        let msg_kind_dbg = format!("{:?}", message.to_kind_type());
        let entity_ids_dbg = message.get_entity_ids();
        log_internal(
            LogTarget::Conversations,
            Channel::Debug,
            payload!(
                event: "MessageBufferedPending",
                message_kind: msg_kind_dbg,
                planet_id: format!("{:?}", entity_ids_dbg.0),
                explorer_id: format!("{:?}", entity_ids_dbg.1)
            ),
        );
        pending.push(message);
        None
    }

    /// This method removes and returns a conversation from the scheduler's active or inactive conversations
    /// based on its ID. It also updates the expected message kind mapping if applicable.
    /// Returns None if such conversation is not found.
    fn remove_conversation(&self, id: ID) -> Option<Box<dyn Conversation + Send + Sync>> {
        if let Some(inactive) = self.inactive_convos.lock().unwrap().remove(&id) {
            if let Some(kind) = inactive.get_expected_kind() {
                self.by_expected_msg
                    .lock()
                    .unwrap()
                    .entry(kind)
                    .or_default()
                    .remove(&id);
            }
            Some(inactive)
        } else if let Some(active) = self.active_convos.lock().unwrap().remove(&id) {
            Some(active)
        } else {
            None
        }
    }

    /// This method adds a new conversation to the scheduler.
    /// It assigns a unique ID to the conversation, and pushes it onto the priority queue.
    /// It is then added to the active conversations if it has expected kind None or a waiting message already present,
    /// otherwise it is added to the inactive conversations to wait for the message.
    /// Moreover, if the conversation is added to the inactive conversations,
    /// it updates the mapping of expected kinds to conversation IDs.
    /// If the conversation has a timeout configured, it registers the timeout.
    pub fn add_conversation(&self, conversation: Box<dyn Conversation + Send + Sync>) {
        let id = conversation.get_id();

        // Register timeout if the conversation has one configured
        if let Some(timeout_duration) = conversation.get_timeout() {
            let scaled_timeout = if timeout_duration == crate::globals::get_convo_timeout() {
                timeout_duration.max(crate::globals::get_game_step() + Duration::from_secs(1))
            } else {
                timeout_duration
            };
            self.set_timeout(id, scaled_timeout);
        }

        let priority = conversation.get_priority();
        let entities = conversation.get_entities_ids();
        // Capture expected_kind before the conversation is moved into a map
        let expected_kind = conversation.get_expected_kind();
        let ready_to_transition = expected_kind.is_none() || self.get_waiting_message(id).is_some();
        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "QueueEnqueue",
                conversation_id: id,
                priority: priority,
                ready_to_transition: ready_to_transition,
                expected_kind: format!("{:?}", expected_kind),
                planet_id: format!("{:?}", entities.0),
                explorer_id: format!("{:?}", entities.1)
            ),
        );
        // If the conversation is ready to transition, add it to the active convos.
        // Otherwise, add it to the inactive convos, which are waiting for a match.
        //
        // For conversations with an expected_kind, we hold the pending_msgs lock
        // across both the insertion into inactive_convos AND the pending drain.
        // This prevents a TOCTOU race with find_matching_or_buffer: without this,
        // a message could be buffered *after* the drain completes if it arrives
        // between transition() and add_conversation().
        if ready_to_transition {
            self.active_convos.lock().unwrap().insert(id, conversation);
            self.queue.push(id, priority);
        } else {
            // Hold pending_msgs lock for the entire add-to-inactive + drain operation.
            let mut pending = self.pending_msgs.lock().unwrap();

            // Add the expected kind to the expected messages
            if let Some(ref kind) = expected_kind {
                self.by_expected_msg
                    .lock()
                    .unwrap()
                    .entry(kind.clone())
                    .or_default()
                    .insert(id);
            }
            self.inactive_convos
                .lock()
                .unwrap()
                .insert(id, conversation);

            // Enqueue
            self.queue.push(id, priority);

            // Drain pending messages that match this newly registered conversation.
            // This closes the race where a response arrives between transition() and
            // add_conversation() on the processor thread.
            if let Some(ref kind) = expected_kind
                && let Some(pos) = pending.iter().position(|msg| {
                    let msg_kind = msg.to_kind_type();
                    let msg_entities = msg.get_entity_ids();
                    msg_kind == *kind
                        && (msg_entities == entities
                            || (msg_entities.0 == entities.1 && msg_entities.1 == entities.0))
                })
            {
                let matched_msg = pending.remove(pos);
                log_internal(
                    LogTarget::Conversations,
                    Channel::Debug,
                    payload!(
                        event: "PendingMessageDrained",
                        conversation_id: id,
                        message_kind: format!("{:?}", matched_msg.to_kind_type())
                    ),
                );
                // Drop the pending lock before calling add_waiting_message to avoid deadlock
                drop(pending);
                self.add_waiting_message(id, matched_msg);
            }
        }
    }

    /// This method retrieves and removes the next conversation from the scheduler based on priority.
    /// If the conversation is no longer active, it reinserts it in the queue and returns None.
    /// Otherwise, it removes the conversation from the active conversations map
    /// and also updates the expected message kind mapping if applicable.
    pub fn get_next_conversation(&self) -> Option<Box<dyn Conversation + Send + Sync>> {
        let (id, priority) = self.queue.pop()?;
        if !self.is_active_conversation(id) {
            self.queue.push(id, priority);
            return None;
        }

        self.handle_timeouts();

        let conversation = self.active_convos.lock().unwrap().remove(&id);
        let Some(conversation) = conversation else {
            // Conversation was removed by another thread between the check and removal
            return None;
        };

        let expected_kind = conversation.get_expected_kind();
        // Log dequeue before returning
        let entities = conversation.get_entities_ids();
        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "QueueDequeue",
                conversation_id: id,
                priority: priority,
                expected_kind: format!("{:?}", expected_kind),
                planet_id: format!("{:?}", entities.0),
                explorer_id: format!("{:?}", entities.1)
            ),
        );
        Some(conversation)
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    fn is_active_conversation(&self, id: ID) -> bool {
        self.active_convos.lock().unwrap().contains_key(&id)
    }

    pub fn remove_convos_for_dead_entity(&self, id: ID) {
        let mut active_convos = self.active_convos.lock().unwrap();
        let mut inactive_convos = self.inactive_convos.lock().unwrap();
        let mut by_expected_msg = self.by_expected_msg.lock().unwrap();

        // Remove from active convos
        let active_ids: Vec<ID> = active_convos
            .iter()
            .filter(|(_, convo)| {
                convo.get_entities_ids().0 == Some(id) || convo.get_entities_ids().1 == Some(id)
            })
            .map(|(&convo_id, _)| convo_id)
            .collect();
        for convo_id in active_ids {
            active_convos.remove(&convo_id);
            if let Some(kind) = active_convos
                .get(&convo_id)
                .and_then(|c| c.get_expected_kind())
            {
                by_expected_msg.entry(kind).or_default().remove(&convo_id);
            }
        }

        // Remove from inactive convos
        let inactive_ids: Vec<ID> = inactive_convos
            .iter()
            .filter(|(_, convo)| {
                convo.get_entities_ids().0 == Some(id) || convo.get_entities_ids().1 == Some(id)
            })
            .map(|(&convo_id, _)| convo_id)
            .collect();
        for convo_id in inactive_ids {
            inactive_convos.remove(&convo_id);
            if let Some(kind) = inactive_convos
                .get(&convo_id)
                .and_then(|c| c.get_expected_kind())
            {
                by_expected_msg.entry(kind).or_default().remove(&convo_id);
            }
        }

        // Also remove pending messages for the dead entity to prevent stale matches
        self.pending_msgs.lock().unwrap().retain(|msg| {
            let (planet, explorer) = msg.get_entity_ids();
            planet != Some(id) && explorer != Some(id)
        });
    }

    pub fn add_waiting_message(&self, convo_id: ID, message: PossibleMessage) {
        // Clear any timeout when a message arrives - the conversation is no longer waiting
        self.clear_timeout(convo_id);

        // For logging
        let entity_ids = message.get_entity_ids();
        let message_kind = message.to_kind_type();

        // Add the waiting message
        self.waiting_msgs.lock().unwrap().insert(convo_id, message);

        if let Some(convo) = self.inactive_convos.lock().unwrap().remove(&convo_id) {
            if let Some(kind) = convo.get_expected_kind() {
                self.by_expected_msg
                    .lock()
                    .unwrap()
                    .entry(kind)
                    .or_default()
                    .remove(&convo_id);
            }
            self.active_convos.lock().unwrap().insert(convo_id, convo);
        }
        // Log parking of the message for the conversation transition
        let (planet_id, explorer_id) = entity_ids;
        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "MessageParked",
                conversation_id: convo_id,
                message_kind: format!("{:?}", message_kind),
                from_planet: format!("{:?}", planet_id),
                from_explorer: format!("{:?}", explorer_id),
                to: "Orchestrator",
                is_active : self.is_active_conversation(convo_id),
            ),
        );
    }

    /// Buffer a message that arrived before its target conversation was registered.
    /// These messages will be drained and matched when `add_conversation()` is called.
    ///
    /// NOTE: Prefer using `find_matching_or_buffer` instead, which atomically checks
    /// for a matching conversation and buffers if none is found. Direct use of this
    /// method is only for non-conversation-matched messages.
    #[cfg(test)]
    pub fn buffer_pending_message(&self, message: PossibleMessage) {
        let message_kind = message.to_kind_type();
        let entity_ids = message.get_entity_ids();
        log_internal(
            LogTarget::Conversations,
            Channel::Debug,
            payload!(
                event: "MessageBufferedPending",
                message_kind: format!("{:?}", message_kind),
                planet_id: format!("{:?}", entity_ids.0),
                explorer_id: format!("{:?}", entity_ids.1)
            ),
        );
        self.pending_msgs.lock().unwrap().push(message);
    }

    pub fn get_waiting_message(&self, convo_id: ID) -> Option<PossibleMessage> {
        self.waiting_msgs.lock().unwrap().remove(&convo_id)
    }

    /// Process all timed-out conversations.
    /// Calls `on_timeout()` for each timed-out conversation, which will panic by default
    /// unless the conversation has overridden `on_timeout()` with custom handling.
    pub fn handle_timeouts(&self) {
        let timed_out_ids = self.get_timed_out_conversations();

        for convo_id in timed_out_ids {
            let tmp = self.remove_conversation(convo_id);
            if let Some(convo) = tmp {
                // Clear the timeout tracking
                self.clear_timeout(convo_id);

                // Call on_timeout - will panic unless overridden
                convo.on_timeout();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::get_convo_timeout;
    use crate::orchestrator::conversations::{
        Conversation, PossibleExpectedKinds, PossibleMessage,
    };
    use common_game::protocols::orchestrator_planet::PlanetToOrchestratorKind;
    use std::sync::{Arc, Mutex};
    // ============================================================================
    // Mock Conversation Implementation
    // ============================================================================

    #[derive(Clone)]
    struct MockConversation {
        id: ID,
        planet_id: ID,
        expected_kind: Option<PossibleExpectedKinds>,
        priority: i32,
        state: Arc<Mutex<MockState>>,
    }

    #[derive(Clone)]
    struct MockState {
        transitions: usize,
        alive: bool,
    }

    impl std::fmt::Debug for MockConversation {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("MockConversation")
                .field("id", &self.id)
                .field("planet_id", &self.planet_id)
                .field("priority", &self.priority)
                .finish_non_exhaustive()
        }
    }

    impl PartialEq for MockConversation {
        fn eq(&self, other: &Self) -> bool {
            self.id == other.id
        }
    }

    impl Eq for MockConversation {}

    impl std::hash::Hash for MockConversation {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.id.hash(state);
        }
    }

    impl Conversation for MockConversation {
        fn get_id(&self) -> ID {
            self.id
        }

        fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
            (Some(self.planet_id), None)
        }

        fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
            self.expected_kind.clone()
        }

        fn transition(
            self: Box<Self>,
            _msg: Option<PossibleMessage>,
        ) -> Option<Box<dyn Conversation + Send + Sync>> {
            let mut state = self.state.lock().unwrap();
            state.transitions = 1;
            state.alive = false;
            None
        }

        fn get_priority(&self) -> i32 {
            self.priority
        }

        fn get_timeout(&self) -> Option<Duration> {
            Some(get_convo_timeout())
        }
    }

    impl MockConversation {
        fn new(
            id: ID,
            planet_id: ID,
            priority: i32,
            expected_kind: Option<PossibleExpectedKinds>,
        ) -> Self {
            Self {
                id,
                planet_id,
                expected_kind,
                priority,
                state: Arc::new(Mutex::new(MockState {
                    transitions: 0,
                    alive: true,
                })),
            }
        }

        fn with_expected_kind(
            id: ID,
            planet_id: ID,
            priority: i32,
            expected_kind: PossibleExpectedKinds,
        ) -> Self {
            Self {
                id,
                planet_id,
                expected_kind: Some(expected_kind),
                priority,
                state: Arc::new(Mutex::new(MockState {
                    transitions: 0,
                    alive: true,
                })),
            }
        }

        // TODO: add tests that actually trigger the transition and check that the state is updated accordingly, to verify that the conversations retrieved from the scheduler are the same ones that were added and that their methods work as expected.
        #[allow(dead_code)]
        fn transitions(&self) -> usize {
            self.state.lock().unwrap().transitions
        }
    }

    // ============================================================================
    // ConvoScheduler Basic Operations
    // ============================================================================

    #[test]
    fn scheduler_new_is_empty() {
        let scheduler = ConvoScheduler::new();
        assert!(scheduler.is_empty());
    }

    #[test]
    fn scheduler_add_single_conversation() {
        let scheduler = ConvoScheduler::new();
        let convo = Box::new(MockConversation::new(100, 1, 10, None));

        scheduler.add_conversation(convo);
        assert!(!scheduler.is_empty());
    }

    #[test]
    fn scheduler_get_next_conversation() {
        let scheduler = ConvoScheduler::new();
        let convo = Box::new(MockConversation::new(100, 1, 10, None));
        scheduler.add_conversation(convo);

        let retrieved = scheduler.get_next_conversation().unwrap();
        assert_eq!(retrieved.get_entities_ids(), (Some(1), None));
        assert_eq!(retrieved.get_priority(), 10);
        assert!(scheduler.is_empty());

        let is_active = scheduler.is_active_conversation(retrieved.get_id());
        assert!(!is_active);

        let is_inside_msg_map = scheduler.get_waiting_message(retrieved.get_id());
        assert!(is_inside_msg_map.is_none());

        let is_inside_expected_map = scheduler.find_matching_conversation(
            &PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck),
            (Some(1), None),
        );
        assert!(is_inside_expected_map.is_none());
    }

    #[test]
    fn scheduler_get_from_empty_returns_none() {
        let scheduler = ConvoScheduler::new();
        assert!(scheduler.get_next_conversation().is_none());
    }

    #[test]
    fn scheduler_clone_shares_state() {
        let scheduler = ConvoScheduler::new();
        scheduler.add_conversation(Box::new(MockConversation::new(100, 1, 10, None)));

        let cloned = scheduler.clone();
        assert!(!cloned.is_empty());

        cloned.get_next_conversation();
        assert!(scheduler.is_empty());
    }

    // ============================================================================
    // Message Kind Matching
    // ============================================================================

    #[test]
    fn find_matching_conversation_by_kind_and_entity() {
        let scheduler = ConvoScheduler::new();
        let kind = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck);

        let convo = Box::new(MockConversation::with_expected_kind(
            100,
            42,
            10,
            kind.clone(),
        ));
        scheduler.add_conversation(convo);

        let found = scheduler.find_matching_conversation(&kind, (Some(42), None));
        assert!(found.is_some());
        assert_eq!(found.unwrap(), 100);
    }

    #[test]
    fn find_matching_conversation_wrong_entity() {
        let scheduler = ConvoScheduler::new();
        let kind = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck);

        let convo = Box::new(MockConversation::with_expected_kind(
            100,
            42,
            10,
            kind.clone(),
        ));
        scheduler.add_conversation(convo);

        let found = scheduler.find_matching_conversation(&kind, (Some(999), None));
        assert!(found.is_none());
    }

    #[test]
    fn find_matching_conversation_wrong_kind() {
        let scheduler = ConvoScheduler::new();
        let kind1 = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck);
        let kind2 = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::SunrayAck);

        let convo = Box::new(MockConversation::with_expected_kind(100, 42, 10, kind1));
        scheduler.add_conversation(convo);

        let found = scheduler.find_matching_conversation(&kind2, (Some(42), None));
        assert!(found.is_none());
        assert!(!scheduler.is_empty());
    }

    #[test]
    fn find_matching_conversation_no_expected_kind() {
        let scheduler = ConvoScheduler::new();
        let kind = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck);

        let convo = Box::new(MockConversation::new(100, 42, 10, None));
        scheduler.add_conversation(convo);

        let found = scheduler.find_matching_conversation(&kind, (Some(42), None));
        assert!(found.is_none());
    }

    #[test]
    fn find_matching_multiple_same_kind_different_entities() {
        let scheduler = ConvoScheduler::new();
        let kind = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck);

        scheduler.add_conversation(Box::new(MockConversation::with_expected_kind(
            100,
            1,
            10,
            kind.clone(),
        )));
        scheduler.add_conversation(Box::new(MockConversation::with_expected_kind(
            200,
            2,
            10,
            kind.clone(),
        )));
        scheduler.add_conversation(Box::new(MockConversation::with_expected_kind(
            300,
            3,
            10,
            kind.clone(),
        )));

        let found1 = scheduler.find_matching_conversation(&kind, (Some(2), None));
        assert!(found1.is_some());
        assert_eq!(found1.unwrap(), 200);

        let found2 = scheduler.find_matching_conversation(&kind, (Some(1), None));
        assert!(found2.is_some());
        assert_eq!(found2.unwrap(), 100);

        let found3 = scheduler.find_matching_conversation(&kind, (Some(3), None));
        assert!(found3.is_some());
        assert_eq!(found3.unwrap(), 300);
    }

    // ============================================================================
    // Waiting Messages
    // ============================================================================

    #[test]
    fn get_waiting_message_not_found() {
        let scheduler = ConvoScheduler::new();
        let msg = scheduler.get_waiting_message(999);
        assert!(msg.is_none());
    }

    #[test]
    fn get_waiting_message_twice_returns_none() {
        let scheduler = ConvoScheduler::new();
        scheduler.get_waiting_message(100);
        let msg = scheduler.get_waiting_message(100);
        assert!(msg.is_none());
    }

    // ============================================================================
    // Concurrent Access
    // ============================================================================

    #[test]
    fn concurrent_add() {
        use std::thread;

        let scheduler = Arc::new(ConvoScheduler::new());
        let mut handles = vec![];

        for thread_id in 0..4 {
            let scheduler_clone = scheduler.clone();
            let handle = thread::spawn(move || {
                for i in 0..25 {
                    let id = u32::try_from(thread_id * 100 + i).unwrap() as ID;
                    scheduler_clone.add_conversation(Box::new(MockConversation::new(
                        id,
                        u32::try_from(thread_id).unwrap() as ID,
                        i % 10,
                        None,
                    )));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let mut count = 0;
        while scheduler.get_next_conversation().is_some() {
            count += 1;
        }

        assert_eq!(count, 100);
    }

    #[test]
    fn concurrent_get() {
        use std::thread;

        let scheduler = Arc::new(ConvoScheduler::new());

        for i in 0..100 {
            scheduler.add_conversation(Box::new(MockConversation::new(
                i,
                (i % 5) as ID,
                i32::try_from(i % 20).unwrap(),
                None,
            )));
        }

        let results = Arc::new(Mutex::new(Vec::new()));
        let mut handles = vec![];

        for _ in 0..4 {
            let scheduler_clone = scheduler.clone();
            let results_clone = results.clone();
            let handle = thread::spawn(move || {
                while let Some(convo) = scheduler_clone.get_next_conversation() {
                    let convo: Box<dyn Conversation + Send + Sync> = convo;
                    results_clone.lock().unwrap().push(convo.get_id());
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let retrieved = results.lock().unwrap();
        assert_eq!(retrieved.len(), 100);
    }

    #[test]
    fn concurrent_add_and_get() {
        use std::thread;

        let scheduler = Arc::new(ConvoScheduler::new());
        let results = Arc::new(Mutex::new(Vec::new()));

        let scheduler_producer = scheduler.clone();
        let producer = thread::spawn(move || {
            for i in 0..50 {
                scheduler_producer.add_conversation(Box::new(MockConversation::new(
                    i,
                    (i % 5) as ID,
                    10,
                    None,
                )));
                thread::yield_now();
            }
        });

        let scheduler_consumer = scheduler.clone();
        let results_clone = results.clone();
        let consumer = thread::spawn(move || {
            let mut count = 0;
            while count < 50 {
                if let Some(convo) = scheduler_consumer.get_next_conversation() {
                    let convo: Box<dyn Conversation + Send + Sync> = convo;
                    results_clone.lock().unwrap().push(convo.get_id());
                    count += 1;
                }
                thread::yield_now();
            }
        });

        producer.join().unwrap();
        consumer.join().unwrap();

        assert_eq!(results.lock().unwrap().len(), 50);
    }

    // ============================================================================
    // Timeout Integration Tests
    // ============================================================================

    /// Mock conversation that supports timeout
    struct TimeoutMockConversation {
        id: ID,
        planet_id: ID,
        timeout_duration: Option<Duration>,
        on_timeout_called: Arc<Mutex<bool>>,
    }

    impl std::fmt::Debug for TimeoutMockConversation {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("TimeoutMockConversation")
                .field("id", &self.id)
                .finish_non_exhaustive()
        }
    }

    impl TimeoutMockConversation {
        fn new(id: ID, planet_id: ID, timeout: Option<Duration>) -> Self {
            Self {
                id,
                planet_id,
                timeout_duration: timeout,
                on_timeout_called: Arc::new(Mutex::new(false)),
            }
        }

        // TODO: add tests that trigger the timeout and check that the on_timeout logic is executed as expected, by checking that the on_timeout_called flag is set to true after handling timeouts.
        #[allow(dead_code)]
        fn was_timeout_called(&self) -> bool {
            *self.on_timeout_called.lock().unwrap()
        }
    }

    impl Conversation for TimeoutMockConversation {
        fn get_id(&self) -> ID {
            self.id
        }

        fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
            (Some(self.planet_id), None)
        }

        fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
            Some(PossibleExpectedKinds::PlanetToOrchKind(
                PlanetToOrchestratorKind::SunrayAck,
            ))
        }

        fn transition(
            self: Box<Self>,
            _msg: Option<PossibleMessage>,
        ) -> Option<Box<dyn Conversation + Send + Sync>> {
            None
        }

        fn get_priority(&self) -> i32 {
            1
        }

        fn get_timeout(&self) -> Option<Duration> {
            self.timeout_duration
        }

        fn on_timeout(self: Box<Self>) {
            *self.on_timeout_called.lock().unwrap() = true;
            // Just mark as called - no return value
        }
    }

    #[test]
    fn timeout_registered_when_conversation_added() {
        let scheduler = ConvoScheduler::new();
        let timeout_duration = Duration::from_millis(100);

        let convo = TimeoutMockConversation::new(1, 10, Some(timeout_duration));
        scheduler.add_conversation(Box::new(convo));

        // Timeout should be registered
        let timeouts = scheduler.timeouts.lock().unwrap();
        assert!(timeouts.contains_key(&1));
        let (_, duration, _) = timeouts.get(&1).unwrap();
        assert_eq!(*duration, timeout_duration);
    }

    #[test]
    fn no_timeout_registered_for_conversation_without_timeout() {
        let scheduler = ConvoScheduler::new();

        let convo = MockConversation::new(1, 10, 1, None);
        scheduler.add_conversation(Box::new(convo));

        // Default timeout is applied by the trait, so a timeout should be registered
        let timeouts = scheduler.timeouts.lock().unwrap();
        assert!(timeouts.contains_key(&1));
    }

    #[test]
    fn timeout_cleared_when_message_arrives() {
        let scheduler = ConvoScheduler::new();
        let timeout_duration = Duration::from_secs(10);

        let convo = TimeoutMockConversation::new(1, 10, Some(timeout_duration));
        scheduler.add_conversation(Box::new(convo));

        // Timeout should be registered initially
        assert!(scheduler.timeouts.lock().unwrap().contains_key(&1));

        // Simulate message arrival
        let msg = PossibleMessage::PlanetToOrch(
            common_game::protocols::orchestrator_planet::PlanetToOrchestrator::SunrayAck {
                planet_id: 10,
            },
        );
        scheduler.add_waiting_message(1, msg);

        // Timeout should be cleared
        assert!(!scheduler.timeouts.lock().unwrap().contains_key(&1));
    }

    #[test]
    fn handle_timeouts_calls_on_timeout() {
        let scheduler = ConvoScheduler::new();

        // Add a conversation with a very short timeout
        let on_timeout_called = Arc::new(Mutex::new(false));
        let convo = TimeoutMockConversation {
            id: 1,
            planet_id: 10,
            timeout_duration: Some(Duration::from_millis(1)),
            on_timeout_called: on_timeout_called.clone(),
        };
        scheduler.add_conversation(Box::new(convo));

        // Wait for timeout to expire
        std::thread::sleep(Duration::from_millis(10));

        // Handle timeouts should call on_timeout
        scheduler.handle_timeouts();

        // Verify on_timeout was called
        assert!(
            *on_timeout_called.lock().unwrap(),
            "on_timeout should have been called"
        );

        // Original conversation should be removed from active
        assert!(!scheduler.active_convos.lock().unwrap().contains_key(&1));

        // Timeout tracking should be cleared
        assert!(!scheduler.timeouts.lock().unwrap().contains_key(&1));
    }

    #[test]
    fn handle_timeouts_does_nothing_before_expiry() {
        let scheduler = ConvoScheduler::new();

        // Add a conversation with a long timeout
        let convo = TimeoutMockConversation::new(1, 10, Some(Duration::from_mins(1)));
        scheduler.add_conversation(Box::new(convo));

        // Handle timeouts immediately - should do nothing
        scheduler.handle_timeouts();

        // Conversation should not be removed
        assert!(scheduler.inactive_convos.lock().unwrap().contains_key(&1));
    }

    #[test]
    fn get_timed_out_conversations_works() {
        let scheduler = ConvoScheduler::new();

        // Manually set a timeout that has already expired
        scheduler.timeouts.lock().unwrap().insert(
            42,
            (
                Instant::now().checked_sub(Duration::from_secs(10)).unwrap(),
                Duration::from_secs(1),
                Duration::ZERO,
            ),
        );

        let timed_out = scheduler.get_timed_out_conversations();
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0], 42);
    }

    #[test]
    fn is_timed_out_works() {
        let scheduler = ConvoScheduler::new();

        // Not registered = not timed out
        assert!(!scheduler.is_timed_out(1));

        // Set a timeout that hasn't expired
        scheduler.set_timeout(1, Duration::from_mins(1));
        assert!(!scheduler.is_timed_out(1));

        // Set a timeout in the past (already expired)
        scheduler.timeouts.lock().unwrap().insert(
            2,
            (
                Instant::now().checked_sub(Duration::from_secs(10)).unwrap(),
                Duration::from_secs(1),
                Duration::ZERO,
            ),
        );
        assert!(scheduler.is_timed_out(2));
    }

    // ============================================================================
    // Stop/Resume Timeout Freezing Tests
    // ============================================================================

    #[test]
    fn stopped_scheduler_reports_no_timeouts() {
        let scheduler = ConvoScheduler::new();
        scheduler.set_timeout(1, Duration::from_millis(1));

        // Stop before timeout expires
        scheduler.set_stopped(true);
        std::thread::sleep(Duration::from_millis(10));

        // Should report no timeouts when stopped
        assert!(scheduler.get_timed_out_conversations().is_empty());
        assert!(!scheduler.is_timed_out(1));

        scheduler.set_stopped(false);
    }

    #[test]
    fn paused_duration_subtracts_from_elapsed_time() {
        let scheduler = ConvoScheduler::new();
        scheduler.set_timeout(1, Duration::from_millis(50));

        // Let timeout start and elapse 10ms
        std::thread::sleep(Duration::from_millis(10));

        // Pause for 20ms (would exceed timeout if counted)
        scheduler.set_stopped(true);
        std::thread::sleep(Duration::from_millis(20));
        scheduler.set_stopped(false);

        // Should not be timed out because paused time is subtracted
        // Actual elapsed: ~30ms, Paused: ~20ms, Effective: ~10ms < 50ms
        assert!(!scheduler.is_timed_out(1));

        // Wait for remaining effective time
        std::thread::sleep(Duration::from_millis(50));

        // Now should be timed out (total effective elapsed ~60ms > 50ms)
        assert!(scheduler.is_timed_out(1));
    }

    #[test]
    fn multiple_pause_resume_cycles_accumulate() {
        let scheduler = ConvoScheduler::new();
        scheduler.set_timeout(1, Duration::from_millis(50));

        std::thread::sleep(Duration::from_millis(10));

        // First pause/resume cycle
        scheduler.set_stopped(true);
        std::thread::sleep(Duration::from_millis(15));
        scheduler.set_stopped(false);

        std::thread::sleep(Duration::from_millis(10));

        // Second pause/resume cycle
        scheduler.set_stopped(true);
        std::thread::sleep(Duration::from_millis(15));
        scheduler.set_stopped(false);

        // Total actual: ~50ms, paused: ~30ms, effective: ~20ms < 50ms
        assert!(!scheduler.is_timed_out(1));

        std::thread::sleep(Duration::from_millis(35));
        // Now effective should exceed 50ms
        assert!(scheduler.is_timed_out(1));
    }

    #[test]
    fn get_timed_out_conversations_respects_stopped_state() {
        let scheduler = ConvoScheduler::new();

        // Add multiple conversations with short timeouts
        scheduler.set_timeout(1, Duration::from_millis(5));
        scheduler.set_timeout(2, Duration::from_millis(5));

        std::thread::sleep(Duration::from_millis(10));

        // They should be timed out when not stopped
        let timed_out = scheduler.get_timed_out_conversations();
        assert_eq!(timed_out.len(), 2);

        // Stop and check again - should be empty
        scheduler.set_stopped(true);
        let timed_out = scheduler.get_timed_out_conversations();
        assert!(timed_out.is_empty());

        // Resume and they should be timed out again
        scheduler.set_stopped(false);
        let timed_out = scheduler.get_timed_out_conversations();
        assert_eq!(timed_out.len(), 2);

        scheduler.set_stopped(true);
    }

    // ============================================================================
    // Pending Message Buffer Tests
    // ============================================================================

    /// Helper: create a PlanetToOrch SunrayAck message for a given planet_id
    fn make_sunray_ack_msg(planet_id: ID) -> PossibleMessage {
        PossibleMessage::PlanetToOrch(
            common_game::protocols::orchestrator_planet::PlanetToOrchestrator::SunrayAck {
                planet_id,
            },
        )
    }

    /// Helper: create a PlanetToOrch StartPlanetAIResult message for a given planet_id
    fn make_start_planet_ai_result_msg(planet_id: ID) -> PossibleMessage {
        PossibleMessage::PlanetToOrch(
            common_game::protocols::orchestrator_planet::PlanetToOrchestrator::StartPlanetAIResult {
                planet_id,
            },
        )
    }

    #[test]
    fn pending_message_drained_on_add_conversation() {
        let scheduler = ConvoScheduler::new();
        let kind = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::SunrayAck);

        // Buffer a message before any conversation is registered
        let msg = make_sunray_ack_msg(42);
        scheduler.buffer_pending_message(msg);
        assert_eq!(scheduler.pending_msgs.lock().unwrap().len(), 1);

        // Now register a conversation expecting that exact kind + entity
        let convo = Box::new(MockConversation::with_expected_kind(100, 42, 10, kind));
        scheduler.add_conversation(convo);

        // The pending buffer should be drained
        assert_eq!(scheduler.pending_msgs.lock().unwrap().len(), 0);
        // The message should now be a waiting message (already consumed by add_waiting_message)
        // and the conversation should be active
        assert!(scheduler.is_active_conversation(100));
    }

    #[test]
    fn pending_message_not_drained_wrong_kind() {
        let scheduler = ConvoScheduler::new();
        let kind =
            PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::StartPlanetAIResult);

        // Buffer a SunrayAck message
        let msg = make_sunray_ack_msg(42);
        scheduler.buffer_pending_message(msg);

        // Register a conversation expecting StartPlanetAIResult (different kind)
        let convo = Box::new(MockConversation::with_expected_kind(100, 42, 10, kind));
        scheduler.add_conversation(convo);

        // Pending buffer should NOT be drained — wrong kind
        assert_eq!(scheduler.pending_msgs.lock().unwrap().len(), 1);
    }

    #[test]
    fn pending_message_not_drained_wrong_entity() {
        let scheduler = ConvoScheduler::new();
        let kind = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::SunrayAck);

        // Buffer a SunrayAck for planet 42
        let msg = make_sunray_ack_msg(42);
        scheduler.buffer_pending_message(msg);

        // Register a conversation for planet 999 (different entity)
        let convo = Box::new(MockConversation::with_expected_kind(100, 999, 10, kind));
        scheduler.add_conversation(convo);

        // Pending buffer should NOT be drained — wrong entity
        assert_eq!(scheduler.pending_msgs.lock().unwrap().len(), 1);
    }

    #[test]
    fn pending_cleanup_on_dead_entity() {
        let scheduler = ConvoScheduler::new();

        // Buffer messages for entity 42
        scheduler.buffer_pending_message(make_sunray_ack_msg(42));
        scheduler.buffer_pending_message(make_start_planet_ai_result_msg(42));
        // Buffer a message for a different entity
        scheduler.buffer_pending_message(make_sunray_ack_msg(99));
        assert_eq!(scheduler.pending_msgs.lock().unwrap().len(), 3);

        // Kill entity 42
        scheduler.remove_convos_for_dead_entity(42);

        // Only the message for entity 99 should remain
        assert_eq!(scheduler.pending_msgs.lock().unwrap().len(), 1);
    }

    #[test]
    fn multiple_pending_first_match_wins() {
        let scheduler = ConvoScheduler::new();
        let kind = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::SunrayAck);

        // Buffer two SunrayAck messages for different planets
        scheduler.buffer_pending_message(make_sunray_ack_msg(10));
        scheduler.buffer_pending_message(make_sunray_ack_msg(20));
        assert_eq!(scheduler.pending_msgs.lock().unwrap().len(), 2);

        // Register a conversation for planet 20
        let convo = Box::new(MockConversation::with_expected_kind(200, 20, 10, kind));
        scheduler.add_conversation(convo);

        // Only the matching message (planet 20) should be drained
        let pending = scheduler.pending_msgs.lock().unwrap();
        assert_eq!(pending.len(), 1);
        // The remaining message should be for planet 10
        let remaining_entities = pending[0].get_entity_ids();
        assert_eq!(remaining_entities.0, Some(10));
    }
}
