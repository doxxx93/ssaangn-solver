mod data;
mod hangul;
mod input;
mod matcher;
mod modes;
mod solver;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use hangul::decompose_word;
use input::{parse_feedback, parse_hint, parse_pin, Action};
use matcher::clue_str;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use solver::Solver;

/// Best opening by full-game simulation over the known answer pool
/// (관심: avg 4.14 guesses, vs 4.28 for the greedy 1-ply pick 간장).
const RECOMMENDED_OPENING: &str = "관심";

struct App {
    /// Primary solver over the curated answer pool (good suggestions).
    solver: Solver,
    /// Mirror solver over the full dictionary; used as a fallback when the
    /// curated pool can't explain the clues (answer outside the curated set).
    wide: Solver,
    input: String,
    message: String,
    suggestions: Vec<solver::Suggestion>,
}

impl App {
    /// The solver whose pool currently has live candidates: the curated one
    /// normally, the full-dictionary mirror once the curated pool is exhausted.
    fn active(&self) -> &Solver {
        if self.solver.remaining.is_empty() && !self.solver.records.is_empty() {
            &self.wide
        } else {
            &self.solver
        }
    }

    fn using_wide(&self) -> bool {
        self.solver.remaining.is_empty() && !self.solver.records.is_empty()
    }

    fn status_msg(&self, prefix: &str) -> String {
        let n = self.active().remaining.len();
        if self.using_wide() {
            if n == 0 {
                format!("{prefix} 어디서도 일치 없음 — 피드백을 확인하세요")
            } else {
                format!("{prefix} 큐레이션 풀에 없어 전체 사전에서 검색 → {n}개")
            }
        } else {
            format!("{prefix} 남은 후보 {n}개")
        }
    }

    fn parse_input(&self) -> Action {
        let t = self.input.trim();
        match t {
            "" => return Action::Noop,
            "q" => return Action::Quit,
            "u" => return Action::Undo,
            "r" => return Action::Reset,
            _ => {}
        }
        if let Some(rest) = t.strip_prefix("x ").or_else(|| t.strip_prefix("x\t")) {
            let word = rest.trim();
            return match decompose_word(word) {
                Some(_) => Action::Exclude(word.to_string()),
                None => Action::Error("제외할 한글 2글자 단어를 입력하세요. 예) x 옌병".into()),
            };
        }
        if let Some(h) = parse_hint(t) {
            return match h {
                Ok(j) => Action::Hint(j),
                Err(e) => Action::Error(e),
            };
        }
        if let Some(p) = parse_pin(t) {
            return match p {
                Ok(a) => a,
                Err(e) => Action::Error(e),
            };
        }
        let mut it = t.split_whitespace();
        let guess = match it.next() {
            Some(g) => g,
            None => return Action::Noop,
        };
        let rest: String = it.collect();
        let word = match decompose_word(guess) {
            Some(w) => w,
            None => return Action::Error("추측은 한글 2글자여야 해요".into()),
        };
        if !self.solver.is_valid_guess(guess) {
            // allowed, just warn — dictionaries are not exhaustive
        }
        let clue = match parse_feedback(&rest) {
            Some(c) => c,
            None => {
                return Action::Error(
                    "피드백 2개: 당근 버섯 마늘 가지 바나나 사과 (또는 🥕🍄🧄🍆🍌🍎 / 숫자 1-6)".into(),
                )
            }
        };
        Action::Guess(word, clue)
    }

    fn recompute_suggestions(&mut self) {
        self.suggestions = if self.using_wide() {
            self.wide.suggestions(8)
        } else {
            self.solver.suggestions(8)
        };
    }

