//! The non-TUI subcommands (`ssaangn <mode> ...`): batch oracle, simulation,
//! hint-timing, analysis, opening ranking, and the headless replay mirror.

use crate::data;
use crate::hangul::{self, decompose_word};
use crate::input::{parse_hint, parse_pin, Action};
use crate::matcher::{self, clue_str};
use crate::solver::{self, Solver};

/// Run the subcommand named `cmd`, returning `true` if it was recognised (so
/// `main` knows to exit instead of launching the TUI).
pub fn dispatch(cmd: &str) -> bool {
    match cmd {
        "clue" => run_clue(),
        "sim" => run_sim(),
        "hintsim" => run_hintsim(),
        "analyze" => run_analyze(),
        "openings" => run_openings(),
        "replay" => run_replay(),
        _ => return false,
    }
    true
}

/// Batch oracle mode: `ssaangn clue` reads "secret<space>guess" lines from
/// stdin and prints the computed clue per line. Used to validate the matcher
/// against the live server.
fn run_clue() {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let mut it = line.split_whitespace();
        let (s, g) = match (it.next(), it.next()) {
            (Some(s), Some(g)) => (s, g),
            _ => continue,
        };
        match (decompose_word(s), decompose_word(g)) {
            (Some(sw), Some(gw)) => {
                println!("{}", clue_str(&matcher::compute_clue(&sw, &gw)))
            }
            _ => println!("ERR"),
        }
    }
}

/// Simulation mode: `ssaangn sim` reads secret words from stdin and, for each,
/// plays the solver greedily (always take the top suggestion) using the
/// verified matcher to generate feedback. Prints "secret guesses" per line and
/// a summary, so we can measure solve quality (avg guesses, >7 failures).
fn run_sim() {
    use std::io::BufRead;
    let candidates = data::load_words(data::CANDIDATES_JSON);
    let words = data::load_words(data::WORDS_JSON);
    let weights = data::load_weights();
    let new_solver = || data::weighted_solver(&candidates, &words, &weights);

    // The opening move is identical for every game; compute it once (or use a
    // forced opening passed as `ssaangn sim <opening>`).
    let opening = match std::env::args().nth(2) {
        Some(w) if decompose_word(&w).is_some() => w,
        _ => new_solver().suggestions(1)[0].word.clone(),
    };
    let opening_word = decompose_word(&opening).unwrap();

    // Guess pool: `ssaangn sim <opening> <rem|cand|all>` (default cand).
    let pool_arg = std::env::args().nth(3).unwrap_or_else(|| "cand".into());
    let all_words: Vec<hangul::Word2> = if pool_arg == "all" {
        words.iter().filter_map(|w| decompose_word(w)).collect()
    } else {
        Vec::new()
    };
    let pick_next = |solver: &Solver| -> String {
        match pool_arg.as_str() {
            "rem" => solver.suggestions_ex(1, solver::GuessPool::Remaining)[0].word.clone(),
            "all" => solver.suggestions_over(1, all_words.iter())[0].word.clone(),
            "smart" => solver.suggestions(1)[0].word.clone(), // endgame lookahead
            _ => solver.suggestions_ex(1, solver::GuessPool::Candidates)[0].word.clone(),
        }
    };

    // Fallback mirror so out-of-pool secrets are actually measured instead of
    // scored as instant failures. Mirrors the TUI: once the curated pool empties,
    // suggestions come from the frequency-weighted noun fallback.
    let fallback = data::load_words(data::FALLBACK_JSON);
    let new_wide = || {
        let mut s = data::weighted_solver(&fallback, &words, &weights);
        s.weighted = true; // fallback pool scores by prior (see App in main.rs)
        s
    };

    let mut counts: Vec<usize> = Vec::new();
    let mut fails = 0usize;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let secret_str = line.trim();
        let secret = match decompose_word(secret_str) {
            Some(w) => w,
            None => continue,
        };
        let mut solver = new_solver();
        let mut wide = new_wide();
        let mut guesses = 0usize;
        let mut next = opening_word.clone();
        loop {
            guesses += 1;
            let clue = matcher::compute_clue(&secret, &next);
            if clue == [matcher::Clue::Match, matcher::Clue::Match] {
                break;
            }
            solver.apply(next.clone(), clue);
            wide.apply(next, clue);
            // Active solver: curated pool while it still has candidates, else the
            // fallback mirror (the case the old sim scored as an instant 99 fail).
            let active = if solver.remaining.is_empty() { &wide } else { &solver };
            if active.remaining.is_empty() {
                guesses = 99; // unsolvable: secret not even in the fallback pool
                break;
            }
            next = decompose_word(&pick_next(active)).unwrap();
            if guesses >= 12 {
                break;
            }
        }
        if guesses > 7 {
            fails += 1;
        }
        counts.push(guesses);
        println!("{secret_str} {guesses}");
    }

    let solved: Vec<usize> = counts.iter().copied().filter(|&c| c <= 12).collect();
    let n = solved.len().max(1);
    let avg = solved.iter().sum::<usize>() as f64 / n as f64;
    let mut dist = [0usize; 13];
    for &c in &counts {
        if c <= 12 {
            dist[c] += 1;
        }
    }
    eprintln!("\n=== sim summary ===");
    eprintln!("games={} avg_guesses={:.3} >7_fails={}", counts.len(), avg, fails);
    for (g, &count) in dist.iter().enumerate().skip(1) {
        if count > 0 {
            eprintln!("  {g} guesses: {count}");
        }
    }
    eprintln!("opening = {opening}  pool = {pool_arg}");
}

