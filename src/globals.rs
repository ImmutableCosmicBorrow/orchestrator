use crate::id::IdManager;
use std::sync::OnceLock;

static ID_MANAGER: OnceLock<IdManager> = OnceLock::new();

pub fn get_id_manager() -> &'static IdManager {
    ID_MANAGER.get_or_init(IdManager::new)
}
