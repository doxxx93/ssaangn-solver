//! Parsing of TUI / replay input lines into solver actions.

use crate::hangul::Word2;
use crate::matcher::Clue;

/// A parsed input line, ready for the solver to apply.
pub enum Action {
    Guess(Word2, [Clue; 2]),
    Hint(char),
    CharAt(usize, char),
    JamoAt(usize, char),
    /// Permanently exclude a junk word from the answer pools.
    Exclude(String),
    Undo,
    Reset,
    Quit,
    Error(String),
    Noop,
}

/// One clue from a Korean food name (the game's icon names).
fn clue_from_food(tok: &str) -> Option<Clue> {
    match tok {
        "당근" => Some(Clue::Match),
        "버섯" => Some(Clue::Similar),
        "마늘" => Some(Clue::Many),
        "가지" => Some(Clue::Exists),
        "바나나" => Some(Clue::Opposite),
        "사과" => Some(Clue::None),
        _ => None,
    }
}

/// Parse the feedback portion into exactly two clues. Accepts food names
/// (가지 사과), emojis (🍆🍎), and digits (46), freely mixed and with or without
/// spaces. Greedy: tries 3- then 2-char food names, else a single emoji/digit.
pub fn parse_feedback(s: &str) -> Option<[Clue; 2]> {
    let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut clues: Vec<Clue> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let mut matched = false;
        for len in [3usize, 2] {
            if i + len <= chars.len() {
                let sub: String = chars[i..i + len].iter().collect();
                if let Some(c) = clue_from_food(&sub) {
                    clues.push(c);
                    i += len;
                    matched = true;
                    break;
                }
            }
        }
        if matched {
            continue;
        }
        match Clue::parse(chars[i]) {
            Some(c) => {
                clues.push(c);
                i += 1;
            }
            None => return None,
        }
    }
    if clues.len() == 2 {
        Some([clues[0], clues[1]])
    } else {
        None
    }
}

/// Map the input to a 🎃 pumpkin hint if it looks like one: a line starting with
/// `h ` or `🎃`, followed by a single jamo (consonant or vowel). Returns the jamo.
pub fn parse_hint(t: &str) -> Option<Result<char, String>> {
    let rest = if let Some(r) = t.strip_prefix('🎃') {
        r
    } else {
        t.strip_prefix("h ").or_else(|| t.strip_prefix("h\t"))?
    };
    let rest = rest.trim();
    let chars: Vec<char> = rest.chars().collect();
    if chars.len() == 1 && ('ㄱ'..='ㅣ').contains(&chars[0]) {
        Some(Ok(chars[0]))
    } else {
        Some(Err("🎃 힌트는 자모 1개를 입력하세요. 예) h ㅏ".into()))
    }
}

/// Parse a position constraint: a line starting with `1` or `2`, then either a
/// full syllable (`1복` = first char is 복) or a jamo (`2ㅇ` = second char has ㅇ).
pub fn parse_pin(t: &str) -> Option<Result<Action, String>> {
    let mut it = t.chars();
    let pos = match it.next()? {
        '1' => 0,
        '2' => 1,
        _ => return None,
    };
    let rest: Vec<char> = it.collect::<String>().trim().chars().collect();
    if rest.len() != 1 {
        return Some(Err("위치 지정: 1복 / 2용 (확정 글자) 또는 1ㅂ / 2ㅇ (자모)".into()));
    }
    let c = rest[0];
    if ('가'..='힣').contains(&c) {
        Some(Ok(Action::CharAt(pos, c)))
    } else if ('ㄱ'..='ㅣ').contains(&c) {
        Some(Ok(Action::JamoAt(pos, c)))
    } else {
        Some(Err("위치 지정: 1복 / 2용 (확정 글자) 또는 1ㅂ / 2ㅇ (자모)".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fb(s: &str) -> [Clue; 2] {
        parse_feedback(s).expect("should parse")
    }

    #[test]
    fn food_names_with_or_without_space() {
        assert_eq!(fb("가지사과"), [Clue::Exists, Clue::None]);
        assert_eq!(fb("가지 사과"), [Clue::Exists, Clue::None]);
        assert_eq!(fb("바나나당근"), [Clue::Opposite, Clue::Match]);
        assert_eq!(fb("버섯마늘"), [Clue::Similar, Clue::Many]);
    }

    #[test]
    fn digits_emojis_and_mixes_still_work() {
        assert_eq!(fb("46"), [Clue::Exists, Clue::None]);
        assert_eq!(fb("🍆🍎"), [Clue::Exists, Clue::None]);
        assert_eq!(fb("가지6"), [Clue::Exists, Clue::None]);
    }

    #[test]
    fn rejects_wrong_count_or_unknown() {
        assert!(parse_feedback("가지").is_none());
        assert!(parse_feedback("가지사과당근").is_none());
        assert!(parse_feedback("토마토사과").is_none());
    }

    #[test]
    fn pin_parsing() {
        assert!(matches!(parse_pin("1복"), Some(Ok(Action::CharAt(0, '복')))));
        assert!(matches!(parse_pin("2용"), Some(Ok(Action::CharAt(1, '용')))));
        assert!(matches!(parse_pin("1ㅂ"), Some(Ok(Action::JamoAt(0, 'ㅂ')))));
        assert!(matches!(parse_pin("2ㅇ"), Some(Ok(Action::JamoAt(1, 'ㅇ')))));
        assert!(parse_pin("관심").is_none()); // a normal guess, not a pin
        assert!(matches!(parse_pin("1복용"), Some(Err(_))));
    }
}
