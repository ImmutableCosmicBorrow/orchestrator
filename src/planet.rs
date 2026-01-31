#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

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

/// Canonical undirected edge key: always (min, max).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct EdgeKey {
    lo: ID,
    hi: ID,
}
impl EdgeKey {
    #[inline]
    fn new(a: ID, b: ID) -> Option<Self> {
        use std::cmp::Ordering;
        match a.cmp(&b) {
            Ordering::Equal => None,
            Ordering::Less => Some(Self { lo: a, hi: b }),
            Ordering::Greater => Some(Self { lo: b, hi: a }),
        }
    }

    #[inline]
    fn contains(self, id: ID) -> bool {
        self.lo == id || self.hi == id
    }

    #[inline]
    fn other(self, id: ID) -> Option<ID> {
        if self.lo == id {
            Some(self.hi)
        } else if self.hi == id {
            Some(self.lo)
        } else {
            None
        }
    }
}

/// Stores ONLY “connection between 2 ids”.
#[derive(Default)]
struct ConnectionStore {
    edges: HashSet<EdgeKey>,
}
impl ConnectionStore {
    fn insert_edge(&mut self, a: ID, b: ID) {
        if let Some(k) = EdgeKey::new(a, b) {
            self.edges.insert(k);
        }
    }

    fn remove_edge(&mut self, a: ID, b: ID) {
        if let Some(k) = EdgeKey::new(a, b) {
            self.edges.remove(&k);
        }
    }

    fn remove_all_for(&mut self, id: ID) {
        self.edges.retain(|k| !k.contains(id));
    }

    fn neighbors_snapshot(&self, id: ID) -> Vec<ID> {
        self.edges.iter().filter_map(|k| k.other(id)).collect()
    }

    fn has_neighbor(&self, id: ID, neighbor: ID) -> bool {
        EdgeKey::new(id, neighbor).is_some_and(|k| self.edges.contains(&k))
    }
}

/* -------------------- PRIVATE side-car registry -------------------- */

type ConnHandle = Arc<RwLock<ConnectionStore>>;

static CONN_REGISTRY: OnceLock<Mutex<HashMap<usize, ConnHandle>>> = OnceLock::new();

#[inline]
fn registry() -> &'static Mutex<HashMap<usize, ConnHandle>> {
    CONN_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

#[inline]
fn map_key(map: &PlanetMap) -> usize {
    // stable for the lifetime of this Arc allocation
    Arc::as_ptr(map) as usize
}

#[inline]
fn conns_for_map(map: &PlanetMap) -> ConnHandle {
    let key = map_key(map);

    let mut reg = match registry().lock() {
        Ok(g) => g,
        Err(poison) => {
            log_internal(
                Channel::Warning,
                payload! {
                    event: "mutex_poison_recovered",
                    mutex: "CONN_REGISTRY",
                },
            );
            poison.into_inner()
        }
    };

    reg.entry(key)
        .or_insert_with(|| Arc::new(RwLock::new(ConnectionStore::default())))
        .clone()
}

/// Optional: if you *do* know you’re done with a `PlanetMap` and want to drop its side-car.
/// (Not required for correctness; only for freeing registry memory deterministically.)
pub fn unregister_connections(map: &PlanetMap) {
    let key = map_key(map);
    let mut reg = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    reg.remove(&key);
}

/* -------------------- PlanetNode (no neighbors stored) -------------------- */

/// A planet node in the galaxy graph (undirected connections live elsewhere).
pub struct PlanetNode {
    id: ID,
    alive: AtomicBool,
    // Not a neighbor set. Just a pointer key so node methods can find the right ConnectionStore.
    map_key: usize,
}

impl PlanetNode {
    fn new(id: ID, map_key: usize) -> Self {
        Self {
            id,
            alive: AtomicBool::new(true),
            map_key,
        }
    }

    pub fn id(&self) -> ID {
        self.id
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    fn mark_dead(&self) {
        self.alive.store(false, Ordering::Relaxed);
    }

    /// Same public API as before.
    pub fn neighbors_snapshot(&self) -> Vec<ID> {
        // Look up connection store via registry using the stored key.
        let reg = registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let conns = match reg.get(&self.map_key) {
            Some(c) => Arc::clone(c),
            None => return Vec::new(),
        };
        drop(reg);

        let cg = conns_read(&conns);
        cg.neighbors_snapshot(self.id)
    }

    /// Same public API as before.
    pub fn has_neighbor(&self, neighbor_id: ID) -> bool {
        if neighbor_id == self.id {
            return false;
        }

        let reg = registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let conns = match reg.get(&self.map_key) {
            Some(c) => Arc::clone(c),
            None => return false,
        };
        drop(reg);

        let cg = conns_read(&conns);
        cg.has_neighbor(self.id, neighbor_id)
    }
}

/* -------------------- Poison-tolerant locks (PlanetMap + Connections) -------------------- */

#[inline]
pub(crate) fn map_read(
    map: &PlanetMap,
) -> std::sync::RwLockReadGuard<'_, HashMap<ID, Arc<PlanetNode>>> {
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

#[inline]
pub(crate) fn map_write(
    map: &PlanetMap,
) -> std::sync::RwLockWriteGuard<'_, HashMap<ID, Arc<PlanetNode>>> {
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

#[inline]
fn conns_read(conns: &ConnHandle) -> std::sync::RwLockReadGuard<'_, ConnectionStore> {
    match conns.read() {
        Ok(g) => g,
        Err(poison) => {
            log_internal(
                Channel::Warning,
                payload! {
                    event: "rwlock_poison_recovered",
                    lock: "Connections",
                    mode: "read",
                },
            );
            poison.into_inner()
        }
    }
}

