#![cfg_attr(not(feature = "use_std"), no_std)]
#![cfg_attr(feature = "nightly", feature(cfg_target_has_atomic, integer_atomics))]
#![warn(missing_docs, missing_debug_implss)]

#[cfg(not(feature = "use_std"))]
extern crate core as std;

extern crate crossbeam;

pub mod atomic;
pub mod atomic_cell;
pub mod atomic_ref_cell;
pub mod hazard_cell;
pub mod pointer;

pub use atomic_cell::AtomicCell;
