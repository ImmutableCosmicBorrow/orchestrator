use crate::id::IdManager;
use std::sync::OnceLock;

pub(crate) const TIMEOUT: u64 = 1000;
static GAME_STEP: OnceLock<u64> = OnceLock::new();
static ID_MANAGER: OnceLock<IdManager> = OnceLock::new();
pub fn get_id_manager() -> &'static IdManager {
    ID_MANAGER.get_or_init(IdManager::new)
}
pub(crate) fn set_game_step(game_step: u64) {
    let _ = GAME_STEP.set(game_step);
}
pub(crate) fn get_game_step() -> u64 {
    *GAME_STEP.get_or_init(|| 1000)
}
pub(crate) fn get_explorer_timeout() -> u64 {
    get_game_step() + TIMEOUT
}
