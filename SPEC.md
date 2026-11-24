# SPEC — Concurrent PriDeFUL CT with tagged cross-term handles

This document is the specification the implementation follows: the Rust crates and the Solidity
contracts both implement it, and the tests check that they agree with it and with each other.

The maths behind it is in `docs/PriDeFUL_CT_concurrency_plan.pdf` (Section 3.4 and on). This file only
says *what to compute and how to encode it*.

---

## 1. Notation

Code uses **additive notation** for every group, including GT.

| Paper | Code | Meaning |
|---|---|---|
| `[a]_1 = g1^a` | `g1() * a` | element of G1 |
| `[a]_2 = g2^a` | `g2() * a` | element of G2 |
| `[a]_T = e(g1,g2)^a` | `gt() * a` | element of GT |
| `X · Y` (group product) | `X + Y` | group operation |
| `X / Y` | `X - Y` | |
| `X^s` | `X * s` | scalar multiplication |
| `e(A, B)` | `pair(A, B)` | pairing, bilinear: `pair(A*a, B*b) = gt()*(a*b)` |

`Fr` is the scalar field (order `r` of the groups). All scalars live in `Fr`.

## 2. Parameters

| Name | Value | Notes |
|---|---|---|
| Curve | BLS12-381 | |
| `MAX` | 2^32 − 1 | balances and payloads lie in `[0, MAX]` |
| `L` | 16 blocks (configurable) | epoch length on chain |
| `K_MAX` | 8 (configurable) | max credits per account per epoch; extra credits revert |
| Ring size `n` | 1..32 | number of receivers per transaction (real + decoys) |

Epoch on chain: `e = block.number / L + 1` (integer division). Epochs start at 1, so the value 0 means
"never" in fields such as `debited_epoch`. In the simulator, epochs are logical counters starting at 1.

## 3. Byte encodings (BLS12-381, EIP-2537)

| Object | Bytes | Encoding |
|---|---|---|
| `Fr` scalar | 32 | big-endian integer `< r` |
| `Fp` base field element | 64 | big-endian, top 16 bytes zero |
| G1 point | 128 | `x ‖ y`, each 64-byte Fp; point at infinity = 128 zero bytes |
| G2 point | 256 | `x.c0 ‖ x.c1 ‖ y.c0 ‖ y.c1`, each 64-byte Fp (c0 first); infinity = 256 zero bytes |
| `u32` / `u64` | 4 / 8 | big-endian |

In Solidity, a G1 point is `bytes32[4]` = `[x_hi, x_lo, y_hi, y_lo]` and a G2 point is `bytes32[8]`.
The 64-byte `Fp` is the pair `(hi, lo)` of words where `hi` carries the top 16 zero bytes.

Constants (checked in `reference/bls_consts.py`):
- `p  = 0x1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab`
- `r  = 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001`

Precompiles (EIP-2537): `G1ADD 0x0b`, `G1MSM 0x0c`, `G2ADD 0x0d`, `G2MSM 0x0e`, `PAIRING_CHECK 0x0f`.
There is no single-multiplication precompile: use `G1MSM` with one pair. `G1MSM` and `PAIRING_CHECK`
do subgroup checks and fail on bad points; `G1ADD` does not.

**Encoding pitfall:** arkworks serialises points compressed and little-endian by default. Always go
through `btp_algebra::codec`, never through `ark_serialize`, when producing bytes for the EVM.

**Exponent backends.** `Exp61` and `ExpBls` have no curve points. For hashing only, they encode a G1 or
G2 element as the 32-byte big-endian integer of its discrete log (the same width for both groups). These
bytes never go to the EVM. As a result, tags and weights differ between `ExpBls` and `Bls` for the "same"
transaction, so the backends are cross-checked on identities, not byte for byte.

## 4. Hashing

All hashes are `keccak256`. `H(tag, data) = keccak256(tag_bytes ‖ data)`, interpreted as a big-endian
integer and reduced mod `r` when a scalar is needed.

| Use | Domain tag (ASCII) | Input |
|---|---|---|
| Transaction tag `t` | `BTP/tag/v1` | `enc(core)` (Section 7.3) |
| Batch weights `w_i` | `BTP/w/v1` | `txhash ‖ u32(i)` where `txhash = keccak256(enc(core) ‖ enc(handles) ‖ g1(N0))` (Section 7.3) |
| Record hash | `BTP/rec/v1` | `enc(record)` (Section 8) |
| δ side channel | `BTP/side/v1` | `enc_g1(ρ · pk_base)` |
| Fiat–Shamir (L1) | `BTP/fs/v1` | statement ‖ commitments |
| CRS from seed (dev only) | `BTP/crs/v1` | `seed ‖ u8(i)`, `i = 0, 1, 2` for `β, η, κ` |

