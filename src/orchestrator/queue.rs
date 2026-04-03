use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::ExplorerBagContent;
use crate::orchestrator::conversations::Conversation;
use crate::orchestrator::conversations::PossibleExpectedKinds;
use crate::orchestrator::conversations::PossibleMessage;
use crate::payload;
use common_game::logging::Channel;
use common_game::utils::ID;
use priority_queue::PriorityQueue;
use std::collections::HashSet;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{collections::HashMap, fmt::Debug};
pub(crate) struct PQueue {
    queue: Arc<Mutex<PriorityQueue<ID, i32>>>,
}

impl PQueue {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(PriorityQueue::new())),
        }
    }

    pub fn push(&self, id: ID, priority: i32) {
        let mut queue = self.queue.lock().unwrap();
        queue.push(id, priority);
    }

    pub fn pop(&self) -> Option<(ID, i32)> {
        let mut queue = self.queue.lock().unwrap();
        queue.pop()
    }

    pub fn is_empty(&self) -> bool {
        let queue = self.queue.lock().unwrap();
        queue.is_empty()
    }
}

impl Clone for PQueue {
    fn clone(&self) -> Self {
        Self {
            queue: Arc::clone(&self.queue),
        }
    }
}

pub type ConversationMap<T> = Arc<Mutex<HashMap<ID, Box<dyn Conversation<T> + Send + Sync>>>>;

pub struct ConvoScheduler<T: Debug + Eq + Hash> {
    queue: PQueue,
    active_convos: ConversationMap<T>,
    inactive_convos: ConversationMap<T>,
    by_expected_msg: Arc<Mutex<HashMap<PossibleExpectedKinds, HashSet<ID>>>>,
    waiting_msgs: Arc<Mutex<HashMap<ID, PossibleMessage<ExplorerBagContent>>>>,
    /// Maps conversation IDs to their timeout info: (start time, timeout duration)
    timeouts: Arc<Mutex<HashMap<ID, (Instant, Duration)>>>,
}

impl<T: Debug + Eq + Hash> Clone for ConvoScheduler<T> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            active_convos: Arc::clone(&self.active_convos),
            inactive_convos: Arc::clone(&self.inactive_convos),
            by_expected_msg: Arc::clone(&self.by_expected_msg),
            waiting_msgs: Arc::clone(&self.waiting_msgs),
            timeouts: Arc::clone(&self.timeouts),
        }
    }
}