    fn submit(&mut self) {
        match self.parse_input() {
            Action::Noop => {}
            Action::Quit => {}
            Action::Error(e) => self.message = e,
            Action::Undo => {
                self.solver.undo();
                self.wide.undo();
                self.input.clear();
                self.recompute_suggestions();
                self.message = "직전 입력 취소".into();
            }
            Action::Reset => {
                self.solver.reset();
                self.wide.reset();
                self.input.clear();
                self.recompute_suggestions();
                self.message = "초기화 완료".into();
            }
            Action::Guess(word, clue) => {
                let not_in_dict = !self.solver.valid.contains(&word.text);
                self.solver.apply(word.clone(), clue);
                self.wide.apply(word, clue);
                self.input.clear();
                self.recompute_suggestions();
                self.message = self.status_msg(if not_in_dict {
                    "기록됨 (사전에 없는 단어)."
                } else {
                    "기록됨."
                });
            }
            Action::Hint(j) => {
                self.solver.apply_hint(j);
                self.wide.apply_hint(j);
                self.input.clear();
                self.recompute_suggestions();
                self.message = self.status_msg(&format!("🎃 '{j}' 정답에 포함."));
            }
            Action::CharAt(pos, c) => {
                self.solver.apply_char_at(pos, c);
                self.wide.apply_char_at(pos, c);
                self.input.clear();
                self.recompute_suggestions();
                self.message = self.status_msg(&format!("📌 {}번째 글자 = {c}.", pos + 1));
            }
            Action::JamoAt(pos, j) => {
                self.solver.apply_jamo_at(pos, j);
                self.wide.apply_jamo_at(pos, j);
                self.input.clear();
                self.recompute_suggestions();
                self.message = self.status_msg(&format!("📌 {}번째 글자에 {j}.", pos + 1));
            }
            Action::Exclude(word) => {
                let found = self.solver.exclude(&word) | self.wide.exclude(&word);
                self.input.clear();
                if found {
                    let newly = data::add_to_blocklist(&word);
                    self.recompute_suggestions();
                    let saved = if newly { " (blocklist 저장됨)" } else { "" };
                    self.message = self.status_msg(&format!("'{word}' 제외{saved}."));
                } else {
                    self.message = format!("'{word}'는 후보 풀에 없어요.");
                }
            }
        }
    }
}