## 5. Setup and CRS

Trapdoor `β, η, κ ← Fr`. Publish:

- `crs1 = ( g1, g1·β, g1·β², g1·η, g1·ηβ, g1·κ, g1·κβ )` in G1
- `crs2 = ( g2, g2·β, g2·β², g2·η, g2·κ )` in G2

Derived for tag `t` and epoch `e` (anyone can compute):
- `phi1(t,e) = crs1.β·t + crs1.η + crs1.κ·e`       (= `[φ]_1`)
- `bphi1(t,e) = crs1.β²·t + crs1.ηβ + crs1.κβ·e`   (= `[βφ]_1`)
- `phi2(t,e) = crs2.β·t + crs2.η + crs2.κ·e`       (= `[φ]_2`)

**Simplification S1 (dev setup):** the trapdoor is derived from a seed, `β = H(BTP/crs/v1, seed ‖ u8(0))`,
`η = H(BTP/crs/v1, seed ‖ u8(1))`, `κ = H(BTP/crs/v1, seed ‖ u8(2))`, so runs are reproducible. In a real deployment it would come from a ceremony and be erased. The attack harness
may use the trapdoor to demonstrate Remark 3 (trapdoor leak breaks privacy).

## 6. Keys and accounts

- Secret key `s ∈ Fr`; public key `pk = g1·s`.
- A fresh account with balance `b` is `C = g1·x`, `D = g1·(x·s + b)` for random `x`.
- Registration: `register(pk, C, D)`. The owner supplies an encryption of 0 with fresh random `x`
  (`C = g1·x`, `D = pk·x`). Starting from `C = D = 0` is not allowed: then `x = 0` is public, and
  `reference/tdddh.py` ("epoch-start randomness x known") shows that leaks the first epoch's payloads.
  The contract checks only that `C ≠ 0`; it cannot check that `x` is secret.

### 6.1 Account state

| Field | Group | Meaning |
|---|---|---|
| `pk_base` | G1 | public key valid for the current epoch |
| `C, D` | G1, G1 | live balance ciphertext (G1 part) |
| `C_base, D_base` | G1, G1 | snapshot taken at the first touch in the current epoch |
| `dPK` | G1 | sum of key offsets `g1·δ` received this epoch |
| `up_agg` | G1² | sum of update ciphertexts received this epoch |
| `snap_epoch` | u64 | epoch of the snapshot |
| `credits_this_epoch` | u32 | for the `K_MAX` cap |
| `debited_epoch` | u64 | last epoch with a debit (one debit per epoch) |
| `open_epochs` | list of u64 | epochs whose handle records are not yet normalised |
| `chain[e]` | bytes32 | hash chain of handle records of epoch `e` |
| `W` (native only) | GT | live cross-term accumulator; on chain it is *deferred*: the records stand in for it |

Balance invariant (native ledger): `pair(D - C·s, g2) + W = gt()·b`.

### 6.2 Epoch rollover (lazy, on the first touch of an account in a new epoch `e`)

1. `pk_base ← pk_base + dPK`
2. emit `KeyUpdated(account, snap_epoch, dPK, up_agg)` so the owner can update its key
3. `dPK ← 0`, `up_agg ← (0,0)`, `credits_this_epoch ← 0`
4. `C_base ← C`, `D_base ← D`, `snap_epoch ← e`
5. (native only) `W_base ← W`

The owner's key for epoch `e` is `s_e = s_{e-1} + Δ` where `g1·Δ = dPK` of the epoch that just closed.

## 7. Transact

Sender `S` (key `s0`, account 0) pays `pl_j ≥ 0` to ring members `j = 1..n` in epoch `e`.
Decoys get `pl_j = 0`. Total debit `pl0 = Σ pl_j`.

### 7.1 Steps

1. Roll over every touched account (Section 6.2) in the sender's local view.
2. `r ← Fr`. `C = g1·r`. `D0 = pk_base0·r - g1·pl0`. For each `j`: `D_j = pk_base_j·r + g1·pl_j`.
3. For each `j`: `δ_j, ρ_j, τ_j ← Fr`;
   `up_j = ( g1·ρ_j , pk_base_j·ρ_j + g1·δ_j )`; `dpk_j = g1·δ_j`; `E_j = (C_base_j + C)·δ_j`;
   `side_j = δ_j XOR H(BTP/side/v1, enc_g1(pk_base_j·ρ_j))` (32 bytes) — see S2.
