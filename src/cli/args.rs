use clap::{Parser, ValueEnum};
use common_game::utils::ID;
use orchestrator::ExplorerType;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ExplorerKindArg {
    Explorer,
    Vojager,
    Nomad,
}

impl From<ExplorerKindArg> for ExplorerType {
    fn from(value: ExplorerKindArg) -> Self {
        match value {
            ExplorerKindArg::Explorer => ExplorerType::Explorer,
            ExplorerKindArg::Vojager => ExplorerType::Vojager,
            ExplorerKindArg::Nomad => ExplorerType::Nomad,
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "orchestrator", version, about = "Orchestrator CLI")]
pub struct Cli {
    /// Galaxy config file path.
    #[arg(short, long, default_value = "galaxy/test_galaxy.txt")]
    pub galaxy: PathBuf,

    /// Game step in milliseconds.
    #[arg(short = 't', long, default_value_t = 1000)]
    pub game_step: u64,

    /// First explorer type.
    #[arg(short = 'e', long, value_enum, default_value_t = ExplorerKindArg::Explorer)]
    pub explorer1: ExplorerKindArg,

    /// Optional second explorer type.
    #[arg(short = 'E', long, value_enum)]
    pub explorer2: Option<ExplorerKindArg>,

    /// Optional spawn planet id for initial explorers.
    #[arg(short = 's', long)]
    pub spawn_planet: Option<ID>,

    /// Disable interactive REPL and just run.
    #[arg(short = 'n', long, default_value_t = false)]
    pub no_repl: bool,
}
