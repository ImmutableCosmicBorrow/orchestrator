use crate::orchestrator::conversations::Conversation;
use common_game::utils::ID;
use priority_queue::PriorityQueue;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
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

pub type ConversationMap = Arc<Mutex<HashMap<ID, Box<dyn Conversation + Send + Sync>>>>;