4. `core = (e, sender, ring, C, D0, [D_j], [up_j], [dpk_j], [E_j], [side_j])`; `t = H(BTP/tag/v1, enc(core))`.
5. Randomness handles: `R1 = crs1.β·r`, `R2 = bphi1(t,e)·r`.
6. Update handles per `j` (all in G2): `H0_j = g2·δ_j + crs2.β²·τ_j`, `H1_j = phi2(t,e)·τ_j`,
   `H2_j = g2·τ_j`.
7. Normalised balance: the sender knows its snapshot balance `b0` and key `s0`;
   `bal' = b0 - pl0` (must be `≥ 0`); `N0 = (C_base0 + C)·s0 + g1·bal'`.
8. Spend witness: the sender's handle records of all `open_epochs < e` (from events), plus
   inverse hints `h[m][i] = (t_m - t_i)^{-1} mod r` for every ordered pair in the same epoch with `m` an
   update record (Section 9).
9. Proof `π`: none at L0 (S3). L1: Section 11.

**Simplification S2 (δ delivery):** recovering `δ` from `g1·δ` is a discrete log, infeasible for random `δ`.
The paper itself mentions hashed ElGamal as the practical alternative. The receiver decrypts `side_j`
with `pk_base_j·ρ_j = u1·s` (it knows `s` and `u1 = g1·ρ_j`), then checks `g1·δ_j == dpk_j`.
`up_j` is still posted and checked, so the published state is exactly the scheme in the plan.

### 7.2 Transaction structure (Rust and ABI)

```
Tx {
  epoch: u64, sender: u32, ring: Vec<u32>,
  C: G1, D0: G1, N0: G1, R1: G1, R2: G1,
  recv: Vec<Recv>,             // one per ring member, same order as ring
  spend: SpendWitness,         // Section 9
  proof: Option<Proof>,        // L1 only
}
Recv { D: G1, up: (G1, G1), dpk: G1, E: G1, side: [u8;32], H0: G2, H1: G2, H2: G2 }
```

### 7.3 `enc(core)`

`u64(e) ‖ u32(sender) ‖ u32(n) ‖ u32(ring_1)..u32(ring_n) ‖ g1(C) ‖ g1(D0) ‖` then for each `j` in order:
`g1(D_j) ‖ g1(up_j.0) ‖ g1(up_j.1) ‖ g1(dpk_j) ‖ g1(E_j) ‖ side_j`.

`enc(handles) = g1(R1) ‖ g1(R2) ‖` then for each `j`: `g2(H0_j) ‖ g2(H1_j) ‖ g2(H2_j)`; and `N0` is appended
after the handles: `txhash = keccak256(enc(core) ‖ enc(handles) ‖ g1(N0))`.

## 8. Handle records

A record is what one transaction leaves at one account:
`Record { t: Fr, C: G1, R1: G1, R2: G1, H0: G2, H1: G2, H2: G2, is_update: bool }`.
At a receiver `is_update = true`. At the sender, `is_update = false` and `H0 = H1 = H2 = 0`.

`enc(record) = fr(t) ‖ g1(C) ‖ g1(R1) ‖ g1(R2) ‖ g2(H0) ‖ g2(H1) ‖ g2(H2) ‖ u8(is_update)`.
`chain[e] ← keccak256(chain[e] ‖ H(BTP/rec/v1, enc(record)))`, starting from `bytes32(0)`.
The full record is emitted in an event `Record(account, epoch, record)`; only the chain hash is stored.

### 8.1 Cross-term (native ledger)

For records `i ≠ m` of the same account and epoch, with `m.is_update`:

`X(i→m) = pair(C_i, H0_m) - h·pair(R1_i, H1_m) + h·pair(R2_i, H2_m)`, with `h = (t_m - t_i)^{-1}`.

It equals `gt()·(r_i·δ_m)`. If `m` is not an update record, `X(i→m) = 0`.
Applying a new record `m` to an account adds `X(new→old) + X(old→new)` over all records `old` of the
same epoch already there.

## 9. Verify (contract and Rust mirror)

Reject unless all hold:

1. `e` equals the current epoch; every ring member is registered; ring members are distinct and do
   not include the sender.
