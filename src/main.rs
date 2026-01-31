use orchestrator::ExplorerType;

fn main() {
    let _ = orchestrator::run(1000, ExplorerType::Nico, None, None);
}
