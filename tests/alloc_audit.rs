//! Allocation audit of the search (performance plan, Phase 7).
//!
//! The search must not allocate per node: no `Vec` growth, `Box`, `String` or
//! `format!` inside the node loop, in move generation, in the evaluation, in the
//! transposition table or in the picker. This test installs a counting global
//! allocator, counts only the allocations made by the thread that runs the
//! search, and checks the property that matters:
//!
//! * the number of allocations of a whole search does **not grow with the node
//!   count**. It may grow with the number of iterations (the per-iteration
//!   report owns a principal-variation `Vec`), which is a few dozen per search
//!   against hundreds of thousands of nodes;
//! * for the bench positions it stays below a fixed budget per iteration.
//!
//! This file holds exactly one test so no other test thread competes for the
//! allocator while it counts.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use knightbyte::board::position::Position;
use knightbyte::search::limits::Limits;
use knightbyte::search::searcher::Searcher;
use knightbyte::tools::bench::BENCH_POSITIONS;

thread_local! {
    /// Whether the current thread's allocations are being counted. A plain
    /// `Cell<bool>` with a constant initialiser: reading it never allocates.
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

/// The system allocator plus a counter for the audited thread.
struct Counting;

// SAFETY: every call is forwarded unchanged to the system allocator; the only
// addition is a relaxed counter bump that does not touch the memory involved.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.try_with(Cell::get).unwrap_or(false) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNTING.try_with(Cell::get).unwrap_or(false) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Result of one counted search.
struct Counted {
    allocations: u64,
    bytes: u64,
    nodes: u64,
    iterations: u64,
}

/// Runs one search to `depth` and counts its allocations.
fn counted_search(searcher: &mut Searcher, position: &Position, depth: u32) -> Counted {
    searcher.clear();
    let mut iterations = 0_u64;
    ALLOCATIONS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    COUNTING.with(|flag| flag.set(true));
    let result = searcher.search(position, Limits::depth(depth), &mut |event| {
        if matches!(event, knightbyte::search::searcher::SearchEvent::Info(_)) {
            iterations += 1;
        }
    });
    COUNTING.with(|flag| flag.set(false));
    Counted {
        allocations: ALLOCATIONS.load(Ordering::Relaxed),
        bytes: BYTES.load(Ordering::Relaxed),
        nodes: result.nodes,
        iterations,
    }
}

#[test]
fn the_search_does_not_allocate_per_node() {
    let mut searcher = Searcher::new(16);
    let positions: Vec<Position> = BENCH_POSITIONS
        .iter()
        .map(|fen| Position::from_fen(fen).expect("a bench FEN must parse"))
        .collect();

    // Warm-up: lazily initialised statics (tables, the embedded network, the
    // kernel selection) allocate once per process, not per search.
    for position in &positions {
        counted_search(&mut searcher, position, 6);
    }

    println!("position depth  nodes  iterations  allocations  bytes");
    for (index, position) in positions.iter().enumerate() {
        let shallow = counted_search(&mut searcher, position, 8);
        let deep = counted_search(&mut searcher, position, 12);
        for (depth, counted) in [(8, &shallow), (12, &deep)] {
            println!(
                "{index:>8} {depth:>5} {:>7} {:>11} {:>12} {:>6}",
                counted.nodes, counted.iterations, counted.allocations, counted.bytes
            );
        }

        // Per-iteration budget: each completed iteration owns two principal-
        // variation `Vec`s (the report and the carried result), measured as
        // exactly two per iteration and none per search. Nothing may scale
        // with the nodes, so the budget leaves room for one more per
        // iteration and a handful of fixed ones.
        for counted in [&shallow, &deep] {
            let budget = 3 * counted.iterations + 8;
            assert!(
                counted.allocations <= budget,
                "position {index}: {} allocations for {} iterations ({} nodes)",
                counted.allocations,
                counted.iterations,
                counted.nodes
            );
        }
        // The deeper search visits several times the nodes; its allocations
        // may only grow with its extra iterations.
        assert!(
            deep.nodes > 2 * shallow.nodes,
            "the depths must differ in size"
        );
        let extra_iterations = deep.iterations.saturating_sub(shallow.iterations);
        assert!(
            deep.allocations <= shallow.allocations + 3 * extra_iterations + 4,
            "position {index}: allocations grew with the nodes ({} -> {} for {} -> {} nodes)",
            shallow.allocations,
            deep.allocations,
            shallow.nodes,
            deep.nodes
        );
    }
}
