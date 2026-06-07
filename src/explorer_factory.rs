use crate::convo_manager::ConvoManager;
use crate::globals::get_game_step;
use crate::logging::{LogTarget, log_internal};
use crate::orchestrator::{ChannelsManagerRef, OrchContextRef};
use crate::{get_id_manager, payload};
use common_explorer::ExplorerAI;
use common_game::logging::Channel;
use common_game::utils::ID;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;

#[derive(Clone, Copy, Debug)]
pub enum ExplorerType {
    Vojager,  //Roberto
    Explorer, //Nicola
    Nomad,    //Jacopo
}

// Spawns an Explorer into the Galaxy
pub(crate) fn spawn_explorer(
    orch_context: &OrchContextRef,
    convo_manager: &Arc<ConvoManager>,
    explorers_threads: &mut HashMap<ID, JoinHandle<()>>,
    explorer_type: ExplorerType,
    spawn_planet: ID,
) {
    let id = get_id_manager().get_next_explorer_id_by_type(explorer_type);

    // Create Explorer
    let explorer: Box<dyn ExplorerAI + Send> =
        create_explorer(&orch_context.get_channels_manager(), explorer_type, id);

    // Spawn the Explorer thread
    let handle = spawn_explorer_thread(explorer, id);

    // Add the Explorer location in the DashMap
    orch_context.insert_explorer_location(id, spawn_planet);

    // Add handle to the hashmap
    explorers_threads.insert(id, handle);

    log_internal(
        LogTarget::General,
        Channel::Info,
        payload!(
            action: "Created Explorer",
            explorer_id : id,
        ),
    );

    // Move Manually the explorer to the planet
    convo_manager.create_send_manual_move_conversation(id, None, spawn_planet);
}

// Spawns the first one (or two) Explorer(s)
pub(crate) fn spawn_first_explorers(
    orch_context: &OrchContextRef,
    convo_manager: &Arc<ConvoManager>,
    explorers_threads: &mut HashMap<ID, JoinHandle<()>>,
    explorer1: ExplorerType,
    explorer2: Option<ExplorerType>,
    spawn_planet: Option<ID>,
) {
    // Get Planet id
    let into_planet = resolve_planet_id(&orch_context.get_channels_manager(), spawn_planet);

    // Add first Explorer
    spawn_explorer(
        orch_context,
        convo_manager,
        explorers_threads,
        explorer1,
        into_planet,
    );

    // If the second Explorer is some, add it too
    if let Some(explorer) = explorer2 {
        spawn_explorer(
            orch_context,
            convo_manager,
            explorers_threads,
            explorer,
            into_planet,
        );
    }
}

// Handles Explorers death
pub(crate) fn kill_explorer(
    orch_context_ref: &OrchContextRef,
    convo_manager: &Arc<ConvoManager>,
    explorer_id: ID,
    planet_id: Option<ID>,
    handle_outgoing: bool,
) {
    let planet_id = planet_id.or_else(|| {
        orch_context_ref
            .get_explorers_location()
            .get(&explorer_id)
            .map(|entry| *entry.value())
    });

    if let Some(planet_id) = planet_id {
        convo_manager.remove_convos_for_dead_entity(explorer_id);
        convo_manager.create_kill_explorer_conversation(explorer_id, planet_id, handle_outgoing);
    }
}

fn resolve_planet_id(channels_manager: &ChannelsManagerRef, spawn_planet: Option<ID>) -> ID {
    spawn_planet
        .filter(|id| channels_manager.to_planet_senders_contains(*id))
        .unwrap_or_else(|| {
            let fallback_id = channels_manager.to_planet_senders_next_id().expect("Planet senders hashmap is empty");
            // Log only if a planet was actually requested but not found
            if let Some(requested_id) = spawn_planet {
                log_internal(
                    LogTarget::General,
                    Channel::Warning,
                    payload!(
                    action: "Parameter spawn_planet was Some, but Planet was not found. Spawning Explorer(s) in a random Planet instead",
                    missing_planet_id: requested_id,
                    random_planet_id_chosen: fallback_id
                ),
                );
            }
            fallback_id
        })
}

fn create_explorer(
    channels_manager: &ChannelsManagerRef,
    explorer_type: ExplorerType,
    id: ID,
) -> Box<dyn ExplorerAI + Send> {
    // Create channels
    let (_, rx_orchestrator_to_explorer) = channels_manager.create_orch_to_explorer_channel(id);
    let (_, rx_planet_to_explorer) = channels_manager.create_planet_to_exp_channel(id);

    match explorer_type {
        ExplorerType::Explorer => Box::new(explorer_nico::Explorer::new(
            id,
            channels_manager.get_from_explorers_sender(),
            rx_orchestrator_to_explorer,
            rx_planet_to_explorer,
            get_game_step(),
        )),
        ExplorerType::Vojager => Box::new(explorer_rob::Voyager::new(
            id,
            channels_manager.get_from_explorers_sender(),
            rx_orchestrator_to_explorer,
            rx_planet_to_explorer,
            get_game_step(),
        )),
        ExplorerType::Nomad => {
            let nomad = explorer_jacopo::Nomad::new(
                id,
                channels_manager.get_from_explorers_sender(),
                rx_orchestrator_to_explorer,
                rx_planet_to_explorer,
                get_game_step(),
            );
            Box::new(nomad)
        }
    }
}

fn spawn_explorer_thread(mut explorer: Box<dyn ExplorerAI + Send>, id: ID) -> JoinHandle<()> {
    thread::spawn(move || {
        let result = explorer.run();

        if let Err(e) = result {
            log_internal(
                LogTarget::General,
                Channel::Warning,
                payload!(
                    action: "Explorer thread ended with an error",
                    explorer_id : id,
                    error : e
                ),
            );
        } else {
            log_internal(
                LogTarget::General,
                Channel::Debug,
                payload!(
                    action : "Explorer thread ended correctly",
                    explorer_id : id,
                ),
            );
        }
    })
}
