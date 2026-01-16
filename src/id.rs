use std::sync::{Arc, Mutex};

pub struct IdManager {
    planet: Arc<Mutex<u32>>,
    conversation: Arc<Mutex<u32>>,
    explorer: Arc<Mutex<u32>>,
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

    fn get_next_planet_id(&self) -> u32 {
        let mut id_lock = self.planet.lock().unwrap();
        let id = *id_lock;
        *id_lock += 1;
        id
    }

    pub fn get_next_trip_id(&self) -> u32 {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::TRIP_SHIFT | (id & Self::ID_MASK)
    }

    pub fn get_next_rustrelli_id(&self) -> u32 {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::RUSTRELLI_SHIFT | (id & Self::ID_MASK)
    }

    pub fn get_next_conversation_id(&self) -> u32 {
        let mut id_lock = self.conversation.lock().unwrap();
        let id = *id_lock;
        *id_lock += 1;
        1 << Self::CONVERSATION_SHIFT | id
    }

    pub fn get_next_explorer_id(&self) -> u32 {
        let mut id_lock = self.explorer.lock().unwrap();
        let id = *id_lock;
        *id_lock += 1;
        1 << Self::EXPLORER_SHIFT | id
    }

    pub fn get_next_luna4_id(&self) -> u32 {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::LUNA4_SHIFT | (id & Self::ID_MASK)
    }

    pub fn get_next_rusty_crab_id(&self) -> u32 {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::RUSTY_CRAB_SHIFT | (id & Self::ID_MASK)
    }

    pub fn get_next_enterprise_id(&self) -> u32 {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::ENTERPRISE_SHIFT | (id & Self::ID_MASK)
    }

    pub fn get_next_orbitron_id(&self) -> u32 {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::ORBITRON_SHIFT | (id & Self::ID_MASK)
    }

    pub fn get_next_houston_id(&self) -> u32 {
        let id = self.get_next_planet_id();
        1 << Self::PLANET_SHIFT | 1 << Self::HOUSTON_SHIFT | (id & Self::ID_MASK)
    }

    pub fn is_planet_id(id: u32) -> bool {
        (id & (1 << Self::PLANET_SHIFT)) != 0
    }

    // Helper functions to identify planet types
    pub fn is_trip_id(id: u32) -> bool {
        (id & (1 << Self::TRIP_SHIFT)) != 0
    }

    pub fn is_rustrelli_id(id: u32) -> bool {
        (id & (1 << Self::RUSTRELLI_SHIFT)) != 0
    }

    pub fn is_luna4_id(id: u32) -> bool {
        (id & (1 << Self::LUNA4_SHIFT)) != 0
    }

    pub fn is_rusty_crab_id(id: u32) -> bool {
        (id & (1 << Self::RUSTY_CRAB_SHIFT)) != 0
    }

    pub fn is_enterprise_id(id: u32) -> bool {
        (id & (1 << Self::ENTERPRISE_SHIFT)) != 0
    }

    pub fn is_orbitron_id(id: u32) -> bool {
        (id & (1 << Self::ORBITRON_SHIFT)) != 0
    }

    pub fn is_houston_id(id: u32) -> bool {
        (id & (1 << Self::HOUSTON_SHIFT)) != 0
    }

    pub fn is_explorer_id(id: u32) -> bool {
        (id & (1 << Self::EXPLORER_SHIFT)) != 0
    }

    pub fn is_conversation_id(id: u32) -> bool {
        (id & (1 << Self::CONVERSATION_SHIFT)) != 0
    }
}
