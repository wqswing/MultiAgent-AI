//! L1 Controller for MutilAgent.
//!
//! This crate provides the ReAct loop, DAG orchestration, and SOP engine
//! for executing complex tasks.

pub mod dag;
pub mod persistence;
pub mod react;
pub mod sop;
pub mod context;
pub mod delegation;

pub use persistence::InMemorySessionStore;
pub use mutilAgent_core::traits::SessionStore;
pub use react::{ReActAction, ReActConfig, ReActController};