/// Hint-timing simulation: `ssaangn hintsim <opening> <when>` where <when> is
/// `never` or an integer k meaning "use the 🎃 hint right after k guesses".
///
/// The hint (free, no guess cost) reveals one jamo present in the answer that
/// still splits the remaining pool. The day picks it pseudo-randomly, so we
/// average over every such splitting jamo. Prints avg guesses over the secrets
/// read from stdin.
fn run_hintsim() {
    use std::io::BufRead;
    let candidates = data::load_words(data::CANDIDATES_JSON);
    let words = data::load_words(data::WORDS_JSON);
    let weights = data::load_weights();
    let opening = std::env::args().nth(2).unwrap_or_else(|| "관심".into());
    let when = std::env::args().nth(3).unwrap_or_else(|| "never".into());
    let hint_after: Option<usize> = when.parse().ok();
    let opening_word = decompose_word(&opening).unwrap();
    let new_solver = || data::weighted_solver(&candidates, &words, &weights);

    // Greedy game; if `hint` = Some((k, j)) apply jamo j once, right after the
    // k-th guess's clue is recorded. Returns guesses used (99 if pool empties).
    let play = |secret: &hangul::Word2, hint: Option<(usize, char)>| -> usize {
        let mut solver = new_solver();
        if let Some((0, j)) = hint {
            solver.apply_hint(j); // hint before any guess
        }
        let mut guesses = 0usize;
        let mut next = opening_word.clone();
        loop {
            let clue = matcher::compute_clue(secret, &next);
            guesses += 1;
            if clue == [matcher::Clue::Match, matcher::Clue::Match] {
                return guesses;
            }
            solver.apply(next, clue);
            if let Some((k, j)) = hint {
                if guesses == k {
                    solver.apply_hint(j);
                }
            }
            if solver.remaining.is_empty() {
                return 99;
            }
            next = decompose_word(&solver.suggestions_ex(1, solver::GuessPool::Candidates)[0].word)
                .unwrap();
            if guesses >= 12 {
                return guesses;
            }
        }
    };

    // Eligible hint jamo after k guesses: secret jamo that are NOT shared by
    // every remaining candidate (i.e. revealing one actually narrows the pool).
    let secret_jamo = |secret: &hangul::Word2| -> Vec<char> {
        let mut sj: Vec<char> = secret.syl[0]
            .jamos
            .iter()
            .chain(secret.syl[1].jamos.iter())
            .copied()
            .collect();
        sj.sort();
        sj.dedup();
        sj
    };
    let splits = |solver: &Solver, secret: &hangul::Word2| -> Vec<char> {
        let rem = solver.remaining_words();
        secret_jamo(secret)
            .into_iter()
            .filter(|&j| {
                !rem.iter().all(|w| {
                    let d = decompose_word(w).unwrap();
                    d.syl[0].contains(j) || d.syl[1].contains(j)
                })
            })
            .collect()
    };
    let eligible = |secret: &hangul::Word2, k: usize| -> Vec<char> {
        let mut solver = new_solver();
        if k == 0 {
            return splits(&solver, secret);
        }
        let mut guesses = 0usize;
        let mut next = opening_word.clone();
        loop {
            let clue = matcher::compute_clue(secret, &next);
            guesses += 1;
            if clue == [matcher::Clue::Match, matcher::Clue::Match] || guesses >= 12 {
                return Vec::new(); // already solved before the hint point
            }
            solver.apply(next, clue);
            if solver.remaining.is_empty() {
                return Vec::new();
            }
            if guesses == k {
                return splits(&solver, secret);
            }
            next = decompose_word(&solver.suggestions_ex(1, solver::GuessPool::Candidates)[0].word)
                .unwrap();
        }
    };

    let mut total = 0.0f64;
    let mut n = 0usize;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let secret = match decompose_word(line.trim()) {
            Some(w) => w,
            None => continue,
        };
        let g = match hint_after {
            None => play(&secret, None) as f64,
            Some(k) => {
                let elig = eligible(&secret, k);
                if elig.is_empty() {
                    play(&secret, None) as f64
                } else {
                    elig.iter().map(|&j| play(&secret, Some((k, j))) as f64).sum::<f64>()
                        / elig.len() as f64
                }
            }
        };
        total += g;
        n += 1;
    }
    eprintln!(
        "opening={opening} hint={when}  games={n}  avg_guesses={:.3}",
        total / n.max(1) as f64
    );
}