#[inline]
fn conns_write(conns: &ConnHandle) -> std::sync::RwLockWriteGuard<'_, ConnectionStore> {
    match conns.write() {
        Ok(g) => g,
        Err(poison) => {
            log_internal(
                Channel::Warning,
                payload! {
                    event: "rwlock_poison_recovered",
                    lock: "Connections",
                    mode: "write",
                },
            );
            poison.into_inner()
        }
    }
}

/// Internal helper: ensure node exists in an *already-held* `PlanetMap` write guard.
/// Mirrors `ensure_node` logic but avoids re-locking the map.
fn ensure_node_in_guard(
    guard: &mut HashMap<ID, Arc<PlanetNode>>,
    id: ID,
    key: usize,
) -> Arc<PlanetNode> {
    if let Some(existing) = guard.get(&id) {
        if existing.is_alive() {
            return Arc::clone(existing);
        }
        // Node exists but is dead - this is the replacement case worth warning about
        log_internal(
            Channel::Warning,
            payload! {
                event: "Replacing dead PlanetNode with fresh one",
                planet_id: id,
            },
        );
    }

    let fresh = Arc::new(PlanetNode::new(id, key));
    guard.insert(id, Arc::clone(&fresh));

    fresh
}

/* -------------------- Public API (UNCHANGED) -------------------- */

/// Insert node if missing, return Arc.
/// If a node exists but is marked dead, replace it defensively.
pub fn ensure_node(map: &PlanetMap, id: ID) -> Arc<PlanetNode> {
    let key = map_key(map);

    let mut guard = map_write(map);

    if let Some(existing) = guard.get(&id) {
        if existing.is_alive() {
            return Arc::clone(existing);
        }
        // Node exists but is dead - this is the replacement case worth warning about
        log_internal(
            Channel::Warning,
            payload! {
                event: "Replacing dead PlanetNode with fresh one",
                planet_id: id,
            },
        );
    }

    // Ensure side-car exists (does nothing if already created).
    let _ = conns_for_map(map);

    let fresh = Arc::new(PlanetNode::new(id, key));
    guard.insert(id, Arc::clone(&fresh));

    fresh
}

/// Connect two nodes with an undirected edge (A <-> B).
/// Public signature unchanged.
/// Lock order: PlanetMap(write) -> Connections(write)
///
/// # Errors
///
/// Returns [`ConnectError::SameNode`] if `a == b`, [`ConnectError::MissingEndpoint`] if
/// either endpoint is not present in the map, or [`ConnectError::EndpointDead`] if one of
/// the endpoints is not alive.
pub fn connect_undirected(map: &PlanetMap, a: ID, b: ID) -> Result<(), ConnectError> {
    if a == b {
        return Err(ConnectError::SameNode);
    }

    let conns = conns_for_map(map);

    // Hold map write lock for endpoint validation (same style as before).
    let guard = map_write(map);

    let (na, nb) = match (guard.get(&a), guard.get(&b)) {
        (Some(na), Some(nb)) => (Arc::clone(na), Arc::clone(nb)),
        _ => return Err(ConnectError::MissingEndpoint),
    };

    if !na.is_alive() || !nb.is_alive() {
        return Err(ConnectError::EndpointDead);
    }

    // Single source-of-truth edge insert (cannot become one-sided).
    let mut cg = conns_write(&conns);
    cg.insert_edge(a, b);

    Ok(())
}

/// Remove a node from the graph and clean up connections.
/// Public signature unchanged.
/// Lock order: PlanetMap(write) -> Connections(write)
pub fn remove_node_with_stop<F>(map: &PlanetMap, dead_id: ID, stop_fn: F) -> bool
where
    F: FnOnce(ID),
{
    let conns = conns_for_map(map);

    // 1) Hold map write lock for structural change.
    let mut guard = map_write(map);

    let removed = match guard.get(&dead_id) {
        Some(node) => Arc::clone(node),
        None => return false,
    };

    // 2) Mark dead while still in map.
    removed.mark_dead();

    // 3) Remove all edges involving dead_id in ONE place (no asymmetry possible).
    let mut cg = conns_write(&conns);
    cg.remove_all_for(dead_id);
    drop(cg);

    // 4) Remove from map.
    guard.remove(&dead_id);
    drop(guard);

    // 5) Callback after consistent.
    stop_fn(dead_id);
    true
}

/// Add (or ensure) a planet node and connect it to the provided neighbors.
///
/// - Creates/replaces the planet node if missing or dead
/// - Creates/replaces neighbor nodes if missing or dead
/// - Adds undirected edges using the centralized connection store (no one-sided edges possible)
///
/// Returns the Arc<PlanetNode> for `id`.
pub fn add_planet_with_neighbors<I>(map: &PlanetMap, id: ID, neighbors: I) -> Arc<PlanetNode>
where
    I: IntoIterator<Item = ID>,
{
    let key = map_key(map);

    // Ensure the connection store exists for this map.
    let conns = conns_for_map(map);

    // Lock the planet map and ensure all nodes exist and are alive.
    let mut mg = map_write(map);

    let node = ensure_node_in_guard(&mut mg, id, key);

    // Collect unique neighbor IDs, skipping self.
    let mut uniq_neighbors: HashSet<ID> = HashSet::new();
    for n in neighbors {
        if n != id {
            uniq_neighbors.insert(n);
        }
    }

    // Ensure all neighbor nodes exist.
    for nid in uniq_neighbors.iter().copied() {
        let _ = ensure_node_in_guard(&mut mg, nid, key);
    }

    // Lock connections and add all edges.
    // Lock order: PlanetMap(write) then Connections(write)
    let mut cg = conns_write(&conns);
    for nid in uniq_neighbors {
        cg.insert_edge(id, nid);
    }

    node
}
