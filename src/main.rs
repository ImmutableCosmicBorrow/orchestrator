use orchestrator::ExplorerType;

fn main() {
    let _ = orchestrator::run(ExplorerType::Nico, Some(ExplorerType::Rob), None, 1000);
}
