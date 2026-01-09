use crate::orchestrator::conversations::Conversation;
use std::sync::{Arc, Mutex};

pub(crate) type PQueue<T> = Arc<Mutex<Vec<Box<dyn Conversation<T> + Send + Sync>>>>;
