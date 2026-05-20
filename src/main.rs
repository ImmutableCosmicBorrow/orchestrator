use orchestrator::ExplorerType;

fn main() {
    let (..) = orchestrator::run(1000, ExplorerType::Nomad,None, None);
}
