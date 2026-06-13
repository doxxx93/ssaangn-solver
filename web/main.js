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

let remExpanded = false; // 남은 후보 칩 펼침 여부

// 펼침 토글만 반영 (솔버 재계산 없이 표시만 갱신).
function updateRemView() {
  $('remToggle').textContent = remExpanded ? '접기 ▴' : '모두 보기 ▾';
  $('remNote').hidden = !remExpanded;
  $('remaining').hidden = !remExpanded;
}

function render() {
  const st = JSON.parse(solver.state());

  $('remCount').textContent = `${st.remaining_count}개`;
  $('wideBadge').classList.toggle('hidden', !st.using_wide);
  // 추천 헤더에 남은 후보 수를 같이 보여줘 진행 상황이 화면 상단에서 바로 보이게.
  $('recCount').textContent = log.length > 0 ? `· 남은 ${st.remaining_count}개` : '';

  // 다음에 칠 단어 (메인 추천). 정답이 아닌 단어라도 후보를 가장 많이 줄이면 위로.
  const sg = $('suggestions');
  sg.innerHTML = '';
  if (log.length === 0) {
    // 첫 수: 데이터로 검증된 추천 오프닝(관심). 한 줄 전체폭.
    const li = document.createElement('li');
    li.className = 'full';
    li.title = '누르면 추측칸에 채워집니다';
    li.innerHTML =
      '<span class="rank">1</span>' +
      `<span class="w">${OPENING}</span>` +
      '<span class="meta">추천 오프닝</span>';
    li.addEventListener('click', () => fillGuess(OPENING));
    sg.appendChild(li);
  } else if (st.suggestions.length === 0) {
    sg.innerHTML =
      '<li class="empty full">일치하는 단어가 없어요. 입력한 힌트를 다시 확인해 보세요.</li>';
  } else {
    const n = st.remaining_count;
    // 상위 6개만 — 그 아래는 후보 축소량이 거의 같아 변별이 없음. 전체 가능 단어는 '남은 후보'에.
    st.suggestions.slice(0, 6).forEach((s, i) => {
      // 후보가 적게 남을수록 변별이 중요하므로 10개 미만은 소수 1자리로.
      const after = s.expected < 9.95 ? s.expected.toFixed(1) : Math.round(s.expected);
      const li = document.createElement('li');
      li.title = '누르면 추측칸에 채워집니다';
      li.innerHTML =
        `<span class="rank">${i + 1}</span>` +
        `<span class="w">${s.word}</span>` +
        // 추천은 전부 정답 가능 후보라 뱃지 생략. 메타는 컴팩트하게 "→ N개"(치면 남을 후보 수).
        // 후보 2개 이하면 노이즈라 생략.
        (n > 2 ? `<span class="meta">→ ${after}개</span>` : '');
      li.addEventListener('click', () => fillGuess(s.word));
      sg.appendChild(li);
    });
  }

  // 호박 힌트 최적 타이밍 안내: 첫 추측 직후(추측 1개, 힌트 아직 없음)에만 노출.
  const guesses = log.filter((m) => m.kind === 'guess').length;
  const usedHint = log.some((m) => m.kind === 'hint');
  $('pumpkinTip').hidden = !(guesses === 1 && !usedHint);

  // 남은 후보 (정답 가능 단어들). 기본 접힘, '모두 보기'로 펼침.
  const hasRem = log.length > 0 && st.remaining_count > 0;
  $('remToggle').hidden = !hasRem;
  if (!hasRem) {
    remExpanded = false;
  }
  const rem = $('remaining');
  rem.innerHTML = '';
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
  updateRemView();

  renderHistory();
}

function fillGuess(word) {
  guessEl.value = word;
  refreshApply();
  guessEl.scrollIntoView({ behavior: 'smooth', block: 'center' });
  guessEl.focus();
}

// 적용/힌트 후, 새 추천이 화면 바닥에 묻히지 않도록 추천 영역을 시야에 올림.
function scrollToNext() {
  $('suggestions').scrollIntoView({ behavior: 'smooth', block: 'center' });
}

function doApply() {
  if (applyEl.disabled) return;
  const word = guessEl.value.trim();
  const clue = `${EMOJI[sel[0]]}${EMOJI[sel[1]]}`; // 방금 친 단어+힌트를 메시지에 남김
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
  setMsg(`“${word}” ${clue} 기록됨.`);
  scrollToNext();
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
  scrollToNext();
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
  $('remToggle').addEventListener('click', () => {
    remExpanded = !remExpanded;
    updateRemView();
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
    remExpanded = false;
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
