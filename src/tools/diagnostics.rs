//! Runtime diagnostics behind the hidden `searchstats` and `nnueregions` UCI
//! commands.
//!
//! Both are opt-in measurement tools for the engine's developers, off by
//! default, and neither changes what the engine plays:
//!
//! * `searchstats` counts, per search, how much of the tree is quiescence, how
//!   often the transposition table is probed and hit, and how early the cutoff
//!   move is found. Every searcher keeps plain per-thread counters, copies the
//!   switch once at the start of a search (so the node loop never reads an
//!   atomic) and merges its counters into one process-wide snapshot when its
//!   search ends.
//! * `nnueregions` counts accumulator refreshes per king region: the
//!   real-game distribution of the network's region table. Refreshes are rare
//!   (a few percent of the nodes), so the counters are atomics read on that
//!   path only.
//!
//! The command surface and the output lines follow the reference engine's
//! hidden hooks so existing analysis scripts keep working.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Number of cutoff-index buckets; the last bucket means "that index or later".
pub const CUTOFF_BUCKETS: usize = 16;

/// Counters of one search (one searcher, or the merge of all of them).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchStats {
    /// Nodes visited, quiescence included.
    pub nodes: u64,
    /// Quiescence nodes.
    pub qnodes: u64,
    /// Transposition-table probes by the main search.
    pub tt_probes: u64,
    /// Probes that found an entry.
    pub tt_hits: u64,
    /// Transposition-table probes by quiescence.
    pub qtt_probes: u64,
    /// Quiescence probes that found an entry.
    pub qtt_hits: u64,
    /// Beta cutoffs in the main search.
    pub cutoffs: u64,
    /// Beta cutoffs in quiescence.
    pub qcutoffs: u64,
    /// Main-search cutoffs by the index of the cutoff move (0 = the first
    /// legal move tried).
    pub cutoff_index: [u64; CUTOFF_BUCKETS],
    /// Quiescence cutoffs by the index of the cutoff move.
    pub qcutoff_index: [u64; CUTOFF_BUCKETS],
}

impl SearchStats {
    /// Returns zeroed counters.
    pub const fn new() -> Self {
        Self {
            nodes: 0,
            qnodes: 0,
            tt_probes: 0,
            tt_hits: 0,
            qtt_probes: 0,
            qtt_hits: 0,
            cutoffs: 0,
            qcutoffs: 0,
            cutoff_index: [0; CUTOFF_BUCKETS],
            qcutoff_index: [0; CUTOFF_BUCKETS],
        }
    }

    /// Adds `other` into `self`.
    pub fn add(&mut self, other: &SearchStats) {
        self.nodes += other.nodes;
        self.qnodes += other.qnodes;
        self.tt_probes += other.tt_probes;
        self.tt_hits += other.tt_hits;
        self.qtt_probes += other.qtt_probes;
        self.qtt_hits += other.qtt_hits;
        self.cutoffs += other.cutoffs;
        self.qcutoffs += other.qcutoffs;
        for index in 0..CUTOFF_BUCKETS {
            self.cutoff_index[index] += other.cutoff_index[index];
            self.qcutoff_index[index] += other.qcutoff_index[index];
        }
    }

    /// Records one main-search transposition-table probe.
    #[inline]
    pub fn record_tt_probe(&mut self, hit: bool) {
        self.tt_probes += 1;
        self.tt_hits += u64::from(hit);
    }

    /// Records one quiescence transposition-table probe.
    #[inline]
    pub fn record_qtt_probe(&mut self, hit: bool) {
        self.qtt_probes += 1;
        self.qtt_hits += u64::from(hit);
    }

    /// Records a main-search beta cutoff by the `legal_move_index`-th move.
    #[inline]
    pub fn record_cutoff(&mut self, legal_move_index: usize) {
        self.cutoffs += 1;
        self.cutoff_index[legal_move_index.min(CUTOFF_BUCKETS - 1)] += 1;
    }

