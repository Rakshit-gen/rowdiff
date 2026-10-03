use std::sync::atomic::{AtomicU8, AtomicU64, Ordering::Relaxed};

/// Where a running diff is, readable from any thread while it runs.
///
/// `done` and `total` are bytes while a file is being read and sorted, and
/// rows of the first file while comparing.
#[derive(Debug, Default)]
pub struct Progress {
    phase: AtomicU8,
    done: AtomicU64,
    total: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Starting,
    ReadingA,
    ReadingB,
    Comparing,
    Done,
}

/// How often the hot loops publish progress. Often enough for a smooth bar,
/// rare enough that the atomics cost nothing.
pub(crate) const EVERY: u64 = 16 * 1024;

impl Progress {
    pub fn snapshot(&self) -> (Phase, u64, u64) {
        let phase = match self.phase.load(Relaxed) {
            1 => Phase::ReadingA,
            2 => Phase::ReadingB,
            3 => Phase::Comparing,
            4 => Phase::Done,
            _ => Phase::Starting,
        };
        (phase, self.done.load(Relaxed), self.total.load(Relaxed))
    }

    pub(crate) fn start(&self, phase: Phase, total: u64) {
        self.done.store(0, Relaxed);
        self.total.store(total, Relaxed);
        self.phase.store(phase as u8, Relaxed);
    }

    pub(crate) fn set(&self, done: u64) {
        self.done.store(done, Relaxed);
    }
}
