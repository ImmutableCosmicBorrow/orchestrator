mod galaxy_setup;
mod globals;
mod id;
mod logging_utils;
mod orchestrator;
mod planet;
mod planet_factory;

pub use globals::get_id_manager;

fn main() {
    // Initialize and start logger
    logging_utils::start_logger();

    println!("Hello, world!");
}