    /// Records a quiescence beta cutoff by the `legal_move_index`-th move.
    #[inline]
    pub fn record_qcutoff(&mut self, legal_move_index: usize) {
        self.qcutoffs += 1;
        self.qcutoff_index[legal_move_index.min(CUTOFF_BUCKETS - 1)] += 1;
    }

    /// Formats the one-line report of `searchstats`.
    pub fn report(&self) -> String {
        let percent = |part: u64, whole: u64| {
            if whole > 0 {
                100.0 * part as f64 / whole as f64
            } else {
                0.0
            }
        };
        format!(
            "searchstats nodes {} qnodes {} qshare {:.1}% \
             tt {} hits {} hitrate {:.1}% \
             qtt {} hits {} hitrate {:.1}% \
             cutoffs {} first {:.1}% second {:.1}% third {:.1}% \
             qcutoffs {} first {:.1}% second {:.1}%",
            self.nodes,
            self.qnodes,
            percent(self.qnodes, self.nodes),
            self.tt_probes,
            self.tt_hits,
            percent(self.tt_hits, self.tt_probes),
            self.qtt_probes,
            self.qtt_hits,
            percent(self.qtt_hits, self.qtt_probes),
            self.cutoffs,
            percent(self.cutoff_index[0], self.cutoffs),
            percent(self.cutoff_index[1], self.cutoffs),
            percent(self.cutoff_index[2], self.cutoffs),
            self.qcutoffs,
            percent(self.qcutoff_index[0], self.qcutoffs),
            percent(self.qcutoff_index[1], self.qcutoffs),
        )
    }
}

impl Default for SearchStats {
    fn default() -> Self {
        Self::new()
    }
}

static SEARCH_STATS_ENABLED: AtomicBool = AtomicBool::new(false);
static LAST_SEARCH: Mutex<SearchStats> = Mutex::new(SearchStats::new());

