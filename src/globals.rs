use crate::id::IdManager;
use std::sync::OnceLock;
use std::time::Duration;

pub(crate) const TIMEOUT: Duration = Duration::from_millis(1000);
static GAME_STEP: OnceLock<Duration> = OnceLock::new();
static ID_MANAGER: OnceLock<IdManager> = OnceLock::new();
pub fn get_id_manager() -> &'static IdManager {
    ID_MANAGER.get_or_init(IdManager::new)
}
pub(crate) fn set_game_step(game_step: u64) {
    let _ = GAME_STEP.set(Duration::from_millis(game_step));
}
pub(crate) fn get_game_step() -> Duration {
    *GAME_STEP.get_or_init(|| Duration::from_secs(1))
}
pub(crate) fn get_explorer_timeout() -> Duration {
    get_game_step() + TIMEOUT
}
