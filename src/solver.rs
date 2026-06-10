//! Solver state: keeps the answer pool, narrows it by observed clues, and
//! ranks candidate guesses by how much they are expected to shrink the pool.

use crate::hangul::{decompose_word, Word2};
use crate::matcher::{compute_clue, Clue};
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

/// One recorded guess and the clue the player observed for it.
#[derive(Clone)]
pub struct Step {
    pub guess: Word2,
    pub clue: [Clue; 2],
}

/// A recorded player action that constrains the answer.
#[derive(Clone)]
pub enum Record {
    /// A guess and its observed two-position clue.
    Guess(Step),
    /// A 🎃 pumpkin hint: this jamo is present somewhere in the answer.
    Hint(char),
    /// Position `pos` (0 or 1) is exactly this syllable (a confirmed character).
    CharAt(usize, char),
    /// Position `pos` (0 or 1) contains this jamo.
    JamoAt(usize, char),
}

pub struct Solver {
    /// Full answer pool (precomputed decompositions).
    pub answers: Vec<Word2>,
    /// Prior likelihood weight per answer (≈ word frequency). Uniform 1.0 unless
    /// `set_weights` is called; common words get higher weight so suggestions and
    /// candidate listings favour likely answers.
    pub weight: Vec<f64>,
    /// Valid-guess set, as plain strings, for input validation.
    pub valid: std::collections::HashSet<String>,
    /// Indices into `answers` still consistent with all records so far.
    pub remaining: Vec<usize>,
    /// All player actions (guesses and pumpkin hints), in order.
    pub records: Vec<Record>,
    /// Answer indices permanently excluded by the user — junk words the game
    /// accepts as guesses but never uses as answers (음역 고유명사/비단어). Kept
    /// out of `remaining` across narrowing, undo, and reset.
    pub blocked: std::collections::HashSet<usize>,
    /// When true, the scoring functions weight each remaining answer by its prior
    /// (`weight`) instead of treating them as equally likely. Correct when the
    /// answer distribution is non-uniform — the case in the broad fallback pool,
    /// where common words are far likelier answers than rare distractors.
    pub weighted: bool,
}

/// A ranked guess suggestion.
pub struct Suggestion {
    pub word: String,
    /// Expected remaining pool size after playing this guess (lower is better).
    pub expected_remaining: f64,
    /// Whether this guess is itself still a possible answer.
    pub is_candidate: bool,
}

impl Solver {
    pub fn new(answer_words: &[String], valid_words: &[String]) -> Solver {
        let answers: Vec<Word2> = answer_words
            .iter()
            .filter_map(|w| decompose_word(w))
            .collect();
        let valid: std::collections::HashSet<String> = valid_words.iter().cloned().collect();
        let remaining = (0..answers.len()).collect();
        let weight = vec![1.0; answers.len()];
        Solver {
            answers,
            weight,
            valid,
            remaining,
            records: Vec::new(),
            blocked: std::collections::HashSet::new(),
            weighted: false,
        }
    }

    /// Prior mass of answer `i` for scoring: its weight when probability-weighted
    /// scoring is on, else 1.0 (every remaining answer equally likely).
    #[inline]
    fn mass(&self, i: usize) -> f64 {
        if self.weighted {
            self.weight[i]
        } else {
            1.0
        }
    }

    /// Permanently drop an answer word from this solver's pool (a junk word).
    /// Stays excluded across narrowing, undo, and reset. Returns true if the word
    /// was present in the pool.
    pub fn exclude(&mut self, word: &str) -> bool {
        let Some(i) = self.answers.iter().position(|w| w.text == word) else {
            return false;
        };
        self.blocked.insert(i);
        self.remaining.retain(|&j| j != i);
        true
    }

    /// Set per-answer prior weights from a word→weight map (missing words and
    /// anything below 1.0 default to 1.0 so they stay possible).
    pub fn set_weights(&mut self, map: &std::collections::HashMap<String, f64>) {
        for (i, w) in self.answers.iter().enumerate() {
            self.weight[i] = map.get(&w.text).copied().unwrap_or(1.0).max(1.0);
        }
    }

    pub fn is_valid_guess(&self, w: &str) -> bool {
        self.valid.contains(w) || decompose_word(w).is_some()
    }

    /// True if answer index `i` is consistent with a single record.
    fn consistent(&self, i: usize, rec: &Record) -> bool {
        let w = &self.answers[i];
        match rec {
            Record::Guess(s) => compute_clue(w, &s.guess) == s.clue,
            Record::Hint(j) => w.syl[0].contains(*j) || w.syl[1].contains(*j),
            Record::CharAt(p, c) => w.syl[*p].ch == *c,
            Record::JamoAt(p, j) => w.syl[*p].contains(*j),
        }
    }

