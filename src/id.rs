use common_game::utils::ID;
use std::sync::{Arc, Mutex};

/// `IdManager` is responsible for generating unique IDs for different entities
/// such as planets, conversations, and explorers. Each ID is constructed using
/// bitwise operations to ensure uniqueness and easy identification of the entity type.
/// The structure uses Mutex to ensure thread safety when generating IDs in a concurrent environment.
///
/// Each constant defines a bit position for different types of entities:
/// `Planets`, `Conversations`, and `Explorers`.
///
/// In addition, `Planets` can be uniquely identified by their group type, with an additional shift.
///
/// When creating a new ID, the relevant bits are set according to the entity type,
/// and a unique number is appended to ensure no two IDs are the same.
pub struct IdManager {
    planet: Arc<Mutex<ID>>,
    conversation: Arc<Mutex<ID>>,
    explorer: Arc<Mutex<ID>>,
}

impl Default for IdManager {
    fn default() -> Self {
        Self::new()
    }
}

impl IdManager {
    const ID_MASK: u32 = 0xF; // 4 bits = 16 planets max per type

    const CONVERSATION_SHIFT: u32 = 16;

    const PLANET_SHIFT: u32 = 12;
    const TRIP_SHIFT: u32 = 11;
    const RUSTRELLI_SHIFT: u32 = 10;
    const LUNA4_SHIFT: u32 = 9;
    const RUSTY_CRAB_SHIFT: u32 = 8;
    const ENTERPRISE_SHIFT: u32 = 7;
    const ORBITRON_SHIFT: u32 = 6;
    const HOUSTON_SHIFT: u32 = 5;

    const EXPLORER_SHIFT: u32 = 4;

    pub fn new() -> Self {
        IdManager {
            planet: Arc::new(Mutex::new(1)),
            conversation: Arc::new(Mutex::new(1)),
            explorer: Arc::new(Mutex::new(1)),
        }
    }

    fn get_next_planet_id(&self) -> ID {
        let mut id_lock = self.planet.lock().unwrap();
        let id = *id_lock;
        *id_lock += 1;
        id
    }

    pub fn get_next_conversation_id(&self) -> ID {
        let mut id_lock = self.conversation.lock().unwrap();
        let id = *id_lock;
        *id_lock += 1;
        1 << Self::CONVERSATION_SHIFT | id
    }

    pub fn get_next_explorer_id(&self) -> ID {
        let mut id_lock = self.explorer.lock().unwrap();
        let id = *id_lock;
        *id_lock += 1;
        1 << Self::EXPLORER_SHIFT | id
    }

    pub fn get_next_trip_id(&self) -> ID {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::TRIP_SHIFT | (id & Self::ID_MASK)
    }

    pub fn get_next_rustrelli_id(&self) -> ID {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::RUSTRELLI_SHIFT | (id & Self::ID_MASK)
    }

    pub fn get_next_luna4_id(&self) -> ID {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::LUNA4_SHIFT | (id & Self::ID_MASK)
    }

    pub fn get_next_rusty_crab_id(&self) -> ID {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::RUSTY_CRAB_SHIFT | (id & Self::ID_MASK)
    }

    pub fn get_next_enterprise_id(&self) -> ID {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::ENTERPRISE_SHIFT | (id & Self::ID_MASK)
    }

    pub fn get_next_orbitron_id(&self) -> ID {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::ORBITRON_SHIFT | (id & Self::ID_MASK)
    }

    pub fn get_next_houston_id(&self) -> ID {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::HOUSTON_SHIFT | (id & Self::ID_MASK)
    }

    pub fn is_planet_id(id: ID) -> bool {
        (id & (1 << Self::PLANET_SHIFT)) != 0
    }

    // Helper functions to identify planet types
    pub fn is_trip_id(id: ID) -> bool {
        (id & (1 << Self::TRIP_SHIFT)) != 0
    }

    pub fn is_rustrelli_id(id: ID) -> bool {
        (id & (1 << Self::RUSTRELLI_SHIFT)) != 0
    }

    pub fn is_luna4_id(id: ID) -> bool {
        (id & (1 << Self::LUNA4_SHIFT)) != 0
    }

    pub fn is_rusty_crab_id(id: ID) -> bool {
        (id & (1 << Self::RUSTY_CRAB_SHIFT)) != 0
    }

    pub fn is_enterprise_id(id: ID) -> bool {
        (id & (1 << Self::ENTERPRISE_SHIFT)) != 0
    }

    pub fn is_orbitron_id(id: ID) -> bool {
        (id & (1 << Self::ORBITRON_SHIFT)) != 0
    }

    pub fn is_houston_id(id: ID) -> bool {
        (id & (1 << Self::HOUSTON_SHIFT)) != 0
    }

    pub fn is_explorer_id(id: ID) -> bool {
        (id & (1 << Self::EXPLORER_SHIFT)) != 0
    }

    pub fn is_conversation_id(id: ID) -> bool {
        (id & (1 << Self::CONVERSATION_SHIFT)) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::get_id_manager;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use std::thread;

    const THREADS: usize = 10;
    const IDS_PER_THREAD: usize = 500;

    #[test]
    fn conversation_ids_unique_across_threads() {
        let manager = get_id_manager();
        let results = Arc::new(Mutex::new(Vec::with_capacity(THREADS * IDS_PER_THREAD)));

        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                let out = Arc::clone(&results);
                let mgr = manager;
                thread::spawn(move || {
                    for _ in 0..IDS_PER_THREAD {
                        let id = mgr.get_next_conversation_id();
                        out.lock().unwrap().push(id);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let locked = results.lock().unwrap();
        let set: HashSet<ID> = locked.iter().copied().collect();
        assert_eq!(locked.len(), set.len(), "conversation ids must be unique");
        assert!(locked.iter().all(|id| IdManager::is_conversation_id(*id)));
    }
}
