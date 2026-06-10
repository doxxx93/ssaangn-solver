#!/usr/bin/env python3
"""Build the frequency weights and the noun-filtered fallback pool.

The primary answer pool (data/candidates.json, built by build_data.py) is left
as-is: expanding it only adds distractors and worsens the common case, while the
clean noun fallback already covers answers outside it.

Inputs:
  /tmp/ko_full.txt          OpenSubtitles Korean frequency (word count) — see README
  data/words.json           valid guesses (97k)
  data/candidates.json      primary pool = proven answers (handle ∪ ssaangn history)

Outputs:
  data/fallback.json        proven ∪ {all valid 2-syllable nouns} (used when primary empties)
  data/weights.json         {word: weight} over the fallback (frequency; proven floored)
  data/raw/valid_nouns.json cache of the noun-tagging (slow step) for reruns
"""
import json
import os

PROVEN_FLOOR = 40   # proven answers behave at least as common as a freq-40 word
NOUN_CACHE = 'data/raw/valid_nouns.json'

valid = set(json.load(open('data/words.json', encoding='utf-8')))
proven = set(json.load(open('data/candidates.json', encoding='utf-8')))
freq = {}
for line in open('/tmp/ko_full.txt', encoding='utf-8'):
    p = line.split()
    if len(p) == 2 and len(p[0]) == 2 and all('가' <= c <= '힣' for c in p[0]):
        freq[p[0]] = int(p[1])

# Noun-tag every valid word (slow); cache the result.
if os.path.exists(NOUN_CACHE):
    valid_nouns = set(json.load(open(NOUN_CACHE, encoding='utf-8')))
else:
    from kiwipiepy import Kiwi
    kiwi = Kiwi()
    def is_noun(w):
        t = kiwi.tokenize(w)
        return len(t) == 1 and t[0].tag in ("NNG", "NNP") and t[0].form == w
    valid_nouns = set(w for w in valid if is_noun(w))
    json.dump(sorted(valid_nouns), open(NOUN_CACHE, 'w', encoding='utf-8'), ensure_ascii=False)

fallback = sorted(proven | valid_nouns)
weights = {}
for w in fallback:
    f = freq.get(w, 0)
    weights[w] = max(f, PROVEN_FLOOR) if w in proven else f

# Drop zero-frequency non-proven nouns. They are valid *guesses* but never real
# answers (음역 고유명사/비단어 like 옌병·원평·은평): kiwipiepy noun-tagging accepts
# them, yet they only add distractors to the fallback pool. Proven answers are
# floored to PROVEN_FLOOR above so they always survive this cut.
fallback = [w for w in fallback if weights[w] > 0]
weights = {w: weights[w] for w in fallback}

json.dump(fallback, open('data/fallback.json', 'w', encoding='utf-8'), ensure_ascii=False)
json.dump(weights, open('data/weights.json', 'w', encoding='utf-8'), ensure_ascii=False)
print(f"proven(primary)={len(proven)}  valid_nouns={len(valid_nouns)}")
print(f"fallback={len(fallback)}  weights={len(weights)}")
