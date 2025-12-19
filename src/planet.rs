// Temporary allow dead code warnings for this module while in development
#![allow(dead_code)]

use common_game::components::planet::Planet;
use common_game::utils::ID;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::{Rc, Weak};

/// State markers for type-safe planet lifecycle management
pub(crate) struct Alive;
pub(crate) struct Dead;

// Crate level trait to constrain planet states to Alive and Dead
pub(crate) trait State {}

impl State for Alive {}
impl State for Dead {}

/// A planet node in the galaxy graph
pub(crate) struct PlanetNode<S: State> {
    planet: Planet,
    neighbors: RefCell<Vec<Weak<PlanetNode<S>>>>,
    _state: PhantomData<S>,
}

/// Implementations for alive planet nodes
impl PlanetNode<Alive> {
    /// Create a new alive planet node
    pub fn new(planet: Planet) -> Self {
        PlanetNode {
            planet,
            neighbors: RefCell::new(Vec::new()),
            _state: PhantomData,
        }
    }

    /// Mark the planet as dead, transitioning its state
    pub fn kill(self) -> PlanetNode<Dead> {
        PlanetNode {
            planet: self.planet,
            neighbors: RefCell::new(Vec::new()),
            _state: PhantomData,
        }
    }

    /// Add a neighbor to the planet
    pub fn add_neighbor(&self, neighbor: Weak<PlanetNode<Alive>>) {
        self.neighbors.borrow_mut().push(neighbor);
    }

    /// Check if a specific planet is a neighbor
    pub fn has_neighbor(&self, neighbor: &Rc<PlanetNode<Alive>>) -> bool {
        self.neighbors
            .borrow()
            .iter()
            .any(|weak| weak.ptr_eq(&Rc::downgrade(neighbor)))
    }

    /// Get all neighbors as vector of IDs
    pub fn get_neighbors(&self) -> Vec<ID> {
        self.neighbors
            .borrow()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .map(|neighbor| neighbor.planet().state().id())
            .collect()
    }

    /// Get the planet data
    pub fn planet(&self) -> &Planet {
        &self.planet
    }
}

/// Implementations for dead planet nodes
impl PlanetNode<Dead> {
    /// Revive the dead planet, transitioning its state back to alive
    ///
    /// Here in case we decide that we let the user revive planets
    /// needs to fetch neighbors from initialization file (not implemented)
    /// remove `allow(dead_code)` if we decide to use it
    #[allow(dead_code)]
    pub fn revive(self) -> PlanetNode<Alive> {
        PlanetNode {
            planet: self.planet,
            neighbors: RefCell::new(Vec::new()),
            _state: PhantomData,
        }
    }
}
