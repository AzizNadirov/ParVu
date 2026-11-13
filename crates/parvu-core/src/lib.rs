//! ParVu core: DuckDB-backed data access, tabs, edits, operations and export.
//!
//! Everything here is UI-independent. The Tauri app wraps [`Workspace`] in a
//! mutex and exposes its methods as commands.

pub mod edits;
pub mod engine;
pub mod error;
pub mod export;
pub mod format;
pub mod ops;
pub mod page;
pub mod pagination;
pub mod search;
pub mod sql;
pub mod stats;
pub mod workspace;

pub use duckdb;
pub use error::{CoreError, Result};
pub use workspace::{TabId, TabSummary, Workspace};
