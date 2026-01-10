use crate::orchestrator::conversations::Conversation;
use crate::orchestrator::conversations::PossibleExpectedKinds;
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
    next_id: Mutex<ID>,
}

impl<T: Debug + Eq + Hash> Clone for ConvoScheduler<T> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            active_convos: Arc::clone(&self.active_convos),
            by_expected_msg: Arc::clone(&self.by_expected_msg),
            next_id: Mutex::new(*self.next_id.lock().unwrap()),
        }
    }
}

impl<T: Debug + Eq + Hash> ConvoScheduler<T> {
    pub fn new() -> Self {
        Self {
            queue: PQueue::new(),
            active_convos: Arc::new(Mutex::new(HashMap::new())),
            by_expected_msg: Arc::new(Mutex::new(HashMap::new())),
            next_id: Mutex::new(1),
        }
    }

    pub fn find_matching_conversation(
        //must match both by expected kind and entity id
        &self,
        message_kind: &PossibleExpectedKinds,
        entity_id: ID,
    ) -> Option<Box<dyn Conversation<T> + Send + Sync>> {
        let by_expected_msg = self.by_expected_msg.lock().unwrap();
        if let Some(convo_ids) = by_expected_msg.get(message_kind) {
            for &convo_id in convo_ids {
                let active_convos = self.active_convos.lock().unwrap();
                if let Some(convo) = active_convos.get(&convo_id)
                    && convo.get_entity_id() == entity_id
                {
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

    pub fn add_conversation(&self, conversation: Box<dyn Conversation<T> + Send + Sync>) {
        let id = {
            let mut guard = self.next_id.lock().unwrap();
            let id = *guard;
            *guard += 1;
            id
        };

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
        self.queue.push(id, priority); // TODO: set proper priority
    }

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
}
