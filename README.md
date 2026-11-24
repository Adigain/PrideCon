# Concurrent PriDeFUL CT with tagged cross-term handles

PriDeFUL CT gives forward secrecy to account-based confidential payments by letting each sender evolve
the receiver's key, but two payments that update the same receiver in one round cannot both be applied.
This repository implements the construction that removes that restriction, **tagged cross-term handles**
on BLS12-381: concurrent payments to the same account commute, the account stays decryptable under the
evolved key, and the construction is forward-secure under the TD-DDH assumption.

The implementation has four parts:

- a Rust library with the full scheme (setup, transact, verify, apply, key update, balance read), generic
  over the algebra so it runs on an exact exponent backend and on real BLS12-381;
- Solidity contracts that run the scheme on an EVM chain through the EIP-2537 precompiles, plus a
  PriDeFUL CT baseline;
- a client that deploys and loads the contracts on a local anvil chain;
- a simulator, an attack harness and the experiments E1–E6.

## Documents

| File | Contents |
|---|---|
| `docs/PriDeFUL_CT_concurrency_plan.pdf` | the theory: diagnosis, flawed designs, the construction and its proofs |
| `docs/tct_proof.pdf` | the construction and the security proof in compact form |
| `SPEC.md` | what the code computes and how it encodes it (byte layouts, hashes, checks, simplifications) |
| `reference/` | exponent-level Python models the Rust code is checked against |

## Layout

```
crates/btp-algebra    backend trait; exponent backends (61-bit, BLS12-381 scalar field); BLS12-381; EIP-2537 codec
crates/btp-protocol   the scheme (SPEC 5-12): native and mirror ledgers, wallet, workloads, ABI, fixture generator
crates/btp-attacks    flawed designs, attacks A1-A5 and the generic-group rank test (E6)
crates/btp-sim        order-independence fuzzing (E1) and the conflict-model simulator (E2, E3)
crates/btp-client     anvil client: gas (E4) and concurrent load against the baseline (E5)
contracts/            Foundry project: BLS12381.sol, Encode.sol, PrideConcurrent.sol, PriDeFULBaseline.sol
fixtures/             JSON test vectors written by Rust and replayed by the Solidity tests
results/              experiment outputs (CSV + meta.json per run) and figures/
scripts/              run_all.sh, plot.py, anvil launchers
```

## Requirements

Rust (stable), Foundry (`forge`, `anvil`), Git, and Python 3 for the plots and reference models.

```bash
git submodule update --init --recursive
python3 -m venv .venv && .venv/bin/pip install -r scripts/requirements.txt
```

## Build and test

```bash
cargo build --workspace
cargo test --workspace
cd contracts && forge build && forge test && cd ..
```

The Rust tests check the scheme against the SPEC and the reference models: exponent-level field checks,
tampering and spend checks, native-versus-mirror ledgers, order-independence, and the attack harness.
The Solidity tests replay the Rust fixtures and require the contract's state to equal Rust's after every
operation.

To regenerate the fixtures:

```bash
cargo run -p btp-protocol --bin gen-fixtures
```

## Reproducing the results

One command rebuilds everything and regenerates every result, figure and table from fixed seeds
(about six minutes):

```bash
bash scripts/run_all.sh
```

| Experiment | What it measures | Command | Output |
|---|---|---|---|
| E1 | order-independence over random workloads and orders | `cargo run --release -p btp-sim -- --experiment e1 --cases 100000` | `results/e1/` |
| E2 | concurrency benefit against four baselines (conflict model) | `cargo run --release -p btp-sim -- --experiment e2` | `results/e2/` |
| E3 | credits per account per epoch | `cargo run --release -p btp-sim -- --experiment e3` | `results/e3/` |
| E4 | gas per operation on anvil | `cargo run --release -p btp-client -- e4` | `results/e4/` |
| E5 | concurrent senders per block, ours vs the baseline | `cargo run --release -p btp-client -- e5` | `results/e5/` |
| E6 | attacks on each design; generic-group rank test | `cargo run --release -p btp-attacks --bin e6` | `results/e6/` |

E2 reads the E4 gas results, so E4 must run first. Each run writes a CSV and a `meta.json` with the seed,
the parameters and the git commit. `scripts/plot.py` turns the CSVs into the figures and tables in
`results/figures/`.

The client starts its own anvil (Prague hardfork, 100M gas per block). To use one started separately,
run `scripts/anvil.sh` (or `scripts/anvil.ps1`) and pass `--rpc http://127.0.0.1:8545`.

## Scope

Proof level L0: the contracts run every check of the scheme except the zero-knowledge proofs, whose
costs are taken from the PriDe CT paper. This and the other simplifications are listed in `SPEC.md`,
Section 14.
