#[cfg(test)]
use std::sync::{Arc, OnceLock};

#[cfg(test)]
use common_game::components::forge::Forge;

/// Returns a shared singleton Forge instance for use in tests.
#[cfg(test)]
pub fn get_test_forge() -> Arc<Forge> {
    static FORGE: OnceLock<Arc<Forge>> = OnceLock::new();
    FORGE
        .get_or_init(|| Arc::new(Forge::new().expect("Failed to init Forge for tests")))
        .clone()
}
