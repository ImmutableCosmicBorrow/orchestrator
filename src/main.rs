use orchestrator::ExplorerType;

fn main() {
    let _ = orchestrator::run(&ExplorerType::Nico, None, None, 1000);
}
