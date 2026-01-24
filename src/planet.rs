#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use crate::logging_utils::log_internal;
use crate::payload;
use common_game::logging::Channel;
use common_game::utils::ID;

/// Map of alive planet nodes by ID.
/// When a planet "dies", it is removed from this map.
pub type PlanetMap = Arc<RwLock<HashMap<ID, Arc<PlanetNode>>>>;

/// Errors for connecting nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    SameNode,
    MissingEndpoint,
    EndpointDead,
}

/// A planet node in the galaxy graph (undirected).
pub struct PlanetNode {
    id: ID,
    alive: AtomicBool,
    neighbors: Mutex<HashSet<ID>>,
}

impl PlanetNode {
    pub fn new(id: ID) -> Self {
        Self {
            id,
            alive: AtomicBool::new(true),
            neighbors: Mutex::new(HashSet::new()),
        }
    }

    pub fn id(&self) -> ID {
        self.id
    }

    // (3) Relaxed is sufficient here since `alive` is a logical gate and
    // not used to publish/guard other memory.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    // (3) Same reasoning as above.
    fn mark_dead(&self) {
        self.alive.store(false, Ordering::Relaxed);
    }

    #[inline]
    fn neighbors_lock(&self) -> std::sync::MutexGuard<'_, HashSet<ID>> {
        match self.neighbors.lock() {
            Ok(g) => g,
            Err(poison) => {
                log_internal(
                    Channel::Warning,
                    payload! {
                        event: "mutex_poison_recovered",
                        mutex: "PlanetNode.neighbors",
                        planet_id: self.id,
                    },
                );
                poison.into_inner()
            }
        }
    }

    fn add_neighbor_id(&self, neighbor_id: ID) {
        if neighbor_id == self.id {
            return;
        }
        self.neighbors_lock().insert(neighbor_id);
    }

    fn remove_neighbor_id(&self, neighbor_id: ID) {
        self.neighbors_lock().remove(&neighbor_id);
    }

    pub fn neighbors_snapshot(&self) -> Vec<ID> {
        self.neighbors_lock().iter().copied().collect()
    }

    pub fn has_neighbor(&self, neighbor_id: ID) -> bool {
        self.neighbors_lock().contains(&neighbor_id)
    }
}

/// Poison-tolerant map read lock with structured logging.
#[inline]
fn map_read(map: &PlanetMap) -> std::sync::RwLockReadGuard<'_, HashMap<ID, Arc<PlanetNode>>> {
    match map.read() {
        Ok(g) => g,
        Err(poison) => {
            log_internal(
                Channel::Warning,
                payload! {
                    event: "rwlock_poison_recovered",
                    lock: "PlanetMap",
                    mode: "read",
                },
            );
            poison.into_inner()
        }
    }
}

/// Poison-tolerant map write lock with structured logging.
#[inline]
fn map_write(map: &PlanetMap) -> std::sync::RwLockWriteGuard<'_, HashMap<ID, Arc<PlanetNode>>> {
    match map.write() {
        Ok(g) => g,
        Err(poison) => {
            log_internal(
                Channel::Warning,
                payload! {
                    event: "rwlock_poison_recovered",
                    lock: "PlanetMap",
                    mode: "write",
                },
            );
            poison.into_inner()
        }
    }
}

/// Helper: lock two nodes’ neighbor sets in deterministic ID order.
#[inline]
fn lock_two_neighbors<'a>(
    a_id: ID,
    a: &'a PlanetNode,
    b_id: ID,
    b: &'a PlanetNode,
) -> (
    std::sync::MutexGuard<'a, HashSet<ID>>,
    std::sync::MutexGuard<'a, HashSet<ID>>,
) {
    if a_id < b_id {
        (a.neighbors_lock(), b.neighbors_lock())
    } else {
        let gb = b.neighbors_lock();
        let ga = a.neighbors_lock();
        (ga, gb)
    }
}

/// Insert node if missing, return Arc.
/// If a node exists but is marked dead, replace it defensively.
pub fn ensure_node(map: &PlanetMap, id: ID) -> Arc<PlanetNode> {
    let mut guard = map_write(map);

    if let Some(existing) = guard.get(&id)
        && existing.is_alive()
    {
        return Arc::clone(existing);
    }

    let fresh = Arc::new(PlanetNode::new(id));
    guard.insert(id, Arc::clone(&fresh));

    log_internal(
        Channel::Warning,
        payload! {
            event: "Created a fresh PlanetNode (replacing dead or missing)",
            planet_id: id,
        },
    );

    fresh
}

/// Connect two nodes with an undirected edge (A <-> B).
/// Lock order: PlanetMap(write) -> node neighbor mutexes (deterministic).
pub fn connect_undirected(map: &PlanetMap, a: ID, b: ID) -> Result<(), ConnectError> {
    if a == b {
        return Err(ConnectError::SameNode);
    }

    // Enforce lock order: take map write lock for the whole mutation.
    let guard = map_write(map);

    let (na, nb) = match (guard.get(&a), guard.get(&b)) {
        (Some(na), Some(nb)) => (Arc::clone(na), Arc::clone(nb)),
        _ => return Err(ConnectError::MissingEndpoint),
    };

    if !na.is_alive() || !nb.is_alive() {
        return Err(ConnectError::EndpointDead);
    }

    // Still holding map lock; now lock node mutexes in a deterministic order.
    let (mut ga, mut gb) = lock_two_neighbors(a, &na, b, &nb);
    ga.insert(b);
    gb.insert(a);

    Ok(())
}

/// Remove a node from the graph and clean up neighbors.
/// Lock order: PlanetMap(write) -> node neighbor mutexes (deterministic).
/// Ensures no window where neighbors contain `dead_id` while map doesn't.
pub fn remove_node_with_stop<F>(map: &PlanetMap, dead_id: ID, stop_fn: F) -> bool
where
    F: FnOnce(ID),
{
    // 1) Hold map write lock for the entire structural change (atomic wrt other mutations).
    let mut guard = map_write(map);

    let removed = match guard.get(&dead_id) {
        Some(node) => Arc::clone(node),
        None => return false,
    };

    // 2) Mark dead while still in map.
    removed.mark_dead();

    // 3) Snapshot neighbors while holding removed's neighbor lock (consistent snapshot).
    let neighbor_ids: Vec<ID> = {
        let g = removed.neighbors_lock();
        g.iter().copied().collect()
    };

    // 4) For each neighbor, remove the undirected edge atomically (both sides),
    //    while still holding map_write and node locks in deterministic order.
    for nid in neighbor_ids {
        // Neighbor might already be missing (if earlier removed); skip if absent.
        let neighbor = match guard.get(&nid) {
            Some(n) => Arc::clone(n),
            None => continue,
        };

        // Lock both sets deterministically (prevents node-node deadlock).
        let (mut g_removed, mut g_neighbor) = lock_two_neighbors(dead_id, &removed, nid, &neighbor);

        g_removed.remove(&nid);
        g_neighbor.remove(&dead_id);
    }

    // 5) Now remove from map (neighbors no longer reference it).
    guard.remove(&dead_id);

    // Drop locks before calling external code.
    drop(guard);

    // 6) Callback after the graph is consistent and unlocked.
    stop_fn(dead_id);
    true
}
