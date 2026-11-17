//! Cancelling long work: a flag the UI sets and the work checks between
//! statements and result batches. The running statement itself is stopped
//! with the connection's interrupt handle (see `parvu-app`'s job runner).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// `Err(Cancelled)` once cancelled.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() { Err(CoreError::Cancelled) } else { Ok(()) }
    }
}