    /// Push a record and narrow the remaining pool by it.
    fn push_record(&mut self, rec: Record) {
        let answers = &self.answers;
        self.remaining.retain(|&i| {
            let w = &answers[i];
            match &rec {
                Record::Guess(s) => compute_clue(w, &s.guess) == s.clue,
                Record::Hint(j) => w.syl[0].contains(*j) || w.syl[1].contains(*j),
                Record::CharAt(p, c) => w.syl[*p].ch == *c,
                Record::JamoAt(p, j) => w.syl[*p].contains(*j),
            }
        });
        self.records.push(rec);
    }

    /// Record a guess + observed clue and narrow the remaining pool.
    pub fn apply(&mut self, guess: Word2, clue: [Clue; 2]) {
        self.push_record(Record::Guess(Step { guess, clue }));
    }

    /// Record a 🎃 pumpkin hint (a jamo present somewhere in the answer).
    pub fn apply_hint(&mut self, jamo: char) {
        self.push_record(Record::Hint(jamo));
    }

    /// Record a confirmed character at a position (0 or 1).
    pub fn apply_char_at(&mut self, pos: usize, ch: char) {
        self.push_record(Record::CharAt(pos, ch));
    }

    /// Record that a position (0 or 1) contains a jamo.
    pub fn apply_jamo_at(&mut self, pos: usize, jamo: char) {
        self.push_record(Record::JamoAt(pos, jamo));
    }

    /// Recompute `remaining` from scratch over all records (used by undo).
    fn recompute(&mut self) {
        self.remaining = (0..self.answers.len())
            .filter(|&i| !self.blocked.contains(&i) && self.records.iter().all(|r| self.consistent(i, r)))
            .collect();
    }

    pub fn undo(&mut self) {
        self.records.pop();
        self.recompute();
    }

    pub fn reset(&mut self) {
        self.records.clear();
        self.remaining = (0..self.answers.len())
            .filter(|i| !self.blocked.contains(i))
            .collect();
    }

    /// Words from the remaining pool, as strings.
    pub fn remaining_words(&self) -> Vec<&str> {
        self.remaining
            .iter()
            .map(|&i| self.answers[i].text.as_str())
            .collect()
    }

    /// Remaining words ordered by prior weight (most likely answers first).
    pub fn remaining_words_ranked(&self) -> Vec<&str> {
        let mut idx = self.remaining.clone();
        idx.sort_by(|&a, &b| {
            self.weight[b]
                .partial_cmp(&self.weight[a])
                .unwrap()
                .then_with(|| self.answers[a].text.cmp(&self.answers[b].text))
        });
        idx.into_iter().map(|i| self.answers[i].text.as_str()).collect()
    }

    /// Default suggestions: rank every candidate by the 1-ply expected remaining
    /// pool (weighted by prior in the fallback). Information-greedy play, not
    /// win-greedy lookahead: with the game's hard 7-guess limit, minimising the
    /// expected pool avoids the rare long tails that a mean-minimising lookahead
    /// gambles into (measured: lookahead lost 11/300 out-of-pool games vs 2/300).
    pub fn suggestions(&self, top: usize) -> Vec<Suggestion> {
        self.suggestions_ex(top, GuessPool::Candidates)
    }

    /// Rank guesses by expected remaining pool size (lower is better).
    ///
    /// Score = sum(bucket^2)/total over the 36-way clue-pattern partition of the
    /// *remaining* pool — the expected size of the surviving pool, assuming each
    /// remaining answer is equally likely.
    ///
    /// The guess pool (what we are allowed to play) is selected separately:
    ///  - `Remaining`  : only still-possible answers (can win this turn).
    ///  - `Candidates` : every answer-pool word, even eliminated ones (probes).
    ///  - `AllWords`   : every valid guess in the dictionary (widest probes).
    ///
    /// Ties break toward guesses that are themselves still possible (so an
    /// equally-informative real candidate is preferred over a dead probe), then
    /// alphabetically for stable output.
    pub fn suggestions_ex(&self, top: usize, pool: GuessPool) -> Vec<Suggestion> {
        match pool {
            GuessPool::Remaining => {
                let g: Vec<&Word2> = self.remaining.iter().map(|&i| &self.answers[i]).collect();
                self.suggestions_over(top, g.into_iter())
            }
            GuessPool::Candidates => self.suggestions_over(top, self.answers.iter()),
        }
    }

