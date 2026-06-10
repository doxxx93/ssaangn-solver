//! Forward matcher: given a secret word and a guess, produce the per-position
//! emoji clue exactly as the ssaangn server does.
//!
//! Algorithm (validated against ssaangn.com/get_external_clues.php):
//!   for each position i:
//!     1. guess[i] == secret[i]                              -> Match  (🥕)
//!     2. n = |unique jamo of guess[i] that are in secret[i]|
//!        n >= 2: guess[i].초성 == secret[i].초성 ? Similar(🍄) : Many(🧄)
//!        n == 1:                                            -> Exists (🍆)
//!        n == 0: any jamo of guess[i] in secret[other] ?    Opposite(🍌) : None(🍎)

use crate::hangul::Word2;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Clue {
    Match,    // 🥕 당연하죠
    Similar,  // 🍄 비슷해요
    Many,     // 🧄 많을 거예요
    Exists,   // 🍆 가지고 있어요
    Opposite, // 🍌 반대로요
    None,     // 🍎 사과
}

impl Clue {
    pub fn emoji(self) -> char {
        match self {
            Clue::Match => '🥕',
            Clue::Similar => '🍄',
            Clue::Many => '🧄',
            Clue::Exists => '🍆',
            Clue::Opposite => '🍌',
            Clue::None => '🍎',
        }
    }

    /// Parse a single clue from an emoji or a digit 1-6.
    pub fn parse(token: char) -> Option<Clue> {
        match token {
            '🥕' | '1' => Some(Clue::Match),
            '🍄' | '2' => Some(Clue::Similar),
            '🧄' | '3' => Some(Clue::Many),
            '🍆' | '4' => Some(Clue::Exists),
            '🍌' | '5' => Some(Clue::Opposite),
            '🍎' | '6' => Some(Clue::None),
            _ => None,
        }
    }
}

/// Count of distinct jamo of `guess_syl` that appear in `secret_syl`.
fn unique_overlap(guess: &crate::hangul::Syllable, secret: &crate::hangul::Syllable) -> usize {
    let mut seen: Vec<char> = Vec::with_capacity(4);
    let mut n = 0;
    for &j in &guess.jamos {
        if seen.contains(&j) {
            continue;
        }
        seen.push(j);
        if secret.jamos.contains(&j) {
            n += 1;
        }
    }
    n
}

/// Compute the two-position clue for `guess` against `secret`.
pub fn compute_clue(secret: &Word2, guess: &Word2) -> [Clue; 2] {
    let mut out = [Clue::None; 2];
    for (i, out_i) in out.iter_mut().enumerate() {
        let other = 1 - i;
        let g = &guess.syl[i];
        let s_same = &secret.syl[i];

        *out_i = if g.ch == s_same.ch {
            Clue::Match
        } else {
            let n = unique_overlap(g, s_same);
            if n >= 2 {
                if g.cho == s_same.cho {
                    Clue::Similar
                } else {
                    Clue::Many
                }
            } else if n == 1 {
                Clue::Exists
            } else if g.jamos.iter().any(|j| secret.syl[other].contains(*j)) {
                Clue::Opposite
            } else {
                Clue::None
            }
        };
    }
    out
}

/// Render a two-position clue as its emoji string.
pub fn clue_str(c: &[Clue; 2]) -> String {
    format!("{}{}", c[0].emoji(), c[1].emoji())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hangul::decompose_word;

    fn check(secret: &str, guess: &str, expect: &str) {
        let s = decompose_word(secret).unwrap();
        let g = decompose_word(guess).unwrap();
        assert_eq!(clue_str(&compute_clue(&s, &g)), expect, "{secret}/{guess}");
    }

    #[test]
    fn matches_known_examples() {
        // From the game's own help screen (secret = 관계).
        check("관계", "올해", "🍆🍎");
        check("관계", "인사", "🍆🍌");
        check("관계", "악보", "🧄🍌");
        check("관계", "과일", "🍄🍎");
        check("관계", "관점", "🥕🍎");
        check("관계", "관계", "🥕🥕");
    }

    #[test]
    fn cho_must_equal_not_just_present() {
        // 나 vs 간: overlap {ㄴ,ㅏ}=2, but 나's 초성 ㄴ is 간's 받침, not 초성 -> 🧄
        check("간장", "나비", "🧄🍎");
        check("나라", "간장", "🧄🍆");
    }
}
