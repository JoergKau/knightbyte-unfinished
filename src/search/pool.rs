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
use std::time::Duration;

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
    /// How many helpers are currently inside a search job. The pool's idle
    /// barrier waits for this to reach zero.
    busy: Arc<std::sync::atomic::AtomicU64>,
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
            busy: Arc::new(std::sync::atomic::AtomicU64::new(0)),
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
            let (commands, thread) = spawn_helper(self.busy.clone());
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

    /// Waits until every helper is back to waiting (bounded).
    ///
    /// After a search's stop flag is set, a helper aborts its iteration at
    /// its next 1024-node poll and releases the shared tables — which can
    /// lag the main worker's join by a few milliseconds (up to 1024 nodes).
    /// Every state-touching operation (`setoption Hash`'s exclusive resize
    /// borrow, `ucinewgame`'s clear) must wait for that drain; this is the
    /// bug-B39/B40 crash window, closed here in one place.
    ///
    /// The wait is bounded to five seconds; a helper that has not returned
    /// by then is a defect and the panic names it loudly.
    pub fn wait_until_idle(&self) {
        use std::sync::atomic::Ordering;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while self.busy.load(Ordering::SeqCst) > 0 {
            if std::time::Instant::now() > deadline {
                panic!(
                    "the helper pool did not drain: {} helper(s) still busy",
                    self.busy.load(Ordering::SeqCst)
                );
            }
            std::thread::sleep(Duration::from_millis(1));
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
/// searches run on the shared handle that arrives with every job. `busy` is
/// the pool's idle barrier: one while the helper is inside a search job,
/// zero while it waits.
fn spawn_helper(
    busy: Arc<std::sync::atomic::AtomicU64>,
) -> (Sender<HelperCommand>, std::thread::JoinHandle<()>) {
    let (sender, receiver) = std::sync::mpsc::channel::<HelperCommand>();
    let thread = std::thread::Builder::new()
        .name("knightbyte-helper".to_owned())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || helper_loop(Searcher::new(1), receiver, busy))
        .expect("a helper thread must spawn");
    (sender, thread)
}

/// The helper's main loop: wait for a job, search it, wait again.
///
/// The helper's search state persists across jobs; the tables, stop flag and
/// parameters arrive with every job, so option changes reach every helper.
fn helper_loop(
    mut searcher: Searcher,
    receiver: Receiver<HelperCommand>,
    busy: Arc<std::sync::atomic::AtomicU64>,
) {
    use std::sync::atomic::Ordering;
    while let Ok(command) = receiver.recv() {
        match command {
            HelperCommand::Search(job) => {
                busy.fetch_add(1, Ordering::SeqCst);
                *searcher.params_mut() = job.params;
                searcher.set_tables(job.tables);
                searcher.attach_publisher(job.helper_nodes);
                searcher.use_stop_flag(job.stop);
                searcher.search(&job.position, job.limits, &mut |_| {});
                searcher.publish_node_blocks(true);
                // Bug B39 fix: release the shared-table handle, or the
                // helper's clones would block every later `setoption Hash`
                // resize's exclusive borrow.
                searcher.release_tables();
                // Back to waiting: the pool's idle barrier (and with it any
                // state-touching command) may proceed.
                busy.fetch_sub(1, Ordering::SeqCst);
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

    #[test]
    fn a_helper_releases_the_shared_tables_after_its_search() {
        // Bug B39 regression: a helper that keeps its SharedTables clones
        // between searches would block every later `setoption Hash` resize
        // (the exclusive borrow requires the Arc count to be one). After the
        // helpers drain their job and are joined, a resize must work — and
        // while they are alive, the release after each job keeps the count
        // at one anyway.
        use std::sync::atomic::AtomicU64;
        let mut pool = WorkerPool::new();
        pool.set_threads(2);
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let position = crate::board::position::Position::from_fen(crate::board::fen::STARTPOS_FEN)
            .expect("the built-in start position must parse");
        pool.launch(SearchJob {
            position,
            limits: crate::search::limits::Limits::depth(3),
            params: crate::search::params::SearchParams::default(),
            tables: crate::search::tables::SharedTables::new(2),
            stop: std::sync::Arc::clone(&stop),
            helper_nodes: std::sync::Arc::new(AtomicU64::new(0)),
        });
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        pool.shutdown();
        let mut tables = crate::search::tables::SharedTables::new(2);
        tables.resize(64);
        assert_eq!(tables.tt.size_bytes(), 64 * 1024 * 1024);
    }

    #[test]
    fn helpers_publish_their_nodes_and_the_main_searcher_does_not() {
        // Bug B44 regression: the helpers never published their node blocks,
        // and the main searcher published its OWN count into the counter it
        // reports from, so a multi-threaded search reported about twice the
        // main thread's nodes instead of the sum over all threads.
        use crate::search::limits::Limits;
        use crate::search::searcher::Searcher;
        use std::sync::atomic::{AtomicU64, Ordering};
        let mut pool = WorkerPool::new();
        pool.set_threads(3);
        let position = crate::board::position::Position::from_fen(crate::board::fen::STARTPOS_FEN)
            .expect("the built-in start position must parse");
        let mut main = Searcher::new(8);
        let stop = main.arm_stop_flag();
        let counter = std::sync::Arc::new(AtomicU64::new(0));
        pool.launch(SearchJob {
            position,
            limits: Limits::depth(9),
            params: *main.params(),
            tables: main.tables().clone(),
            stop: std::sync::Arc::clone(&stop),
            helper_nodes: std::sync::Arc::clone(&counter),
        });
        main.attach_helpers(std::sync::Arc::clone(&counter));
        main.search(&position, Limits::depth(9), &mut |_| {});
        // The helpers run until the stop flag; let them publish their final
        // blocks (the job ends with a forced publish), then count.
        stop.store(true, Ordering::Relaxed);
        pool.wait_until_idle();
        let helpers = counter.load(Ordering::Relaxed);
        assert!(helpers > 0, "the helpers must publish their nodes");
        assert_eq!(
            main.published_nodes, 0,
            "the main searcher must not publish"
        );
        assert_eq!(main.reported_nodes(), main.nodes + helpers);
        pool.shutdown();
    }
}
