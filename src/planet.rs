// Temporary allow dead code warnings for this module while in development
#![allow(dead_code)]

use common_game::components::planet::Planet;
use common_game::utils::ID;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex, Weak};

/// State markers for type-safe planet lifecycle management
pub(crate) struct Alive;
pub(crate) struct Dead;

// Crate level trait to constrain planet states to Alive and Dead
pub(crate) trait State {}

impl State for Alive {}
impl State for Dead {}

pub(crate) struct PlanetNode<S: State> {
    pub(crate) inner: Arc<Mutex<PlanetNodeInner<S>>>,
    pub(crate) id: ID,
    neighbors: Mutex<Vec<Weak<Mutex<PlanetNodeInner<Alive>>>>>,
}

impl PlanetNode<Alive> {
    pub fn new(planet: Planet) -> Self {
        let id = planet.id();
        PlanetNode {
            inner: Arc::new(Mutex::new(PlanetNodeInner::new(planet))),
            id,
            neighbors: Mutex::new(Vec::new()),
        }
    }

    /// Add a neighbor to the planet
    pub fn add_neighbor(&self, neighbor: &PlanetNode<Alive>) {
        self.neighbors
            .lock()
            .unwrap()
            .push(Arc::downgrade(&neighbor.inner));
    }

    /// Check if a specific planet is a neighbor
    pub fn has_neighbor(&self, neighbor: &Arc<Mutex<PlanetNodeInner<Alive>>>) -> bool {
        self.neighbors.lock().unwrap().iter().any(|weak_neighbor| {
            if let Some(strong_neighbor) = weak_neighbor.upgrade() {
                Arc::ptr_eq(&strong_neighbor, neighbor)
            } else {
                false
            }
        })
    }

    /// Get all neighbors as vector of IDs
    pub fn get_neighbors(&self) -> Vec<ID> {
        self.neighbors
            .lock()
            .unwrap()
            .iter()
            .filter_map(Weak::upgrade)
            .map(|strong_neighbor| strong_neighbor.lock().unwrap().planet.id())
            .collect()
    }
}

/// A planet node in the galaxy graph
pub(crate) struct PlanetNodeInner<S: State> {
    pub(crate) planet: Planet,
    _state: PhantomData<S>,
}

/// Implementations for alive planet nodes
impl PlanetNodeInner<Alive> {
    /// Create a new alive planet node
    pub fn new(planet: Planet) -> Self {
        PlanetNodeInner {
            planet,
            _state: PhantomData,
        }
    }

    /// Mark the planet as dead, transitioning its state
    pub fn kill(self) -> PlanetNodeInner<Dead> {
        PlanetNodeInner {
            planet: self.planet,
            _state: PhantomData,
        }
    }

    /// Get the planet data
    pub fn planet(&self) -> &Planet {
        &self.planet
    }
}

/// Implementations for dead planet nodes
impl PlanetNodeInner<Dead> {
    /// Revive the dead planet, transitioning its state back to alive
    ///
    /// Here in case we decide that we let the user revive planets
    /// needs to fetch neighbors from initialization file (not implemented)
    /// remove `allow(dead_code)` if we decide to use it
    #[allow(dead_code)]
    pub fn revive(self) -> PlanetNodeInner<Alive> {
        PlanetNodeInner {
            planet: self.planet,
            _state: PhantomData,
        }
    }
}
