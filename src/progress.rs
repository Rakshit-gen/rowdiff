use std::sync::atomic::{AtomicU8, AtomicU64, Ordering::Relaxed};

/// Where a running diff is, readable from any thread while it runs.
///
/// While reading, `done` and `total` are bytes across both files (they are
/// read at the same time, each into its own counter). While comparing they
/// are rows of the first file.
#[derive(Debug, Default)]
pub struct Progress {
    phase: AtomicU8,
    done: [AtomicU64; 2],
    total: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Starting,
    Reading,
    Comparing,
    Done,
}

/// How often the hot loops publish progress. Often enough for a smooth bar,
/// rare enough that the atomics cost nothing.
pub(crate) const EVERY: u64 = 16 * 1024;

impl Progress {
    pub fn snapshot(&self) -> (Phase, u64, u64) {
        let phase = match self.phase.load(Relaxed) {
            1 => Phase::Reading,
            2 => Phase::Comparing,
            3 => Phase::Done,
            _ => Phase::Starting,
        };
        let done = self.done[0].load(Relaxed) + self.done[1].load(Relaxed);
        (phase, done, self.total.load(Relaxed))
    }

    pub(crate) fn start(&self, phase: Phase, total: u64) {
        self.done[0].store(0, Relaxed);
        self.done[1].store(0, Relaxed);
        self.total.store(total, Relaxed);
        self.phase.store(phase as u8, Relaxed);
    }

    /// The counter one worker reports into: 0 for the first file, 1 for the second.
    pub(crate) fn counter(&self, i: usize) -> &AtomicU64 {
        &self.done[i]
    }

    pub(crate) fn set(&self, done: u64) {
        self.done[0].store(done, Relaxed);
    }
}