fn main() -> std::io::Result<()> {
    if let Some(cmd) = std::env::args().nth(1) {
        if modes::dispatch(&cmd) {
            return Ok(());
        }
    }

    let candidates = data::load_words(data::CANDIDATES_JSON);
    let words = data::load_words(data::WORDS_JSON);
    let fallback = data::load_words(data::FALLBACK_JSON);
    let weights = data::load_weights();
    let mut solver = data::weighted_solver(&candidates, &words, &weights);
    // Fallback mirror: answer pool = noun-filtered dictionary, frequency-weighted.
    // Unlike the curated pool (whose answers are ~uniformly likely, so uniform
    // scoring wins), the fallback spans common words to rare distractors, so it
    // scores by prior — measured to cut out-of-pool failures ~11→4 per 300.
    let mut wide = data::weighted_solver(&fallback, &words, &weights);
    wide.weighted = true;
    // Drop user-blocklisted junk words from both pools up front.
    for w in data::load_blocklist() {
        solver.exclude(&w);
        wide.exclude(&w);
    }

    let mut app = App {
        solver,
        wide,
        input: String::new(),
        message: format!(
            "후보 {}개 로드됨. 추천 오프닝: {} · 예) 관심 가지 사과",
            candidates.len(),
            RECOMMENDED_OPENING
        ),
        suggestions: Vec::new(),
    };
    app.recompute_suggestions();

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> std::io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c'))
            {
                return Ok(());
            }
            match key.code {
                KeyCode::Esc => return Ok(()),
                KeyCode::Enter => {
                    if app.input.trim() == "q" {
                        return Ok(());
                    }
                    app.submit();
                }
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Char(c) => app.input.push(c),
                _ => {}
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Min(8),    // body
        Constraint::Length(3), // input
        Constraint::Length(1), // help
    ])
    .split(f.area());

    f.render_widget(
        Paragraph::new("  쌍근 솔버   당근🥕 버섯🍄 마늘🧄 가지🍆 바나나🍌 사과🍎")
            .style(Style::new().fg(Color::Yellow).bold()),
        chunks[0],
    );

    let body = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    // Left: action history (guesses + pumpkin hints).
    let mut hist: Vec<ListItem> = app
        .solver
        .records
        .iter()
        .map(|r| match r {
            solver::Record::Guess(s) => {
                ListItem::new(Line::from(format!("  {}   {}", s.guess.text, clue_str(&s.clue))))
            }
            solver::Record::Hint(j) => ListItem::new(
                Line::from(format!("  🎃 {j} (정답에 있음)")).style(Style::new().fg(Color::Rgb(255, 130, 45))),
            ),
            solver::Record::CharAt(p, c) => ListItem::new(
                Line::from(format!("  📌 {}번째 글자 = {c}", p + 1)).style(Style::new().fg(Color::Green)),
            ),
            solver::Record::JamoAt(p, j) => ListItem::new(
                Line::from(format!("  📌 {}번째 글자에 {j}", p + 1)).style(Style::new().fg(Color::Green)),
            ),
        })
        .collect();
    if hist.is_empty() {
        hist.push(ListItem::new("  (아직 입력 없음)").style(Style::new().fg(Color::DarkGray)));
    }
    f.render_widget(
        List::new(hist).block(Block::default().borders(Borders::ALL).title(" 추측 기록 ")),
        body[0],
    );

    // Right: status + suggestions + sample remaining.
    let right = Layout::vertical([Constraint::Length(7), Constraint::Min(3)]).split(body[1]);

    let active = app.active();
    let n = active.remaining.len();
    let status_line = if n == 0 {
        Line::from("후보 없음 — 피드백/사전을 확인하세요".to_string())
            .style(Style::new().fg(Color::Red))
    } else if n == 1 {
        Line::from(format!("정답: {}", active.remaining_words()[0]))
            .style(Style::new().fg(Color::Green).bold())
    } else {
        Line::from(format!("남은 후보 {n}개")).style(Style::new().fg(Color::Cyan))
    };

    let mut sug_lines: Vec<Line> = vec![status_line, Line::from("")];
    let title = if app.using_wide() {
        "추천 추측 [폴백·빈도가중] *=정답가능:"
    } else {
        "추천 추측 (기대 잔여 후보) *=정답가능:"
    };
    sug_lines.push(Line::from(title).style(Style::new().bold()));
    for s in &app.suggestions {
        let mark = if s.is_candidate { "*" } else { " " };
        sug_lines.push(Line::from(format!(
            "  {mark} {}   → {:.2}",
            s.word, s.expected_remaining
        )));
    }
    f.render_widget(
        Paragraph::new(sug_lines)
            .block(Block::default().borders(Borders::ALL).title(" 분석 "))
            .wrap(Wrap { trim: true }),
        right[0],
    );

    let words = active.remaining_words_ranked();
    let sample_text = if words.is_empty() {
        "—".to_string()
    } else {
        words.join("  ")
    };
    f.render_widget(
        Paragraph::new(sample_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" 남은 후보 (총 {n}, 화면에 들어가는 만큼 표시) ")),
            )
            .wrap(Wrap { trim: true }),
        right[1],
    );

    // Input line.
    f.render_widget(
        Paragraph::new(format!("> {}", app.input)).block(
            Block::default().borders(Borders::ALL).title(
                " 관심 가지 사과 · 🎃: h ㅏ · 확정: 1복/2용(글자) 1ㅂ/2ㅇ(자모) · x 옌병(제외) · u취소 r초기화 q종료 ",
            ),
        ),
        chunks[2],
    );

    // Help / message.
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {}", app.message), Style::new().fg(Color::Gray)),
        ])),
        chunks[3],
    );
}