/// Analysis mode: `ssaangn analyze <secret> <guess1> <guess2> ...`. Replays each
/// guess (clue computed from the secret) and, after each, prints how many
/// candidates remain and the best next guess under three guess pools:
/// remaining-only / all-candidates / all-words.
fn run_analyze() {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let secret = decompose_word(&args[0]).expect("secret must be 2 Hangul syllables");
    let candidates = data::load_words(data::CANDIDATES_JSON);
    let words = data::load_words(data::WORDS_JSON);
    let weights = data::load_weights();
    let all_words: Vec<hangul::Word2> = words.iter().filter_map(|w| decompose_word(w)).collect();
    let mut solver = data::weighted_solver(&candidates, &words, &weights);

    let best = |s: &Solver, pool: solver::GuessPool| -> String {
        s.suggestions_ex(1, pool)
            .first()
            .map(|x| format!("{} ({:.1})", x.word, x.expected_remaining))
            .unwrap_or_else(|| "—".into())
    };
    let best_all = |s: &Solver| -> String {
        s.suggestions_over(1, all_words.iter())
            .first()
            .map(|x| format!("{} ({:.1})", x.word, x.expected_remaining))
            .unwrap_or_else(|| "—".into())
    };

    println!("정답: {}", args[0]);
    println!(
        "시작: 남은 {}  | 남은풀={} 전체후보={} 전체사전={}",
        solver.remaining.len(),
        best(&solver, solver::GuessPool::Remaining),
        best(&solver, solver::GuessPool::Candidates),
        best_all(&solver),
    );
    for g in &args[1..] {
        let gw = decompose_word(g).expect("guess must be 2 Hangul syllables");
        let clue = matcher::compute_clue(&secret, &gw);
        solver.apply(gw, clue);
        let n = solver.remaining.len();
        let sample: Vec<&str> = solver.remaining_words().into_iter().take(12).collect();
        println!(
            "\n{} {} → 남은 {}{}",
            g,
            matcher::clue_str(&clue),
            n,
            if n <= 12 { format!(" {sample:?}") } else { String::new() }
        );
        if n > 1 {
            println!(
                "  추천: 남은풀={} | 전체후보={} | 전체사전={}",
                best(&solver, solver::GuessPool::Remaining),
                best(&solver, solver::GuessPool::Candidates),
                best_all(&solver),
            );
        }
    }
}

