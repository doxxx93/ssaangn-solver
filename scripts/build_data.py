#!/usr/bin/env python3
"""Rebuild data/candidates.json, data/words.json, data/recent_answers.json
from the raw sources in data/raw/.

Sources
-------
- data/raw/sheet_*.csv      : ssaangn answer calendars (Google Sheet, 3 tabs)
- data/raw/comments.json    : DCInside thread comments (MMDD word) for May 2026
- data/raw/handle_answers.json / handle_words.json : the 한들(handle) game
  dictionaries — {"2": [...]} 2-syllable lists. Used as a broad common-word pool.

Outputs
-------
- data/candidates.json : answer pool the solver searches (handle answers ∪ ssaangn history)
- data/words.json      : valid-guess set (handle words ∪ candidates)
- data/recent_answers.json : {"may2026": [[MMDD, word], ...]}
"""
import csv
import json
import os
import re

RAW = os.path.join(os.path.dirname(__file__), "..", "data", "raw")
OUT = os.path.join(os.path.dirname(__file__), "..", "data")


def valid(w):
    return isinstance(w, str) and len(w) == 2 and all("가" <= c <= "힣" for c in w)


def load(fn):
    with open(os.path.join(RAW, fn), encoding="utf-8-sig") as f:
        return json.load(f)


def words_from_sheet(fn):
    out = set()
    with open(os.path.join(RAW, fn), encoding="utf-8") as f:
        for row in csv.reader(f):
            for cell in row[1:]:  # column 0 is the day label
                out.update(re.findall(r"[가-힣]{2}", cell))
    return out


def main():
    # ssaangn historical answers from the 3 sheet tabs
    ss = set()
    for fn in os.listdir(RAW):
        if fn.startswith("sheet_") and fn.endswith(".csv"):
            ss |= words_from_sheet(fn)

    # DCInside May-2026 daily answers: "MMDD word"
    comments = load("comments.json").get("comments", [])
    pairs = []
    for c in comments:
        if c.get("is_delete") == "1":
            continue
        m = re.match(r"\s*(\d{4})\s+([가-힣]{2})\s*$", c.get("memo", "").strip())
        if m:
            pairs.append([m.group(1), m.group(2)])
            ss.add(m.group(2))

    ss = {w for w in ss if valid(w)}

    h_ans = {w for w in load("handle_answers.json").get("2", []) if valid(w)}
    h_words = {w for w in load("handle_words.json").get("2", []) if valid(w)}

    candidates = sorted(h_ans | ss)
    words = sorted(h_words | set(candidates))

    json.dump(candidates, open(os.path.join(OUT, "candidates.json"), "w", encoding="utf-8"),
              ensure_ascii=False)
    json.dump(words, open(os.path.join(OUT, "words.json"), "w", encoding="utf-8"),
              ensure_ascii=False)
    json.dump({"may2026": pairs},
              open(os.path.join(OUT, "recent_answers.json"), "w", encoding="utf-8"),
              ensure_ascii=False)

    print(f"ssaangn history: {len(ss)}  handle answers: {len(h_ans)}")
    print(f"candidates: {len(candidates)}  valid words: {len(words)}  recent pairs: {len(pairs)}")


if __name__ == "__main__":
    main()
