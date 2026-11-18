//! The persistent helper pool for Lazy SMP.
//!
//! When `Threads > 1`, the UCI layer hands this pool the per-search job: the
//! shared tables (step 7), the main search's stop flag, and a shared node
//! counter. Every helper runs its own iterative deepening on its own search
//! state — history, correction history, killers, PV table, stacks and picker
//! — which persists between searches, exactly like the main thread's state.
//! Only the main thread runs the time manager and reports; a helper stops
//! when the main thread's stop flag is set, aborting its iteration at the
//! next 1024-node poll.
//!
//! Helpers are started when `Threads` grows and joined when it shrinks; a
//! helper that survives keeps its learned tables.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{Receiver, Sender};

use crate::board::position::Position;
use crate::search::limits::Limits;
use crate::search::params::SearchParams;
use crate::search::searcher::Searcher;
use crate::search::tables::SharedTables;

/// The stop-flag handle shared between the UCI loop and the search workers.
pub type StopFlag = Arc<AtomicBool>;

/// The shared node counter: helpers publish their counts in blocks.
pub type SharedNodes = Arc<AtomicU64>;

/// One helper's search command.
#[derive(Clone)]
pub struct SearchJob {
    /// The position to search (the same root the main thread searches).
    pub position: Position,
    /// The limits minus the time budgets: a helper runs no time manager; the
    /// main thread's stop flag ends its iteration.
    pub limits: Limits,
    /// The main searcher's current parameters, so `setoption` levers reach
    /// the helpers without rebuilding their state.
    pub params: SearchParams,
    /// The shared tables (step 7).
    pub tables: SharedTables,
    /// The main search's stop flag.
    pub stop: StopFlag,
    /// The shared node counter the helper publishes to.
    pub helper_nodes: SharedNodes,
}

/// What the pool tells a helper thread to do next.
enum HelperCommand {
    Search(Box<SearchJob>),
    Quit,
}

/// One persistent helper: a command channel and its thread.
struct Helper {
    commands: Sender<HelperCommand>,
    thread: std::thread::JoinHandle<()>,
}

/// The pool of `Threads - 1` persistent helper threads.
pub struct WorkerPool {
    helpers: Vec<Helper>,
    /// The requested `Threads` value; the pool holds `threads - 1` helpers.
    threads: usize,
}

impl Default for WorkerPool {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerPool {
    /// Creates an empty pool (no helpers, `Threads = 1`).
    pub fn new() -> Self {
        Self {
            helpers: Vec::new(),
            threads: 1,
        }
    }

    /// Returns the requested `Threads` value.
    pub fn threads(&self) -> usize {
        self.threads
    }

    /// Applies a `setoption Threads` value: joins the surplus helpers or
    /// starts the missing ones.
    ///
    /// The main thread's state is not touched — helpers own their own state.
    /// Must be called when no search is active (the join-first rule).
    pub fn set_threads(&mut self, threads: usize) {
        self.threads = threads;
        let wanted = threads.saturating_sub(1);
        while self.helpers.len() > wanted {
            if let Some(helper) = self.helpers.pop() {
                let _ = helper.commands.send(HelperCommand::Quit);
                helper.thread.join().expect("the helper thread must return");
            }
        }
        while self.helpers.len() < wanted {
            let (commands, thread) = spawn_helper();
            self.helpers.push(Helper { commands, thread });
        }
    }

    /// Starts one search on every helper.
    ///
    /// Fire-and-forget: the helpers pick the job up from their queues and
    /// run until the main thread's stop flag is set. A helper still draining
    /// the previous search aborts it at its next 1024-node poll before it
    /// sees the new command.
    pub fn launch(&self, job: SearchJob) {
        for helper in &self.helpers {
            let _ = helper
                .commands
                .send(HelperCommand::Search(Box::new(job.clone())));
        }
    }

    /// Joins every helper, keeping the requested thread count.
    ///
    /// Called on `quit` and at end of input. After it returns, the pool
    /// holds no threads; a following `set_threads` restarts them.
    pub fn shutdown(&mut self) {
        for helper in self.helpers.drain(..) {
            let _ = helper.commands.send(HelperCommand::Quit);
            helper.thread.join().expect("the helper thread must return");
        }
    }
}

/// Spawns one persistent helper thread with its own search state.
///
/// The helper's own transposition table is the smallest allocation: the
/// searches run on the shared handle that arrives with every job.
fn spawn_helper() -> (Sender<HelperCommand>, std::thread::JoinHandle<()>) {
    let (sender, receiver) = std::sync::mpsc::channel::<HelperCommand>();
    let thread = std::thread::Builder::new()
        .name("knightbyte-helper".to_owned())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || helper_loop(Searcher::new(1), receiver))
        .expect("a helper thread must spawn");
    (sender, thread)
}

/// The helper's main loop: wait for a job, search it, wait again.
///
/// The helper's search state persists across jobs; the tables, stop flag and
/// parameters arrive with every job, so option changes reach every helper.
fn helper_loop(mut searcher: Searcher, receiver: Receiver<HelperCommand>) {
    while let Ok(command) = receiver.recv() {
        match command {
            HelperCommand::Search(job) => {
                *searcher.params_mut() = job.params;
                searcher.set_tables(job.tables);
                searcher.use_stop_flag(job.stop);
                searcher.search(&job.position, job.limits, &mut |_| {});
                searcher.publish_node_blocks(true);
            }
            HelperCommand::Quit => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SearchJob, WorkerPool};

    #[test]
    fn set_threads_grows_and_shrinks_the_pool() {
        // The pool follows the requested Threads value; a resize joins the
        // surplus helpers and starts the missing ones, without touching the
        // main thread's state.
        let mut pool = WorkerPool::new();
        assert_eq!(pool.threads(), 1);
        pool.set_threads(4);
        assert_eq!(pool.threads(), 4);
        pool.set_threads(2);
        assert_eq!(pool.threads(), 2);
        pool.set_threads(1);
        assert_eq!(pool.threads(), 1);
        pool.shutdown();
    }

    #[test]
    fn a_shrunk_pool_joins_its_helpers() {
        // Growing and shrinking must not leak threads: the same pool can be
        // resized repeatedly.
        let mut pool = WorkerPool::new();
        for size in [2, 8, 3, 1, 5, 1] {
            pool.set_threads(size);
            assert_eq!(pool.threads(), size);
        }
        pool.shutdown();
    }

    #[test]
    fn a_job_is_handed_to_the_helpers() {
        // The launch is fire-and-forget; the helpers stop themselves when
        // the job's stop flag is set. The pool stays usable afterwards.
        let mut pool = WorkerPool::new();
        pool.set_threads(3);
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let position = crate::board::position::Position::from_fen(crate::board::fen::STARTPOS_FEN)
            .expect("the built-in start position must parse");
        pool.launch(SearchJob {
            position,
            limits: crate::search::limits::Limits::depth(4),
            params: crate::search::params::SearchParams::default(),
            tables: crate::search::tables::SharedTables::new(2),
            stop: std::sync::Arc::clone(&stop),
            helper_nodes: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        });
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        pool.shutdown();
    }
}