/// Opening ranking: `ssaangn openings <n>` prints the top-n openings by 1-ply
/// expected remaining, with each word's distinct-jamo count.
fn run_openings() {
    let candidates = data::load_words(data::CANDIDATES_JSON);
    let words = data::load_words(data::WORDS_JSON);
    let weights = data::load_weights();
    let s = data::weighted_solver(&candidates, &words, &weights);
    let n: usize = std::env::args()
        .nth(2)
        .and_then(|a| a.parse().ok())
        .unwrap_or(20);
    for (rank, sug) in s.suggestions(n).into_iter().enumerate() {
        let w = decompose_word(&sug.word).unwrap();
        let mut uniq: Vec<char> = Vec::new();
        for j in w.syl[0].jamos.iter().chain(w.syl[1].jamos.iter()) {
            if !uniq.contains(j) {
                uniq.push(*j);
            }
        }
        println!(
            "{:>2}. {}  기대잔여={:>7.2}  고유자모={}",
            rank + 1,
            sug.word,
            sug.expected_remaining,
            uniq.len()
        );
    }
}

/// Replay mode: `ssaangn replay` reads TUI-style input lines from stdin
/// (`관심 사과사과`, `h ㅇ`, `1복`, `명절 바나나가지`) and prints the final
/// remaining candidates and suggestions. Headless mirror of the TUI for testing.
fn run_replay() {
    use std::io::BufRead;
    let candidates = data::load_words(data::CANDIDATES_JSON);
    let words = data::load_words(data::WORDS_JSON);
    let fallback = data::load_words(data::FALLBACK_JSON);
    let weights = data::load_weights();
    let mut solver = data::weighted_solver(&candidates, &words, &weights);
    let mut wide = data::weighted_solver(&fallback, &words, &weights);
    wide.weighted = true; // fallback pool scores by prior (see App in main.rs)

    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(Ok(j)) = parse_hint(t) {
            solver.apply_hint(j);
            wide.apply_hint(j);
            continue;
        }
        if let Some(Ok(a)) = parse_pin(t) {
            match a {
                Action::CharAt(p, c) => {
                    solver.apply_char_at(p, c);
                    wide.apply_char_at(p, c);
                }
                Action::JamoAt(p, c) => {
                    solver.apply_jamo_at(p, c);
                    wide.apply_jamo_at(p, c);
                }
                _ => {}
            }
            continue;
        }
        let mut it = t.split_whitespace();
        let guess = it.next().unwrap();
        let rest: String = it.collect();
        if let (Some(w), Some(clue)) = (decompose_word(guess), crate::input::parse_feedback(&rest)) {
            solver.apply(w.clone(), clue);
            wide.apply(w, clue);
        } else {
            eprintln!("skip: {t}");
        }
    }

    let active = if solver.remaining.is_empty() && !solver.records.is_empty() {
        &wide
    } else {
        &solver
    };
    println!("남은 후보 {}개", active.remaining.len());
    println!(
        "{}",
        active.remaining_words_ranked().into_iter().take(40).collect::<Vec<_>>().join(" ")
    );
    println!("추천:");
    for s in active.suggestions(8) {
        println!(
            "  {} {}  → {:.3}",
            if s.is_candidate { "*" } else { " " },
            s.word,
            s.expected_remaining
        );
    }
}
