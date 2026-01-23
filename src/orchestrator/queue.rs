use crate::orchestrator::ExplorerBag;
use crate::orchestrator::conversations::Conversation;
use crate::orchestrator::conversations::PossibleExpectedKinds;
use crate::orchestrator::conversations::PossibleMessage;
use common_game::utils::ID;
use priority_queue::PriorityQueue;
use std::collections::HashSet;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
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
    by_expected_msg: Arc<Mutex<HashMap<PossibleExpectedKinds, HashSet<ID>>>>,
    waiting_msgs: Arc<Mutex<HashMap<ID, PossibleMessage<ExplorerBag>>>>,
}

impl<T: Debug + Eq + Hash> Clone for ConvoScheduler<T> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            active_convos: Arc::clone(&self.active_convos),
            by_expected_msg: Arc::clone(&self.by_expected_msg),
            waiting_msgs: Arc::clone(&self.waiting_msgs),
        }
    }
}

impl<T: Debug + Eq + Hash> ConvoScheduler<T> {
    pub fn new() -> Self {
        Self {
            queue: PQueue::new(),
            active_convos: Arc::new(Mutex::new(HashMap::new())),
            by_expected_msg: Arc::new(Mutex::new(HashMap::new())),
            waiting_msgs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Given a message kind and an entity id, this method looks for an active conversation
    /// that is expecting a message of that kind and is associated with the specified entity.
    /// If such a conversation is found, it is removed from the active conversations and returned.
    pub fn find_matching_conversation(
        &self,
        message_kind: &PossibleExpectedKinds,
        entity_id: ID,
    ) -> Option<Box<dyn Conversation<T> + Send + Sync>> {
        let by_expected_msg = self.by_expected_msg.lock().unwrap();
        if let Some(convo_ids) = by_expected_msg.get(message_kind) {
            for &convo_id in convo_ids {
                let entity_matches = {
                    let active_convos = self.active_convos.lock().unwrap();
                    active_convos
                        .get(&convo_id)
                        .is_some_and(|convo| convo.get_entity_id() == entity_id)
                };

                if entity_matches {
                    // Drop all locks before calling deactivate_conversation
                    drop(by_expected_msg);
                    return Some(self.deactivate_conversation(convo_id));
                }
            }
        }
        None
    }

    fn deactivate_conversation(&self, id: ID) -> Box<dyn Conversation<T> + Send + Sync> {
        let conversation = self.active_convos.lock().unwrap().remove(&id);

        assert!(
            conversation.is_some(),
            "No conversation found with the given ID"
        );

        let expected_kind = conversation.as_ref().unwrap().get_expected_kind();
        if let Some(kind) = expected_kind {
            self.by_expected_msg
                .lock()
                .unwrap()
                .entry(kind)
                .or_default()
                .remove(&id);
        }

        conversation.unwrap()
    }

    /// This method adds a new conversation to the scheduler.
    /// It assigns a unique ID to the conversation, stores it in the active conversations map,
    /// and pushes it onto the priority queue. Moreover, if it has an expected message kind,
    /// it updates the mapping of expected kinds to conversation IDs.
    pub fn add_conversation(&self, conversation: Box<dyn Conversation<T> + Send + Sync>) {
        // let id = crate::get_id_manager().get_next_conversation_id(); //TODO: refactor to keep old id?
        let id = conversation.get_id();

        let expected_kind = conversation.get_expected_kind();
        if let Some(kind) = expected_kind {
            self.by_expected_msg
                .lock()
                .unwrap()
                .entry(kind)
                .or_default()
                .insert(id);
        }

        let priority = conversation.get_priority();
        self.active_convos.lock().unwrap().insert(id, conversation);
        self.queue.push(id, priority);
    }

    /// This method retrieves and removes the next conversation from the scheduler based on priority.
    /// If the conversation is no longer active, it returns None.
    /// Otherwise, it removes the conversation from the active conversations map
    /// and also updates the expected message kind mapping if applicable.
    pub fn get_next_conversation(&self) -> Option<Box<dyn Conversation<T> + Send + Sync>> {
        let (id, _priority) = self.queue.pop()?;
        if !self.is_active_conversation(id) {
            return None;
        }

        let conversation = self.active_convos.lock().unwrap().remove(&id);

        let expected_kind = conversation.as_ref().unwrap().get_expected_kind();
        if let Some(kind) = expected_kind {
            self.by_expected_msg
                .lock()
                .unwrap()
                .entry(kind)
                .or_default()
                .remove(&id);
        }

        conversation
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    fn is_active_conversation(&self, id: ID) -> bool {
        self.active_convos.lock().unwrap().contains_key(&id)
    }

    pub fn add_waiting_message(&self, convo_id: ID, message: PossibleMessage<ExplorerBag>) {
        self.waiting_msgs.lock().unwrap().insert(convo_id, message);
    }

    pub fn get_waiting_message(&self, convo_id: ID) -> Option<PossibleMessage<ExplorerBag>> {
        self.waiting_msgs.lock().unwrap().remove(&convo_id)
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
        entity_id: ID,
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
                .field("entity_id", &self.entity_id)
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

    impl Conversation<ExplorerBag> for MockConversation {
        fn get_id(&self) -> ID {
            self.id
        }

        fn get_entity_id(&self) -> ID {
            self.entity_id
        }

        fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> {
            self.expected_kind.clone()
        }

        fn transition(
            self: Box<Self>,
            _msg: Option<PossibleMessage<ExplorerBag>>,
        ) -> Option<Box<dyn Conversation<ExplorerBag> + Send + Sync>> {
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
            entity_id: ID,
            priority: i32,
            expected_kind: Option<PossibleExpectedKinds>,
        ) -> Self {
            Self {
                id,
                entity_id,
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
            entity_id: ID,
            priority: i32,
            expected_kind: PossibleExpectedKinds,
        ) -> Self {
            Self {
                id,
                entity_id,
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
        let scheduler = ConvoScheduler::<ExplorerBag>::new();
        assert!(scheduler.is_empty());
    }

    #[test]
    fn scheduler_add_single_conversation() {
        let scheduler = ConvoScheduler::<ExplorerBag>::new();
        let convo = Box::new(MockConversation::new(100, 1, 10, None));

        scheduler.add_conversation(convo);
        assert!(!scheduler.is_empty());
    }

    #[test]
    fn scheduler_get_next_conversation() {
        let scheduler = ConvoScheduler::<ExplorerBag>::new();
        let convo = Box::new(MockConversation::new(100, 1, 10, None));
        scheduler.add_conversation(convo);

        let retrieved = scheduler.get_next_conversation().unwrap();
        assert_eq!(retrieved.get_entity_id(), 1);
        assert_eq!(retrieved.get_priority(), 10);
        assert!(scheduler.is_empty());

        let is_active = scheduler.is_active_conversation(retrieved.get_id());
        assert!(!is_active);

        let is_inside_msg_map = scheduler.get_waiting_message(retrieved.get_id());
        assert!(is_inside_msg_map.is_none());

        let is_inside_expected_map = scheduler.find_matching_conversation(
            &PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck),
            1,
        );
        assert!(is_inside_expected_map.is_none());
    }

    #[test]
    fn scheduler_get_from_empty_returns_none() {
        let scheduler = ConvoScheduler::<ExplorerBag>::new();
        assert!(scheduler.get_next_conversation().is_none());
    }

    #[test]
    fn scheduler_clone_shares_state() {
        let scheduler = ConvoScheduler::<ExplorerBag>::new();
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
        let scheduler = ConvoScheduler::<ExplorerBag>::new();
        let kind = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck);

        let convo = Box::new(MockConversation::with_expected_kind(
            100,
            42,
            10,
            kind.clone(),
        ));
        scheduler.add_conversation(convo);

        let found = scheduler.find_matching_conversation(&kind, 42);
        assert!(found.is_some());
        assert_eq!(found.unwrap().get_id(), 100);
    }

    #[test]
    fn find_matching_conversation_wrong_entity() {
        let scheduler = ConvoScheduler::<ExplorerBag>::new();
        let kind = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck);

        let convo = Box::new(MockConversation::with_expected_kind(
            100,
            42,
            10,
            kind.clone(),
        ));
        scheduler.add_conversation(convo);

        let found = scheduler.find_matching_conversation(&kind, 999);
        assert!(found.is_none());
    }

    #[test]
    fn find_matching_conversation_wrong_kind() {
        let scheduler = ConvoScheduler::<ExplorerBag>::new();
        let kind1 = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck);
        let kind2 = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::SunrayAck);

        let convo = Box::new(MockConversation::with_expected_kind(100, 42, 10, kind1));
        scheduler.add_conversation(convo);

        let found = scheduler.find_matching_conversation(&kind2, 42);
        assert!(found.is_none());
        assert!(!scheduler.is_empty());
    }

    #[test]
    fn find_matching_conversation_no_expected_kind() {
        let scheduler = ConvoScheduler::<ExplorerBag>::new();
        let kind = PossibleExpectedKinds::PlanetToOrchKind(PlanetToOrchestratorKind::AsteroidAck);

        let convo = Box::new(MockConversation::new(100, 42, 10, None));
        scheduler.add_conversation(convo);

        let found = scheduler.find_matching_conversation(&kind, 42);
        assert!(found.is_none());
    }

    #[test]
    fn find_matching_multiple_same_kind_different_entities() {
        let scheduler = ConvoScheduler::<ExplorerBag>::new();
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

        let found1 = scheduler.find_matching_conversation(&kind, 2);
        assert!(found1.is_some());
        assert_eq!(found1.unwrap().get_id(), 200);

        let found2 = scheduler.find_matching_conversation(&kind, 1);
        assert!(found2.is_some());
        assert_eq!(found2.unwrap().get_id(), 100);

        let found3 = scheduler.find_matching_conversation(&kind, 3);
        assert!(found3.is_some());
        assert_eq!(found3.unwrap().get_id(), 300);
    }

    // ============================================================================
    // Waiting Messages
    // ============================================================================

    #[test]
    fn get_waiting_message_not_found() {
        let scheduler = ConvoScheduler::<ExplorerBag>::new();
        let msg = scheduler.get_waiting_message(999);
        assert!(msg.is_none());
    }

    #[test]
    fn get_waiting_message_twice_returns_none() {
        let scheduler = ConvoScheduler::<ExplorerBag>::new();
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

        let scheduler = Arc::new(ConvoScheduler::<ExplorerBag>::new());
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

        let scheduler = Arc::new(ConvoScheduler::<ExplorerBag>::new());

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

        let scheduler = Arc::new(ConvoScheduler::<ExplorerBag>::new());
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
}
