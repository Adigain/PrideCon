//! The challenger: trials of the two-candidate game with Wilson intervals.

use crate::attacks::run_attack;
use crate::designs::{make_view, Ctx};
use crate::{Attack, Design};
use btp_algebra::Backend;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use rayon::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct RowResult {
    pub trials: u64,
    pub successes: u64,
    pub decided: u64,
}

impl RowResult {
    pub fn rate(&self) -> f64 {
        self.successes as f64 / self.trials as f64
    }

    pub fn wilson95(&self) -> (f64, f64) {
        let (n, p, z) = (self.trials as f64, self.rate(), 1.96f64);
        let denom = 1.0 + z * z / n;
        let centre = (p + z * z / (2.0 * n)) / denom;
        let half = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denom;
        (centre - half, centre + half)
    }
}

fn trial<B: Backend>(ctx: &Ctx<B>, design: Design, attack: Attack, seed: u64) -> (bool, bool) {
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    let c0 = rng.gen_range(0..1u64 << 16);
    let c1 = (c0 + rng.gen_range(1..1u64 << 16)) % (1 << 16);
    let cands = [c0, c1];
    let gamma = rng.gen_range(0..2usize);
    let view = make_view(ctx, design, attack.game(), cands, gamma, &mut rng);
    let answer = run_attack(attack, &view, cands);
    let mut adv_rng = ChaCha20Rng::seed_from_u64(seed ^ 0xadd5_eed5);
    let guess = answer.unwrap_or_else(|| adv_rng.gen_range(0..2));
    (guess == gamma, answer.is_some())
}

pub fn run_row<B: Backend>(
    ctx: &Ctx<B>,
    design: Design,
    attack: Attack,
    trials: u64,
    seed: u64,
) -> RowResult {
    let base = seed
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add((design as u64) << 8 | attack as u64);
    let (successes, decided) = (0..trials)
        .into_par_iter()
        .map(|i| {
            trial(
                ctx,
                design,
                attack,
                base.wrapping_add(i.wrapping_mul(0x2545_f491_4f6c_dd1d)),
            )
        })
        .map(|(s, d)| (s as u64, d as u64))
        .reduce(|| (0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    RowResult {
        trials,
        successes,
        decided,
    }
}
