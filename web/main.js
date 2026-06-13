import init, { WasmSolver } from './pkg/ssaangn_solver.js';

// 힌트 6종. digit은 Rust `Clue::parse`가 받는 토큰("1"~"6").
const CLUES = [
  { d: '1', e: '🥕', name: '당근' },
  { d: '2', e: '🍄', name: '버섯' },
  { d: '3', e: '🧄', name: '마늘' },
  { d: '4', e: '🍆', name: '가지' },
  { d: '5', e: '🍌', name: '바나나' },
  { d: '6', e: '🍎', name: '사과' },
];
const EMOJI = Object.fromEntries(CLUES.map((c) => [c.d, c.e]));
const OPENING = '관심';

const $ = (id) => document.getElementById(id);
const guessEl = $('guess');
const applyEl = $('apply');
const msgEl = $('msg');

let solver = null;
let sel = [null, null]; // 글자별 선택된 힌트 digit
let log = [];           // 입력 기록: {kind:'guess',word,c0,c1} | {kind:'hint',jamo}

function buildEmojiRows() {
  for (let s = 0; s < 2; s++) {
    const row = document.querySelector(`.emoji-row[data-row="${s}"]`);
    CLUES.forEach((c) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'emoji-btn';
      b.dataset.d = c.d;
      b.title = c.name;
      b.innerHTML = `<span class="e">${c.e}</span><span class="n">${c.name}</span>`;
      b.addEventListener('click', () => {
        sel[s] = sel[s] === c.d ? null : c.d;
        row.querySelectorAll('.emoji-btn').forEach((x) =>
          x.classList.toggle('sel', x.dataset.d === sel[s])
        );
        $(`ch${s}`).textContent = sel[s] ? EMOJI[sel[s]] : '고르기';
        refreshApply();
      });
      row.appendChild(b);
    });
  }
}

function clearClues() {
  sel = [null, null];
  document.querySelectorAll('.emoji-btn').forEach((x) => x.classList.remove('sel'));
  $('ch0').textContent = '고르기';
  $('ch1').textContent = '고르기';
}

function validGuess() {
  return /^[가-힣]{2}$/.test(guessEl.value.trim());
}

function refreshApply() {
  applyEl.disabled = !(validGuess() && sel[0] && sel[1]);
}

function setMsg(text, isErr = false) {
  msgEl.textContent = text;
  msgEl.classList.toggle('err', isErr);
}

function renderHistory() {
  const card = $('historyCard');
  const list = $('history');
  card.classList.toggle('hidden', log.length === 0);
  list.innerHTML = '';
  log.forEach((m) => {
    const li = document.createElement('li');
    if (m.kind === 'guess') {
      li.innerHTML =
        `<span class="hw">${m.word}</span>` +
        `<span class="he">${EMOJI[m.c0]}${EMOJI[m.c1]}</span>`;
    } else {
      li.innerHTML = `<span class="hw">🎃</span><span class="tag">자모 “${m.jamo}” 포함</span>`;
    }
    list.appendChild(li);
  });
}

// 쌍근 게임과 같은 두벌식 키보드 배열 (쌍자음/ㅒㅖ는 윗줄). 자음 19 + 모음 14
// 전부 포함하며, Rust state().jamo의 키와 동일.
const JAMO_ROWS = [
  ['ㅃ', 'ㅉ', 'ㄸ', 'ㄲ', 'ㅆ', 'ㅒ', 'ㅖ'],
  ['ㅂ', 'ㅈ', 'ㄷ', 'ㄱ', 'ㅅ', 'ㅛ', 'ㅕ', 'ㅑ', 'ㅐ', 'ㅔ'],
  ['ㅁ', 'ㄴ', 'ㅇ', 'ㄹ', 'ㅎ', 'ㅗ', 'ㅓ', 'ㅏ', 'ㅣ'],
  ['ㅋ', 'ㅌ', 'ㅊ', 'ㅍ', 'ㅠ', 'ㅜ', 'ㅡ'],
];

function renderJamo(jamo) {
  const wrap = $('jamoWrap');
  const board = $('jamoBoard');
  // jamo가 비어있으면(추측 전) 보드 숨김.
  if (!jamo || Object.keys(jamo).length === 0) {
    wrap.hidden = true;
    return;
  }
  wrap.hidden = false;
  board.innerHTML = '';
  JAMO_ROWS.forEach((row) => {
    const r = document.createElement('div');
    r.className = 'jamo-row';
    row.forEach((j) => {
      const k = document.createElement('span');
      k.className = `jkey ${jamo[j] || 'maybe'}`;
      k.textContent = j;
      r.appendChild(k);
    });
    board.appendChild(r);
  });
}

