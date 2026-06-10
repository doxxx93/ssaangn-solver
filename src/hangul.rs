//! Hangul syllable -> jamo component decomposition.
//!
//! Ported verbatim from the ssaangn game source (hangul_tools.js):
//!   - 초성 (leading consonant): never split (ㄲ, ㅃ stay as-is)
//!   - 중성 (vowel): compound vowels split (ㅘ -> ㅗㅏ, ㅙ -> ㅗㅐ, ...)
//!   - 종성 (batchim): compound finals split (ㄳ -> ㄱㅅ, ㄻ -> ㄹㅁ, ...),
//!     but ㄲ/ㅆ are single finals and stay whole.
//!
//! The first element of `jamos` is always the 초성, which the matcher relies on.

const SYLLABLE_BASE: u32 = 0xAC00; // '가'
const SYLLABLE_LAST: u32 = 0xD7A3; // '힣'

// index 0..=18
const CHO: [&str; 19] = [
    "ㄱ", "ㄲ", "ㄴ", "ㄷ", "ㄸ", "ㄹ", "ㅁ", "ㅂ", "ㅃ", "ㅅ", "ㅆ", "ㅇ", "ㅈ", "ㅉ", "ㅊ", "ㅋ",
    "ㅌ", "ㅍ", "ㅎ",
];

// index 0..=20 (compound vowels expanded)
const JUNG: [&str; 21] = [
    "ㅏ", "ㅐ", "ㅑ", "ㅒ", "ㅓ", "ㅔ", "ㅕ", "ㅖ", "ㅗ", "ㅗㅏ", "ㅗㅐ", "ㅗㅣ", "ㅛ", "ㅜ", "ㅜㅓ",
    "ㅜㅔ", "ㅜㅣ", "ㅠ", "ㅡ", "ㅡㅣ", "ㅣ",
];

// index 0..=27 (0 = no batchim; compound finals expanded)
const JONG: [&str; 28] = [
    "", "ㄱ", "ㄲ", "ㄱㅅ", "ㄴ", "ㄴㅈ", "ㄴㅎ", "ㄷ", "ㄹ", "ㄹㄱ", "ㄹㅁ", "ㄹㅂ", "ㄹㅅ", "ㄹㅌ",
    "ㄹㅍ", "ㄹㅎ", "ㅁ", "ㅂ", "ㅂㅅ", "ㅅ", "ㅆ", "ㅇ", "ㅈ", "ㅊ", "ㅋ", "ㅌ", "ㅍ", "ㅎ",
];

/// A decomposed syllable: the original char, its 초성, and all jamo in order
/// (초성 first). Jamo may repeat (e.g. 각 -> ㄱ ㅏ ㄱ).
#[derive(Clone, Debug)]
pub struct Syllable {
    pub ch: char,
    pub cho: char,
    pub jamos: Vec<char>,
}

impl Syllable {
    /// True if `j` appears anywhere in this syllable's jamo.
    pub fn contains(&self, j: char) -> bool {
        self.jamos.contains(&j)
    }
}

fn first_char(s: &str) -> char {
    s.chars().next().unwrap()
}

/// Decompose a single Hangul syllable. Returns `None` if `ch` is not a
/// precomposed Hangul syllable (가–힣).
pub fn decompose(ch: char) -> Option<Syllable> {
    let code = ch as u32;
    if !(SYLLABLE_BASE..=SYLLABLE_LAST).contains(&code) {
        return None;
    }
    let offset = code - SYLLABLE_BASE;
    let cho_idx = (offset / 588) as usize;
    let jung_idx = ((offset % 588) / 28) as usize;
    let jong_idx = (offset % 28) as usize;

    let mut jamos: Vec<char> = Vec::with_capacity(4);
    let cho = first_char(CHO[cho_idx]);
    jamos.push(cho);
    jamos.extend(JUNG[jung_idx].chars());
    jamos.extend(JONG[jong_idx].chars());

    Some(Syllable { ch, cho, jamos })
}

/// A 2-syllable word decomposed into its two syllables.
#[derive(Clone, Debug)]
pub struct Word2 {
    pub text: String,
    pub syl: [Syllable; 2],
}

/// Decompose a 2-character word. Returns `None` unless it is exactly two
/// Hangul syllables.
pub fn decompose_word(text: &str) -> Option<Word2> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() != 2 {
        return None;
    }
    let a = decompose(chars[0])?;
    let b = decompose(chars[1])?;
    Some(Word2 {
        text: text.to_string(),
        syl: [a, b],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decomposes_compound_vowel() {
        // 관 = ㄱ + ㅘ(ㅗㅏ) + ㄴ
        let s = decompose('관').unwrap();
        assert_eq!(s.cho, 'ㄱ');
        assert_eq!(s.jamos, vec!['ㄱ', 'ㅗ', 'ㅏ', 'ㄴ']);
    }

    #[test]
    fn keeps_double_cho_whole() {
        // 깨 = ㄲ + ㅐ
        let s = decompose('깨').unwrap();
        assert_eq!(s.cho, 'ㄲ');
        assert_eq!(s.jamos, vec!['ㄲ', 'ㅐ']);
    }

    #[test]
    fn splits_compound_batchim() {
        // 값 = ㄱ + ㅏ + ㅄ(ㅂㅅ)
        let s = decompose('값').unwrap();
        assert_eq!(s.jamos, vec!['ㄱ', 'ㅏ', 'ㅂ', 'ㅅ']);
    }

    #[test]
    fn rejects_non_syllable() {
        assert!(decompose('ㄱ').is_none());
        assert!(decompose('A').is_none());
        assert!(decompose_word("관").is_none());
        assert!(decompose_word("관계").is_some());
    }
}
