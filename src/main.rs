mod galaxy_setup;
mod logging_utils;
mod orchestrator;
mod planet;

fn main() {
    // Initialize and start logger
    logging_utils::start_logger();

    println!("Hello, world!");
}