    /// Score an arbitrary set of guess words against the remaining pool. Used by
    /// `suggestions_ex` and by analysis code that supplies the full dictionary.
    pub fn suggestions_over<'a>(
        &self,
        top: usize,
        guesses: impl Iterator<Item = &'a Word2>,
    ) -> Vec<Suggestion> {
        let rem = &self.remaining;
        if rem.is_empty() {
            return Vec::new();
        }
        let rem_words: std::collections::HashSet<&str> =
            rem.iter().map(|&i| self.answers[i].text.as_str()).collect();

        // Score each guess in parallel — the per-guess clue partition over the
        // remaining pool is independent, and over the big fallback pool this is
        // the hot loop (12k guesses × hundreds remaining). par_iter preserves
        // order, so the subsequent sort is deterministic.
        let guesses: Vec<&Word2> = guesses.collect();
        // Native uses rayon for the hot loop; wasm has no thread pool, so it
        // falls back to a sequential iterator (identical scoring, just serial).
        #[cfg(not(target_arch = "wasm32"))]
        let guess_iter = guesses.par_iter();
        #[cfg(target_arch = "wasm32")]
        let guess_iter = guesses.iter();
        let mut scored: Vec<Suggestion> = guess_iter
            .map(|&guess| {
                // Bucket the remaining answers by clue pattern, accumulating prior
                // mass (count when unweighted). Expected surviving mass after this
                // guess = Σ mass_b² / total — minimised by an even, probability-
                // aware split. `total` is the mass sum so both modes share a path.
                let mut buckets = [0f64; 36];
                for &si in rem {
                    buckets[pattern_index(&compute_clue(&self.answers[si], guess))] += self.mass(si);
                }
                let total: f64 = buckets.iter().sum();
                let exp = buckets.iter().map(|&b| b * b).sum::<f64>() / total;
                Suggestion {
                    word: guess.text.clone(),
                    expected_remaining: exp,
                    is_candidate: rem_words.contains(guess.text.as_str()),
                }
            })
            .collect();

        scored.sort_by(|a, b| {
            a.expected_remaining
                .partial_cmp(&b.expected_remaining)
                .unwrap()
                .then_with(|| b.is_candidate.cmp(&a.is_candidate)) // prefer real candidates
                .then_with(|| a.word.cmp(&b.word))
        });
        scored.dedup_by(|a, b| a.word == b.word);
        scored.truncate(top);
        scored
    }
}

/// Which set of words a suggestion search may draw guesses from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GuessPool {
    /// Only still-possible answers (can win this turn).
    Remaining,
    /// Every answer-pool word, even eliminated ones (probes).
    Candidates,
}

fn clue_index(c: Clue) -> usize {
    match c {
        Clue::Match => 0,
        Clue::Similar => 1,
        Clue::Many => 2,
        Clue::Exists => 3,
        Clue::Opposite => 4,
        Clue::None => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solver() -> Solver {
        let pool = ["관계", "관심", "사랑", "기차", "나비"].map(String::from);
        Solver::new(&pool, &pool)
    }

    #[test]
    fn hint_keeps_only_words_containing_jamo() {
        let mut s = solver();
        s.apply_hint('ㅁ'); // present only in 관심 (ㅅㅣㅁ)
        assert_eq!(s.remaining_words(), vec!["관심"]);
    }

    #[test]
    fn hint_then_undo_restores_pool() {
        let mut s = solver();
        let before = s.remaining.len();
        s.apply_hint('ㅂ'); // only 나비
        assert_eq!(s.remaining_words(), vec!["나비"]);
        s.undo();
        assert_eq!(s.remaining.len(), before);
    }

    #[test]
    fn char_and_jamo_pins_narrow() {
        let mut s = solver();
        s.apply_char_at(0, '관'); // 관계, 관심
        let mut got = s.remaining_words();
        got.sort();
        assert_eq!(got, vec!["관계", "관심"]);
        s.apply_jamo_at(1, 'ㅁ'); // 관심 (심=ㅅㅣㅁ)
        assert_eq!(s.remaining_words(), vec!["관심"]);
    }

    #[test]
    fn exclude_drops_word_and_survives_undo_reset() {
        let mut s = solver();
        assert!(s.exclude("관심"));
        assert!(!s.remaining_words().contains(&"관심"));
        // ㅁ is present only in 관심 (심=ㅅㅣㅁ); since it is blocked, the hint
        // leaves nothing rather than resurrecting it.
        s.apply_hint('ㅁ');
        assert!(s.remaining_words().is_empty());
        s.undo();
        assert!(!s.remaining_words().contains(&"관심"));
        s.reset();
        assert!(!s.remaining_words().contains(&"관심"));
        assert!(!s.exclude("없는단어")); // not in pool
    }

    #[test]
    fn hint_matches_compound_vowel_component() {
        let mut s = solver();
        s.apply_hint('ㅗ'); // 관계/관심 both have ㅘ -> ㅗㅏ; 기차/나비/사랑 don't
        let mut got = s.remaining_words();
        got.sort();
        assert_eq!(got, vec!["관계", "관심"]);
    }
}

fn pattern_index(c: &[Clue; 2]) -> usize {
    clue_index(c[0]) * 6 + clue_index(c[1])
}
