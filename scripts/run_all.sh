#!/usr/bin/env bash
# Regenerate every result (E1-E6) and every figure and table from fixed seeds (about 6 minutes).
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -n "$(git status --porcelain -- ':(top,exclude)results')" ]; then
    echo "warning: uncommitted changes outside results/; meta.json files will say \"dirty\": true" >&2
fi

echo "== build"
(cd contracts && forge build)
cargo build --release -p btp-sim -p btp-attacks -p btp-client

echo "== E1 order-independence (about 10 s)"
cargo run --release -q -p btp-sim -- --experiment e1 --cases 100000 --seed 1
echo "== E6 attacks and rank test (about 40 s)"
cargo run --release -q -p btp-attacks --bin e6
echo "== E4 gas on anvil (about 1 min)"
cargo run --release -q -p btp-client -- e4
echo "== E5 load on anvil (about 2.5 min)"
cargo run --release -q -p btp-client -- e5
echo "== E2 concurrency benefit (reads results/e4; about 10 s)"
cargo run --release -q -p btp-sim -- --experiment e2
echo "== E3 overlap profile (about 1 s)"
cargo run --release -q -p btp-sim -- --experiment e3

echo "== figures and tables"
PY=python3
[ -x .venv/bin/python ] && PY=.venv/bin/python
"$PY" scripts/plot.py

echo "== meta.json dirty flags (all should be false)"
grep -h '"dirty"' results/*/*.meta.json | sort | uniq -c