/// Turns search counting on or off (it applies from the next search on).
pub fn set_search_stats_enabled(enabled: bool) {
    SEARCH_STATS_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Returns whether searches are being counted.
pub fn search_stats_enabled() -> bool {
    SEARCH_STATS_ENABLED.load(Ordering::Relaxed)
}

/// Forgets the snapshot of the last counted search.
pub fn clear_last_search() {
    *LAST_SEARCH.lock().expect("the stats lock must not poison") = SearchStats::new();
}

/// Adds one searcher's counters to the snapshot of the current search.
pub fn merge_into_last_search(stats: &SearchStats) {
    LAST_SEARCH
        .lock()
        .expect("the stats lock must not poison")
        .add(stats);
}

/// Returns a copy of the snapshot of the last counted search.
pub fn last_search() -> SearchStats {
    *LAST_SEARCH.lock().expect("the stats lock must not poison")
}

/// Upper bound of the network's region count (the loader enforces it).
const MAX_REGIONS: usize = 32;

static REGION_COUNTS: [AtomicU64; MAX_REGIONS] = [const { AtomicU64::new(0) }; MAX_REGIONS];
static REGION_COUNTING: AtomicBool = AtomicBool::new(false);

/// Starts or stops counting accumulator refreshes per king region.
pub fn set_region_counting(enabled: bool) {
    REGION_COUNTING.store(enabled, Ordering::Relaxed);
}

/// Returns whether refreshes are being counted.
pub fn region_counting() -> bool {
    REGION_COUNTING.load(Ordering::Relaxed)
}

/// Clears every region counter.
pub fn reset_regions() {
    for counter in &REGION_COUNTS {
        counter.store(0, Ordering::Relaxed);
    }
}

/// Counts one accumulator refresh of `region`; a no-op unless counting is on.
///
/// `region` is below [`MAX_REGIONS`] because the network loader rejects any
/// larger region table.
#[inline]
pub fn record_region(region: usize) {
    if REGION_COUNTING.load(Ordering::Relaxed) {
        REGION_COUNTS[region % MAX_REGIONS].fetch_add(1, Ordering::Relaxed);
    }
}

/// Returns the first `regions` counters and their total.
pub fn region_snapshot(regions: usize) -> (Vec<u64>, u64) {
    let counts: Vec<u64> = REGION_COUNTS
        .iter()
        .take(regions.min(MAX_REGIONS))
        .map(|counter| counter.load(Ordering::Relaxed))
        .collect();
    let total = counts.iter().sum();
    (counts, total)
}

/// Formats the histogram line of `nnueregions`.
pub fn region_report(counts: &[u64], total: u64, counting: bool) -> String {
    let mut line = String::from("nnueregions");
    for (region, count) in counts.iter().enumerate() {
        line.push_str(&format!(" r{region}={count}"));
    }
    line.push_str(&format!(" total {total}"));
    if counting {
        line.push_str(" (counting)");
    }
    line
}

/// Formats the per-region shares line, or `None` when nothing was counted.
pub fn region_shares(counts: &[u64], total: u64) -> Option<String> {
    if total == 0 {
        return None;
    }
    let shares: Vec<String> = counts
        .iter()
        .enumerate()
        .map(|(region, &count)| format!("r{region} {:.3}%", 100.0 * count as f64 / total as f64))
        .collect();
    Some(format!(
        "info string nnueregions shares: {}",
        shares.join(" ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoffs_land_in_their_bucket_and_the_last_bucket_collects_the_rest() {
        let mut stats = SearchStats::new();
        stats.record_cutoff(0);
        stats.record_cutoff(0);
        stats.record_cutoff(2);
        stats.record_cutoff(15);
        stats.record_cutoff(400);
        assert_eq!(stats.cutoffs, 5);
        assert_eq!(stats.cutoff_index[0], 2);
        assert_eq!(stats.cutoff_index[2], 1);
        assert_eq!(stats.cutoff_index[15], 2);
    }

    #[test]
    fn merging_adds_every_counter() {
        let mut first = SearchStats::new();
        first.nodes = 10;
        first.record_tt_probe(true);
        first.record_qcutoff(1);
        let mut second = SearchStats::new();
        second.nodes = 5;
        second.record_tt_probe(false);
        second.record_qtt_probe(true);
        first.add(&second);
        assert_eq!(first.nodes, 15);
        assert_eq!((first.tt_probes, first.tt_hits), (2, 1));
        assert_eq!((first.qtt_probes, first.qtt_hits), (1, 1));
        assert_eq!(first.qcutoff_index[1], 1);
    }

    #[test]
    fn the_report_has_the_documented_shape_and_no_nan() {
        let empty = SearchStats::new().report();
        assert!(!empty.contains("NaN"));
        assert!(empty.starts_with("searchstats nodes 0 qnodes 0 qshare 0.0% tt 0 hits 0"));

        let mut stats = SearchStats::new();
        stats.nodes = 200;
        stats.qnodes = 50;
        stats.record_tt_probe(true);
        stats.record_tt_probe(false);
        stats.record_cutoff(0);
        assert_eq!(
            stats.report(),
            "searchstats nodes 200 qnodes 50 qshare 25.0% tt 2 hits 1 hitrate 50.0% \
             qtt 0 hits 0 hitrate 0.0% cutoffs 1 first 100.0% second 0.0% third 0.0% \
             qcutoffs 0 first 0.0% second 0.0%"
        );
    }

    #[test]
    fn region_lines_follow_the_reference_format() {
        let counts = [3, 1, 0, 0];
        assert_eq!(
            region_report(&counts, 4, false),
            "nnueregions r0=3 r1=1 r2=0 r3=0 total 4"
        );
        assert_eq!(
            region_report(&counts, 4, true),
            "nnueregions r0=3 r1=1 r2=0 r3=0 total 4 (counting)"
        );
        assert_eq!(
            region_shares(&counts, 4).as_deref(),
            Some("info string nnueregions shares: r0 75.000% r1 25.000% r2 0.000% r3 0.000%")
        );
        assert!(region_shares(&counts, 0).is_none());
    }
}