2. `t = H(BTP/tag/v1, enc(core))` has not been used before (global set).
3. `debited_epoch(sender) ≠ e`; `credits_this_epoch(j) < K_MAX` for every `j`.
4. **Batched V1–V4** — one `PAIRING_CHECK` with `4 + 3n` pairs. Weights `w1, w2, w3_j, w4_j` from
   `H(BTP/w/v1, txhash ‖ u32(i))` with `i = 0, 1, 2+2j, 3+2j`, where `j = 0..n-1` is the 0-based position
   in the ring (so `i` runs over `0..2n+1` with no gaps). With `φ1 = phi1(t,e)`:

   | G2 element | G1 element paired with it |
   |---|---|
   | `g2` | `R1·w1 + R2·w2 - Σ_j dpk_j·w3_j` |
   | `crs2.β` | `-(C·w1) - R1·(w2·t)` |
   | `crs2.η` | `-(R1·w2)` |
   | `crs2.κ` | `-(R1·(w2·e))` |
   | `H0_j` | `g1·w3_j` |
   | `H1_j` | `g1·w4_j` |
   | `H2_j` | `-(crs1.β²·w3_j) - φ1·w4_j` |

   Accept iff the product of all pairings is 1. (V1: `e(R1,g2)=e(C,[β]_2)`; V2:
   `e(R2,g2)=e(t·R1,[β]_2)e(R1,[η]_2)e(e·R1,[κ]_2)`; V3: `e(g1,H0)=e(dpk,g2)e([β²]_1,H2)`;
   V4: `e(g1,H1)=e([φ]_1,H2)`.) Only G1 arithmetic is needed, so this also works on BN254.
5. **V5 spend check, deferred form.** Inputs: the sender's records of every open epoch `< e`
   (checked against `chain[·]`), and the hints `h`. The witness must contain every epoch in `open_epochs`
   that is `< e`, each with exactly the records hashed into `chain[epoch]`. Witness entries for any other
   epoch are ignored: they belong to epochs a normalisation (debit or compaction) earlier in this epoch has
   already dropped, so a debit and a compaction of one account in one epoch succeed in either order.
   For every update record `m` with at least one
   other record `i` in its epoch:
   - check `h[m][i] · (t_m - t_i) ≡ 1 (mod r)` with `mulmod`;
   - `S_m = Σ_{i≠m} C_i`, `A_m = Σ_{i≠m} -h[m][i]·R1_i`, `B_m = Σ_{i≠m} h[m][i]·R2_i` (G1MSM).

   One `PAIRING_CHECK`: `pair(D_base0 + D0 - N0, g2) + Σ_m [pair(S_m,H0_m) + pair(A_m,H1_m) + pair(B_m,H2_m)] == 0`.
   **Never accept `S_m, A_m, B_m` from calldata**: whoever knows `τ_m` could choose them to forge a balance.
6. L1 only: proofs verify (Section 11).

## 10. Apply

For each ring member `j` (after rollover): `C ← C + C_tx`, `D ← D + D_j + E_j`, `dPK ← dPK + dpk_j`,
`up_agg ← up_agg + up_j`, `credits_this_epoch += 1`, append update record, add `e` to `open_epochs`,
and emit `Credit(account, e, up_j, dpk_j, side_j)` so the owner can recover `δ_j` (Section 12).

For the sender (after rollover):
`C ← C + C_tx`; `D ← D + N0 - D_base`; `D_base ← N0 - D0`; drop all open epochs `< e` (their records are
now normalised; delete their `chain` entries); `debited_epoch ← e`; append a non-update record
(`is_update = false`) for epoch `e` and add `e` to `open_epochs` (the record is needed for cross-terms with
later credits of the same epoch). Native: `W ← W - W_base`, `W_base ← 0`.

### 10.1 Compaction

Owner (or anyone who can compute it) submits `D'` and the records of open epochs `< e` (witness rules as
in 9.5, including ignoring epochs that are no longer open).
Check: `pair(D_base - D', g2) + W_base == 0`, i.e. the V5 pairing check with `D_base0 + D0 - N0`
replaced by `D_base - D'`. Then `D ← D + D' - D_base`, `D_base ← D'`, drop those epochs. No proof needed: `D'` is unique.
Native: `W ← W - W_base`, `W_base ← 0` (as for a debit).
Honest `D' = C_base·s + g1·b_snapshot`.

### 10.2 Funding (S4)

`mint(account, amount)`: `D ← D + g1·amount` (and `D_base` too if the snapshot is current — simplest:
force a rollover first, then add to both `D` and `D_base`). Amount is public. No withdraw (S5).

## 11. Proof levels

- **L0:** no proofs. The contract checks only Sections 9.1–9.5. A malicious sender could
  post malformed `D_j`, `E_j`, `up_j` or overdraw (S3). Experiments only use honest senders.
- **L1:** Figure 1 Σ-protocols per receiver for `(up_j, dpk_j, E_j)` with base
  `C_base_j + C`, Fiat–Shamir over `BTP/fs/v1`.
- **L2:** Figure 2 Σ-Bullets with `(nC0, nD0) = (C_base0 + C, N0)`.

