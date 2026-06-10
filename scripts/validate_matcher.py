#!/usr/bin/env python3
"""Validate the Rust matcher against the live ssaangn server.

Samples random (secret, guess) pairs from the known-valid ssaangn answer pool,
asks the server for the real clue, and compares to `ssaangn clue` output.

Notes
-----
- The endpoint requires a browser-like User-Agent (else HTTP 465).
- It rejects guesses outside ssaangn's own dictionary ("🐯 옳은 단어..."), so we
  draw guesses from known-valid words to get comparable pairs.
- Be polite: this hits a third-party server. Keep N modest and the sleep in place.

Usage: python3 scripts/validate_matcher.py [N]
"""
import json
import os
import random
import subprocess
import sys
import time
import urllib.parse
import urllib.request

ROOT = os.path.join(os.path.dirname(__file__), "..")
BIN = os.path.join(ROOT, "target", "release", "ssaangn")
ENDPOINT = "https://ssaangn.com/get_external_clues.php"


def server_clue(secret, guess):
    url = ENDPOINT + "?" + urllib.parse.urlencode({"secret": secret, "guess": guess})
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    for _ in range(3):
        try:
            raw = urllib.request.urlopen(req, timeout=10).read().decode("utf-8-sig")
            return json.loads(raw).get("clues")
        except Exception:
            time.sleep(0.4)
    return "FETCH_ERR"


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 160
    known = json.load(open(os.path.join(ROOT, "data", "raw", "all_answers.json"),
                           encoding="utf-8"))["answers"]
    random.seed(7)
    pairs = [(random.choice(known), random.choice(known)) for _ in range(n)]

    server = []
    for i, (s, g) in enumerate(pairs):
        server.append(server_clue(s, g))
        time.sleep(0.12)
        if (i + 1) % 40 == 0:
            print(f"fetched {i + 1}/{n}", flush=True)

    inp = "\n".join(f"{s} {g}" for s, g in pairs) + "\n"
    rust = subprocess.run([BIN, "clue"], input=inp, capture_output=True, text=True).stdout.strip().split("\n")

    mism, valid, rej, err = [], 0, 0, 0
    for (s, g), sv, ru in zip(pairs, server, rust):
        if sv == "FETCH_ERR":
            err += 1
        elif sv.startswith("🐯"):
            rej += 1
        else:
            valid += 1
            if sv != ru:
                mism.append((s, g, sv, ru))

    print(f"\ncompared={valid} rejected={rej} fetch_err={err}")
    print(f"MISMATCHES: {len(mism)}")
    for s, g, sv, ru in mism[:25]:
        print(f"  secret={s} guess={g} server={sv} rust={ru}")
    sys.exit(1 if mism else 0)


if __name__ == "__main__":
    main()
