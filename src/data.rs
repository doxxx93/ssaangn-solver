//! Embedded data files and the shared solver-construction helper.

use crate::solver::Solver;
use std::collections::HashMap;

pub const CANDIDATES_JSON: &str = include_str!("../data/candidates.json");
pub const WORDS_JSON: &str = include_str!("../data/words.json");
pub const FALLBACK_JSON: &str = include_str!("../data/fallback.json");
pub const WEIGHTS_JSON: &str = include_str!("../data/weights.json");

pub fn load_words(json: &str) -> Vec<String> {
    serde_json::from_str(json).expect("embedded word list must be valid JSON")
}

/// Path to the user-maintained blocklist. Resolved from the project dir at
/// compile time so it works regardless of the cwd the binary is launched from.
fn blocklist_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/blocklist.json")
}

/// Junk words to drop from the answer pools (valid game guesses that are never
/// real answers). Unlike the other lists this is read from the filesystem, not
/// embedded, so the TUI's `x <word>` can append to it and the change takes
/// effect on the next run without a rebuild. Missing/invalid file => empty.
pub fn load_blocklist() -> Vec<String> {
    match std::fs::read_to_string(blocklist_path()) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Append a word to the on-disk blocklist. Returns true if it was newly added
/// (false if already present). Write errors are ignored — the in-session
/// exclusion still holds, only persistence across runs is lost.
pub fn add_to_blocklist(word: &str) -> bool {
    let mut list = load_blocklist();
    if list.iter().any(|w| w == word) {
        return false;
    }
    list.push(word.to_string());
    if let Ok(s) = serde_json::to_string(&list) {
        let _ = std::fs::write(blocklist_path(), s);
    }
    true
}

pub fn load_weights() -> HashMap<String, f64> {
    serde_json::from_str(WEIGHTS_JSON).expect("weights.json must be valid JSON")
}

/// Build a solver over `answers` (validated against `valid`) with frequency
/// priors applied. Every mode constructs solvers through this so the weight
/// behaviour is identical everywhere; previously analyze/hintsim/openings built
/// a bare `Solver::new` and silently diverged from the TUI. Weights only affect
/// the display ranking and endgame tie-breaks, so the measurement modes' numbers
/// are unchanged — they just stop being inconsistent.
pub fn weighted_solver(
    answers: &[String],
    valid: &[String],
    weights: &HashMap<String, f64>,
) -> Solver {
    let mut s = Solver::new(answers, valid);
    s.set_weights(weights);
    s
}
