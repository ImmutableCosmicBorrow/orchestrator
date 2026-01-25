mod galaxy_setup;
mod globals;
mod id;
mod logging_utils;
mod orchestrator;
mod planet;
mod planet_factory;

pub use globals::get_id_manager;
use std::path::Path;

fn main() {
    // Initialize and start logger
    logging_utils::start_logger();

    let mut orchestrator = orchestrator::Orchestrator::new(Path::new("galaxy/test_galaxy.txt"));

    orchestrator.run();
}