function render() {
  const st = JSON.parse(solver.state());

  $('remCount').textContent = `${st.remaining_count}개`;
  $('wideBadge').classList.toggle('hidden', !st.using_wide);
  renderJamo(st.jamo);

  // 추천 배너
  const recWord = $('recWord');
  const recNote = $('recNote');
  if (log.length === 0) {
    recWord.textContent = OPENING;
    recWord.classList.remove('none');
    recNote.textContent = '첫 추측으로 추천하는 오프닝이에요. 누르면 아래 추측칸에 채워집니다.';
  } else if (st.suggestions.length > 0) {
    recWord.textContent = st.suggestions[0].word;
    recWord.classList.remove('none');
    recNote.textContent = '누르면 추측칸에 채워집니다.';
  } else {
    recWord.textContent = st.remaining_count === 0 ? '일치 없음' : '—';
    recWord.classList.add('none');
    recNote.textContent =
      st.remaining_count === 0 ? '입력한 힌트를 다시 확인해 보세요.' : '';
  }

  // 남은 후보
  const rem = $('remaining');
  rem.innerHTML = '';
  if (st.remaining.length === 0) {
    rem.innerHTML = '<span class="empty">남은 후보가 없어요.</span>';
  } else {
    st.remaining.forEach((w) => {
      const c = document.createElement('button');
      c.type = 'button';
      c.className = 'chip';
      c.textContent = w;
      c.title = '누르면 추측칸에 채워집니다';
      c.addEventListener('click', () => fillGuess(w));
      rem.appendChild(c);
    });
    const hidden = st.remaining_count - st.remaining.length;
    if (hidden > 0) {
      const more = document.createElement('span');
      more.className = 'more';
      more.textContent = `+${hidden}개 더`;
      rem.appendChild(more);
    }
  }

  // 추천 후보
  const sg = $('suggestions');
  sg.innerHTML = '';
  if (st.suggestions.length === 0) {
    sg.innerHTML = '<li class="empty">추천할 후보가 없어요.</li>';
  } else {
    st.suggestions.forEach((s, i) => {
      const li = document.createElement('li');
      li.title = '누르면 추측칸에 채워집니다';
      li.innerHTML =
        `<span class="rank">${i + 1}</span>` +
        `<span class="w">${s.word}</span>` +
        (s.is_candidate ? '<span class="cand">정답 후보</span>' : '') +
        `<span class="meta">예상 잔여 ${s.expected.toFixed(1)}</span>`;
      li.addEventListener('click', () => fillGuess(s.word));
      sg.appendChild(li);
    });
  }

  renderHistory();
}

function fillGuess(word) {
  guessEl.value = word;
  refreshApply();
  guessEl.scrollIntoView({ behavior: 'smooth', block: 'center' });
  guessEl.focus();
}

function doApply() {
  if (applyEl.disabled) return;
  const word = guessEl.value.trim();
  try {
    solver.guess(word, sel[0], sel[1]);
  } catch (e) {
    setMsg(String(e), true);
    return;
  }
  log.push({ kind: 'guess', word, c0: sel[0], c1: sel[1] });
  guessEl.value = '';
  clearClues();
  refreshApply();
  render();
  setMsg(`“${word}” 기록됨.`);
}

function doPumpkin() {
  const j = $('pumpkin').value.trim();
  if (!j) return;
  try {
    solver.hint(j);
  } catch (e) {
    setMsg(String(e), true);
    return;
  }
  log.push({ kind: 'hint', jamo: j });
  $('pumpkin').value = '';
  render();
  setMsg(`🎃 호박 힌트 “${j}” 적용됨.`);
}

async function main() {
  await init();
  solver = new WasmSolver();

  buildEmojiRows();
  render();

  guessEl.addEventListener('input', refreshApply);
  guessEl.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') doApply();
  });
  applyEl.addEventListener('click', doApply);
  $('recWord').addEventListener('click', () => {
    if (!$('recWord').classList.contains('none')) fillGuess($('recWord').textContent);
  });
  $('undo').addEventListener('click', () => {
    if (log.length === 0) return;
    solver.undo();
    log.pop();
    render();
    setMsg('직전 입력을 되돌렸어요.');
  });
  $('reset').addEventListener('click', () => {
    solver.reset();
    log = [];
    guessEl.value = '';
    clearClues();
    refreshApply();
    render();
    setMsg('초기화했어요.');
  });
  $('pumpkin').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') doPumpkin();
  });
  $('pumpkinApply').addEventListener('click', doPumpkin);
}

main().catch((e) => setMsg('로드 실패: ' + e, true));
