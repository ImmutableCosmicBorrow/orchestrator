use orchestrator::ExplorerType;

fn main() {
    let (..) = orchestrator::run(1000, ExplorerType::Jaco, None, None);
}
