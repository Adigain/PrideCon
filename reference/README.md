# Reference models (Python)

Exponent-level models: a group element is its discrete log, a pairing is a multiplication.
They are exact for the algebra and are the ground truth the Rust code is checked against.

| File | What it checks |
|---|---|
| `ggm.py` | leak test: original scheme, Method A, tagged handles |
| `tdddh.py` | the TD-DDH rank test with negative controls; end-to-end order-independence |
| `debit_check.py` | the debit rule with N0 mixed with concurrent credits |
| `onchain_check.py` | batched Verify with G1-only arithmetic, deferred spend check, compaction, gas model |
| `bls_consts.py` | BLS12-381 constants and the 2-word negation used in `BLS12381.sol` |

Run any of them with `python reference/<file>.py`.
