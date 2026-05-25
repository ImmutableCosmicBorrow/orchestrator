use common_game::components::resource::{BasicResourceType, ComplexResourceType};
use common_game::utils::ID;
use orchestrator::ExplorerType;

pub fn parse_id(input: &str, field: &str) -> Result<ID, String> {
    input
        .parse::<ID>()
        .map_err(|_| format!("Invalid {field}: '{input}'"))
}

pub fn parse_explorer_kind(input: &str) -> Result<ExplorerType, String> {
    match input.to_ascii_lowercase().as_str() {
        "explorer" => Ok(ExplorerType::Explorer),
        "vojager" => Ok(ExplorerType::Vojager),
        "nomad" => Ok(ExplorerType::Nomad),
        _ => Err(format!(
            "Invalid explorer type: '{input}'. Expected one of: explorer, vojager, nomad"
        )),
    }
}

pub fn parse_basic_resource(input: &str) -> Result<BasicResourceType, String> {
    match input.to_ascii_lowercase().as_str() {
        "carbon" => Ok(BasicResourceType::Carbon),
        "hydrogen" => Ok(BasicResourceType::Hydrogen),
        "oxygen" => Ok(BasicResourceType::Oxygen),
        "silicon" => Ok(BasicResourceType::Silicon),
        _ => Err(format!(
            "Invalid basic resource: '{input}'. Expected one of: carbon, hydrogen, oxygen, silicon"
        )),
    }
}

pub fn parse_complex_resource(input: &str) -> Result<ComplexResourceType, String> {
    match input.to_ascii_lowercase().as_str() {
        "water" => Ok(ComplexResourceType::Water),
        "robot" => Ok(ComplexResourceType::Robot),
        "life" => Ok(ComplexResourceType::Life),
        "diamond" => Ok(ComplexResourceType::Diamond),
        "dolphin" => Ok(ComplexResourceType::Dolphin),
        "aipartner" | "ai-partner" | "ai_partner" => Ok(ComplexResourceType::AIPartner),
        _ => Err(format!(
            "Invalid complex resource: '{input}'. Expected one of: water, robot, life, diamond, dolphin, aipartner"
        )),
    }
}
