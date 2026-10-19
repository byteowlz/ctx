//! Pure prototype state/config/contracts and loopback transport.
//! ctx-core remains independent of the native surface and decision provider.
pub mod commands;
pub mod config;
pub mod exec;
pub mod layout;
pub mod model;
pub mod proxy;
pub mod state;
pub mod tree;

#[cfg(test)]
mod tree_safety_tests;
#[cfg(test)]
mod tree_tests;

#[cfg(test)]
mod retrieval_tests;
