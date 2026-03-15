use orchestrator::ExplorerType;

fn main() {
    let (..) = orchestrator::run(1000, ExplorerType::Explorer, None, None);
}
