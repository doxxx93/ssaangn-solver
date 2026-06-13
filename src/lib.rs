//! Library crate root.
//!
//! Exposes the pure solver core (`hangul`/`matcher`/`solver`/`data`) so it can
//! be reused — notably compiled to WebAssembly for the browser solver. The TUI
//! binary (`main.rs`) keeps its own module tree; only these four pure modules
//! are shared here. No terminal or filesystem code lives in this crate.

pub mod data;
pub mod hangul;
pub mod matcher;
pub mod solver;

/// Browser bindings. Mirrors the TUI's dual-solver flow (`main.rs` `App`):
/// every guess is applied to both the curated pool and the full-dictionary
/// fallback; reads come from whichever pool still has live candidates.
#[cfg(target_arch = "wasm32")]
mod wasm_api {
    use crate::data;
    use crate::hangul::decompose_word;
    use crate::matcher::Clue;
    use crate::solver::{GuessPool, Solver};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub struct WasmSolver {
        /// Curated answer pool — uniform prior, good day-to-day suggestions.
        solver: Solver,
        /// Full-dictionary mirror, frequency-weighted; used once the curated
        /// pool is exhausted (answer outside the curated set).
        wide: Solver,
    }

    #[wasm_bindgen]
    impl WasmSolver {
        #[wasm_bindgen(constructor)]
        pub fn new() -> WasmSolver {
            console_error_panic_hook::set_once();
            let candidates = data::load_words(data::CANDIDATES_JSON);
            let words = data::load_words(data::WORDS_JSON);
            let fallback = data::load_words(data::FALLBACK_JSON);
            let weights = data::load_weights();
            let solver = data::weighted_solver(&candidates, &words, &weights);
            let mut wide = data::weighted_solver(&fallback, &words, &weights);
            wide.weighted = true;
            WasmSolver { solver, wide }
        }

        /// True once the curated pool can't explain the clues — the answer is
        /// outside the curated set, so we read from the fallback mirror.
        fn using_wide(&self) -> bool {
            self.solver.remaining.is_empty() && !self.solver.records.is_empty()
        }

        /// Record a guess and its two observed clues. `c0`/`c1` are single
        /// tokens accepted by `Clue::parse`: an emoji (🥕🍄🧄🍆🍌🍎) or a
        /// digit "1"–"6".
        pub fn guess(&mut self, word: &str, c0: &str, c1: &str) -> Result<(), JsValue> {
            let w = decompose_word(word)
                .ok_or_else(|| JsValue::from_str("추측은 한글 2글자여야 해요"))?;
            let p0 = c0
                .chars()
                .next()
                .and_then(Clue::parse)
                .ok_or_else(|| JsValue::from_str("첫 글자 힌트가 올바르지 않아요"))?;
            let p1 = c1
                .chars()
                .next()
                .and_then(Clue::parse)
                .ok_or_else(|| JsValue::from_str("둘째 글자 힌트가 올바르지 않아요"))?;
            self.solver.apply(w.clone(), [p0, p1]);
            self.wide.apply(w, [p0, p1]);
            Ok(())
        }

        /// Record a 🎃 pumpkin hint: this jamo is present somewhere in the answer.
        pub fn hint(&mut self, jamo: &str) -> Result<(), JsValue> {
            let j = jamo
                .chars()
                .next()
                .ok_or_else(|| JsValue::from_str("자모를 입력하세요"))?;
            self.solver.apply_hint(j);
            self.wide.apply_hint(j);
            Ok(())
        }

        pub fn undo(&mut self) {
            self.solver.undo();
            self.wide.undo();
        }

        pub fn reset(&mut self) {
            self.solver.reset();
            self.wide.reset();
        }

        /// Current state as a JSON string for the UI:
        /// `{using_wide, remaining_count, remaining: [..≤50], suggestions: [{word, expected, is_candidate}]}`.
        pub fn state(&self) -> String {
            let active = if self.using_wide() {
                &self.wide
            } else {
                &self.solver
            };
            // Two facts the UI presents:
            //   `remaining`  — the still-possible answers ("the answer is one of
            //                   these"), ranked by prior likelihood.
            //   `suggestions`— which of those to *play next*, ranked by expected
            //                   remaining pool. Drawn ONLY from the remaining pool
            //                   (`Remaining`), never the whole dictionary, so every
            //                   suggestion is itself a possible answer. This means a
            //                   suggestion can never contain a ruled-out (제외) jamo —
            //                   recommending a word built from excluded letters reads
            //                   as broken. The "probe" alternative (whole-pool
            //                   `Candidates`) narrows marginally faster (in-pool avg
            //                   4.164 vs 4.177) but surfaces non-answer words and
            //                   confused users repeatedly; the ~0.013-guess gain
            //                   isn't worth it.
            let remaining = active.remaining_words_ranked();
            let suggestions: Vec<_> = active
                .suggestions_ex(8, GuessPool::Remaining)
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "word": s.word,
                        "expected": s.expected_remaining,
                    })
                })
                .collect();
            let remaining_top: Vec<&str> = remaining.iter().take(50).copied().collect();
            serde_json::json!({
                "using_wide": self.using_wide(),
                "remaining_count": remaining.len(),
                "remaining": remaining_top,
                "suggestions": suggestions,
            })
            .to_string()
        }
    }
}