impl<T: Debug + Eq + Hash> ConvoScheduler<T> {
    pub fn new() -> Self {
        Self {
            queue: PQueue::new(),
            active_convos: Arc::new(Mutex::new(HashMap::new())),
            inactive_convos: Arc::new(Mutex::new(HashMap::new())),
            by_expected_msg: Arc::new(Mutex::new(HashMap::new())),
            waiting_msgs: Arc::new(Mutex::new(HashMap::new())),
            timeouts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a timeout for a conversation.
    /// The conversation will be considered timed out after the specified duration
    /// from when this method is called.
    pub fn set_timeout(&self, convo_id: ID, duration: Duration) {
        self.timeouts
            .lock()
            .unwrap()
            .insert(convo_id, (Instant::now(), duration));
    }

    /// Check for and return IDs of conversations that have timed out.
    /// Does not remove them from tracking - call `clear_timeout` after handling.
    pub fn get_timed_out_conversations(&self) -> Vec<ID> {
        let timeouts = self.timeouts.lock().unwrap();
        let now = Instant::now();
        timeouts
            .iter()
            .filter(|(_, (start, duration))| now.duration_since(*start) > *duration)
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
    pub fn is_timed_out(&self, convo_id: ID) -> bool {
        let timeouts = self.timeouts.lock().unwrap();
        if let Some((start, duration)) = timeouts.get(&convo_id) {
            Instant::now().duration_since(*start) > *duration
        } else {
            false
        }
    }

    /// Given a message kind and entities ids, this method looks for an inactive conversation
    /// that is expecting a message of that kind and is associated with the specified entities.
    /// If such a conversation is found, its id is returned.
    pub fn find_matching_conversation(
        &self,
        message_kind: &PossibleExpectedKinds,
        entity_ids: (Option<ID>, Option<ID>),
    ) -> Option<ID> {
        let by_expected_msg = self.by_expected_msg.lock().unwrap();
        if let Some(convo_ids) = by_expected_msg.get(message_kind) {
            for &convo_id in convo_ids {
                let inactive_convos = self.inactive_convos.lock().unwrap();
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

    /// This method removes and returns a conversation from the scheduler's active or inactive conversations
    /// based on its ID. It also updates the expected message kind mapping if applicable.
    /// Returns None if such conversation is not found.
    fn remove_conversation(&self, id: ID) -> Option<Box<dyn Conversation<T> + Send + Sync>> {
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
    pub fn add_conversation(&self, conversation: Box<dyn Conversation<T> + Send + Sync>) {
        let id = conversation.get_id();

        // Register timeout if the conversation has one configured
        if let Some(timeout_duration) = conversation.get_timeout() {
            self.set_timeout(id, timeout_duration);
        }

        let priority = conversation.get_priority();
        let entities = conversation.get_entities_ids();
        let ready_to_transition =
            conversation.get_expected_kind().is_none() || self.get_waiting_message(id).is_some();
        log_internal(
            LogTarget::Conversations,
            Channel::Trace,
            payload!(
                event: "QueueEnqueue",
                conversation_id: id,
                priority: priority,
                ready_to_transition: ready_to_transition,
                expected_kind: format!("{:?}", conversation.get_expected_kind()),
                planet_id: format!("{:?}", entities.0),
                explorer_id: format!("{:?}", entities.1)
            ),
        );
        // If the conversation is ready to transition, add it to the active convos.
        // Otherwise, add it to the inactive convos, which are waiting for a match.
        if ready_to_transition {
            self.active_convos.lock().unwrap().insert(id, conversation);
        } else {
            // Add the expected kind to the expected messages
            if let Some(kind) = conversation.get_expected_kind() {
                self.by_expected_msg
                    .lock()
                    .unwrap()
                    .entry(kind)
                    .or_default()
                    .insert(id);
            }
            self.inactive_convos
                .lock()
                .unwrap()
                .insert(id, conversation);
        }

        // Enqueue and log
        self.queue.push(id, priority);
    }

    /// This method retrieves and removes the next conversation from the scheduler based on priority.
    /// If the conversation is no longer active, it reinserts it in the queue and returns None.
    /// Otherwise, it removes the conversation from the active conversations map
    /// and also updates the expected message kind mapping if applicable.
    pub fn get_next_conversation(&self) -> Option<Box<dyn Conversation<T> + Send + Sync>> {
        let (id, priority) = self.queue.pop()?;
        if !self.is_active_conversation(id) {
            self.queue.push(id, priority);
            return None;
        }

        self.handle_timeouts();

        let conversation = self.active_convos.lock().unwrap().remove(&id);
        let expected_kind = conversation.as_ref().unwrap().get_expected_kind();
        // Log dequeue before returning
        let entities = conversation.as_ref().unwrap().get_entities_ids();
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
        conversation
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    fn is_active_conversation(&self, id: ID) -> bool {
        self.active_convos.lock().unwrap().contains_key(&id)
    }

    pub fn add_waiting_message(&self, convo_id: ID, message: PossibleMessage<ExplorerBagContent>) {
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

    pub fn get_waiting_message(&self, convo_id: ID) -> Option<PossibleMessage<ExplorerBagContent>> {
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

    impl Conversation<ExplorerBagContent> for MockConversation {
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
            _msg: Option<PossibleMessage<ExplorerBagContent>>,
        ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
            let mut state = self.state.lock().unwrap();
            state.transitions = 1;
            state.alive = false;
            None
        }

        fn get_priority(&self) -> i32 {
            self.priority
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

        fn transitions(&self) -> usize {
            self.state.lock().unwrap().transitions
        }
    }

    // ============================================================================
    // ConvoScheduler Basic Operations
    // ============================================================================

    #[test]
    fn scheduler_new_is_empty() {
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
        assert!(scheduler.is_empty());
    }

    #[test]
    fn scheduler_add_single_conversation() {
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
        let convo = Box::new(MockConversation::new(100, 1, 10, None));

        scheduler.add_conversation(convo);
        assert!(!scheduler.is_empty());
    }

    #[test]
    fn scheduler_get_next_conversation() {
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
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
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
        assert!(scheduler.get_next_conversation().is_none());
    }

    #[test]
    fn scheduler_clone_shares_state() {
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
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
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
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
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
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
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
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
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
        let kind = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck);

        let convo = Box::new(MockConversation::new(100, 42, 10, None));
        scheduler.add_conversation(convo);

        let found = scheduler.find_matching_conversation(&kind, (Some(42), None));
        assert!(found.is_none());
    }

    #[test]
    fn find_matching_multiple_same_kind_different_entities() {
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
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
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
        let msg = scheduler.get_waiting_message(999);
        assert!(msg.is_none());
    }

    #[test]
    fn get_waiting_message_twice_returns_none() {
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
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

        let scheduler = Arc::new(ConvoScheduler::<ExplorerBagContent>::new());
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

        let scheduler = Arc::new(ConvoScheduler::<ExplorerBagContent>::new());

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
                    let convo: Box<dyn Conversation<ExplorerBagContent> + Send + Sync> = convo;
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

        let scheduler = Arc::new(ConvoScheduler::<ExplorerBagContent>::new());
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
                    let convo: Box<dyn Conversation<ExplorerBagContent> + Send + Sync> = convo;
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

        fn was_timeout_called(&self) -> bool {
            *self.on_timeout_called.lock().unwrap()
        }
    }

    impl Conversation<ExplorerBagContent> for TimeoutMockConversation {
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
            _msg: Option<PossibleMessage<ExplorerBagContent>>,
        ) -> Option<Box<dyn Conversation<ExplorerBagContent> + Send + Sync>> {
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
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
        let timeout_duration = Duration::from_millis(100);

        let convo = TimeoutMockConversation::new(1, 10, Some(timeout_duration));
        scheduler.add_conversation(Box::new(convo));

        // Timeout should be registered
        let timeouts = scheduler.timeouts.lock().unwrap();
        assert!(timeouts.contains_key(&1));
        let (_, duration) = timeouts.get(&1).unwrap();
        assert_eq!(*duration, timeout_duration);
    }

    #[test]
    fn no_timeout_registered_for_conversation_without_timeout() {
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();

        let convo = MockConversation::new(1, 10, 1, None);
        scheduler.add_conversation(Box::new(convo));

        // Default timeout is applied by the trait, so a timeout should be registered
        let timeouts = scheduler.timeouts.lock().unwrap();
        assert!(timeouts.contains_key(&1));
    }

    #[test]
    fn timeout_cleared_when_message_arrives() {
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();
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
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();

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
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();

        // Add a conversation with a long timeout
        let convo = TimeoutMockConversation::new(1, 10, Some(Duration::from_secs(60)));
        scheduler.add_conversation(Box::new(convo));

        // Handle timeouts immediately - should do nothing
        scheduler.handle_timeouts();

        // Conversation should not be removed
        assert!(scheduler.inactive_convos.lock().unwrap().contains_key(&1));
    }

    #[test]
    fn get_timed_out_conversations_works() {
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();

        // Manually set a timeout that has already expired
        scheduler.timeouts.lock().unwrap().insert(
            42,
            (
                Instant::now().checked_sub(Duration::from_secs(10)).unwrap(),
                Duration::from_secs(1),
            ),
        );

        let timed_out = scheduler.get_timed_out_conversations();
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0], 42);
    }

    #[test]
    fn is_timed_out_works() {
        let scheduler = ConvoScheduler::<ExplorerBagContent>::new();

        // Not registered = not timed out
        assert!(!scheduler.is_timed_out(1));

        // Set a timeout that hasn't expired
        scheduler.set_timeout(1, Duration::from_secs(60));
        assert!(!scheduler.is_timed_out(1));

        // Set a timeout in the past (already expired)
        scheduler.timeouts.lock().unwrap().insert(
            2,
            (
                Instant::now().checked_sub(Duration::from_secs(10)).unwrap(),
                Duration::from_secs(1),
            ),
        );
        assert!(scheduler.is_timed_out(2));
    }
}