## 12. Read and key update (client)

- Key update after epoch `e` closes: `Δ = Σ δ_j` recovered from the `side_j` of all `Credit` events of
  epoch `e`; check `g1·Δ == dPK` from the `KeyUpdated` event; `s ← s + Δ`.
- Balance: `gt()·b = pair(D - C·s, g2) + W`, where the client rebuilds `W` from its records
  (Section 8.1) for open epochs. Find `b ∈ [0, MAX]` by baby-step giant-step (table of 2^16 entries).

## 13. Baseline contract (for E5)

`PriDeFULBaseline`: the paper's option (2). Each account stores `pk, C, D, version`; `version`
increments on every change of the account (credit, debit, mint). A transaction carries the `version`
it read for the sender and for each ring member (`versions[0]` = sender, then ring order) and computes
`E_j = (C_j + C)·δ_j` against the live `C_j`; the contract reverts (`StaleVersion`) if any version
changed. Then: sender `C += C_tx`, `D += D0`; ring member `C += C_tx`, `D += D_j + E_j`, `pk += dpk_j`
(immediate key update, as in PriDeFUL CT); `version += 1` for every touched account; one `Credit`
event per ring member. Ring checks as in 9.1. Like `PrideConcurrent` at L0 it verifies no proofs and
no pairings, so it isolates the concurrency rule. Under concurrency, only the first transaction per
touched account per block succeeds.

## 14. Simplifications

| ID | Simplification | Why it is acceptable here |
|---|---|---|
| S1 | CRS trapdoor derived from a seed | reproducible experiments; a ceremony is standard and orthogonal |
| S2 | δ delivered by hashed-ElGamal side value | avoids a discrete log; the paper suggests the same |
| S3 | L0: no zero-knowledge proofs on chain | proof costs are taken from the paper's measurements |
| S4 | public `mint` instead of a token deposit | funding is not part of the contribution |
| S5 | no withdraw, no sender anonymity, no relayer | same as the paper's scope for receiver anonymity |
| S6 | anvil runs the Prague hardfork | avoids the 2^24 per-transaction gas cap of Osaka; results above the cap are flagged |

## 15. Fixtures (Rust → Solidity)

`fixtures/*.json`, produced by `cargo run -p btp-protocol --bin gen-fixtures`. Points are hex strings
of the EIP-2537 encoding; scalars are 32-byte hex. Each fixture holds the CRS, the accounts before,
a list of transactions with their expected verify result, and the accounts after. Solidity tests read
them with `vm.readFile` + `vm.parseJson`.

## 16. Results

Every experiment writes `results/<experiment>/<run-id>.csv` plus a `meta.json` with the seed, git commit
and parameters. Plots are produced from these CSVs only.

## 17. Simulator model and baselines (E2, E3)

E2 and E3 use a conflict model: whether a payment lands depends only on which accounts it touches and
when, so the model has no cryptography. It is validated against the real ledger (same decisions on small
runs) and against the E5 chain measurements.

- Time is in blocks; epoch `e = ⌊block / L⌋` (the ledger's epoch is `e + 1`). Payments arrive as a
  Poisson process with rate λ per block; the sender is uniform, the real receiver uniform or Zipf(s),
  plus `t − 1` distinct uniform decoys. A payment arriving in block `b` can land from block `b + 1`.
  Each sender has at most one payment in flight; the next one waits (as nonces force on chain).
- All schemes see the same arrival stream. Within a block, transactions are applied in random order.
  Optional block capacity (transactions per block) comes from E4's measured gas.

| Scheme | A ready payment lands when | It is forward-secure (FS) at |
|---|---|---|
| ours | the sender has not debited in this epoch and every ring member has fewer than `K_MAX` credits in this epoch; otherwise it waits for the next epoch | the end of its epoch (the receiver updates its key) |
| PriDeFUL option (2) | no account it touches (sender or ring) was changed by an earlier transaction of the same block; otherwise it is resubmitted in the next block | landing |
| PriDeFUL option (1) | always; but if its real receiver was already updated in this block, its key update is ignored | landing if its update was applied, otherwise never |
| PriDe CT | always | never (no key evolution) |
| Construction II | only in the transfer phase (the first `L − U` blocks of an epoch, `U = max(1, L/4)`); 2 messages per payment | the end of its epoch's update phase |

Metrics per scheme: share landing at the first eligible block, attempts per payment, delay from
arrival to landing, share eventually forward-secure, time from arrival to FS, messages per payment.
E3 records `k` (credits per account per epoch, decoys included) with no cap, i.e. the demand that
`K_MAX` must serve.

