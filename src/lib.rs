#![cfg_attr(not(feature = "use_std"), no_std)]
#![cfg_attr(feature = "nightly", feature(cfg_target_has_atomic, integer_atomics))]

#[macro_use]
extern crate cfg_if;

#[cfg(not(feature = "use_std"))]
extern crate core as std;

extern crate crossbeam;

#[cfg(feature = "use_std")]
mod hazard;

pub mod atomic;
// #[cfg(feature = "use_std")]
// pub mod atomic_box;
#[cfg(feature = "use_std")]
pub mod atomic_arc;
pub mod atomic_cell;
pub mod atomic_ref_cell;

// pub use atomic_box::AtomicBox;
pub use atomic_cell::AtomicCell;
pub use atomic_arc::AtomicArc;
