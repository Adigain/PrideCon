//! Experiment E1: order-independence over random workloads and orders.

use crate::meta;
use btp_algebra::exp::Exp61;
use btp_protocol::read::BsgsTable;
use btp_protocol::workload::{generate, run, Order, RunConfig, RunStats, WorkloadParams};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::json;
use std::path::Path;
use std::time::Instant;

pub struct E1Args {
    pub seed: u64,
    pub cases: u64,
    pub orders: u64,
    pub max_accounts: usize,
    pub max_epochs: u64,
}

#[derive(Serialize)]
struct Row {
    case: u64,
    case_seed: u64,
    accounts: usize,
    epochs: u64,
    pays_per_epoch: usize,
    ring_size: usize,
    k_max: u32,
    compact_prob: f64,
    orders: u64,
    pays: u64,
    credits: u64,
    compactions: u64,
    mints: u64,
    divergent_orders: u64,
    rejected: u64,
    native_mirror_mismatches: u64,
    read_failures: u64,
    w_mismatches: u64,
    key_failures: u64,
    pairings: u64,
    msm_terms: u64,
}

fn case_params(rng: &mut ChaCha20Rng, a: &E1Args) -> WorkloadParams {
    let accounts = rng.gen_range(3..=a.max_accounts.max(3));
    WorkloadParams {
        accounts,
        epochs: rng.gen_range(1..=a.max_epochs.max(1)),
        pays_per_epoch: rng.gen_range(1..=accounts),
        ring_size: rng.gen_range(1..accounts.min(5)),
        max_payload: rng.gen_range(1..=200),
        mint_prob: 0.2,
        compact_prob: rng.gen_range(0.0..0.5),
        k_max: rng.gen_range(1..=8),
        ..Default::default()
    }
}

fn run_case(case: u64, a: &E1Args) -> Row {
    let case_seed = a.seed.wrapping_mul(1_000_003).wrapping_add(case);
    let mut rng = ChaCha20Rng::seed_from_u64(case_seed);
    let p = case_params(&mut rng, a);
    let plan = generate(&p, &mut rng);
    let table = BsgsTable::<Exp61>::new(p.max_balance());

    let mut first = None;
    let mut stats = RunStats::default();
    let mut divergent = 0;
    let mut counters = Default::default();
    for o in 0..a.orders {
        let cfg = RunConfig {
            build_seed: case_seed,
            order: Order::Seeded(case_seed ^ (o + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15)),
            check_reads: true,
            check_each_op: false,
        };
        let out = run::<Exp61>(&plan, &p, &cfg, &table);
        let s = &out.stats;
        stats.rejected += s.rejected;
        stats.native_mirror_mismatches += s.native_mirror_mismatches;
        stats.read_failures += s.read_failures;
        stats.w_mismatches += s.w_mismatches;
        stats.key_failures += s.key_failures;
        let state = (out.algebraic_state(), out.balances.clone());
        match &first {
            None => {
                stats.pays = s.pays;
                stats.credits = s.credits;
                stats.compactions = s.compactions;
                stats.mints = s.mints;
                counters = out.mirror.counters.clone();
                first = Some(state);
            }
            Some(f) => divergent += (*f != state) as u64,
        }
    }
    Row {
        case,
        case_seed,
        accounts: p.accounts,
        epochs: p.epochs,
        pays_per_epoch: p.pays_per_epoch,
        ring_size: p.ring_size,
        k_max: p.k_max,
        compact_prob: p.compact_prob,
        orders: a.orders,
        pays: stats.pays,
        credits: stats.credits,
        compactions: stats.compactions,
        mints: stats.mints,
        divergent_orders: divergent,
        rejected: stats.rejected,
        native_mirror_mismatches: stats.native_mirror_mismatches,
        read_failures: stats.read_failures,
        w_mismatches: stats.w_mismatches,
        key_failures: stats.key_failures,
        pairings: counters.pairings,
        msm_terms: counters.msm_terms,
    }
}

pub fn run_e1(a: &E1Args, out_dir: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let git = meta::git_state();
    let start = Instant::now();
    let rows: Vec<Row> = (0..a.cases)
        .into_par_iter()
        .map(|c| run_case(c, a))
        .collect();
    let secs = start.elapsed().as_secs_f64();

    let dir = out_dir.join("e1");
    std::fs::create_dir_all(&dir)?;
    let run_id = format!("seed{}-cases{}-orders{}", a.seed, a.cases, a.orders);
    let mut w = csv::Writer::from_path(dir.join(format!("{run_id}.csv")))?;
    for r in &rows {
        w.serialize(r)?;
    }
    w.flush()?;

    let sum = |f: fn(&Row) -> u64| rows.iter().map(f).sum::<u64>();
    let totals = json!({
        "cases": rows.len(),
        "runs": rows.len() as u64 * a.orders,
        "pays": sum(|r| r.pays),
        "credits": sum(|r| r.credits),
        "compactions": sum(|r| r.compactions),
        "mints": sum(|r| r.mints),
        "divergent_orders": sum(|r| r.divergent_orders),
        "rejected": sum(|r| r.rejected),
        "native_mirror_mismatches": sum(|r| r.native_mirror_mismatches),
        "read_failures": sum(|r| r.read_failures),
        "w_mismatches": sum(|r| r.w_mismatches),
        "key_failures": sum(|r| r.key_failures),
    });
    let failures: u64 = [
        "divergent_orders",
        "rejected",
        "native_mirror_mismatches",
        "read_failures",
        "w_mismatches",
        "key_failures",
    ]
    .iter()
    .map(|k| totals[k].as_u64().unwrap())
    .sum();
    meta::write(
        &dir.join(format!("{run_id}.meta.json")),
        &json!({
            "experiment": "E1 order-independence",
            "run_id": run_id,
            "backend": "exponent, 61-bit field (Exp61)",
            "seed": a.seed,
            "cases": a.cases,
            "orders_per_case": a.orders,
            "case_shape": {
                "accounts": [3, a.max_accounts], "epochs": [1, a.max_epochs],
                "ring_size": "1..min(5, accounts)", "k_max": [1, 8], "compact_prob": [0.0, 0.5],
                "mint_prob": 0.2, "max_payload": [1, 200]
            },
            "git": git,
            "runtime_seconds": secs,
            "totals": totals,
            "all_checks_passed": failures == 0,
        }),
    )?;
    println!(
        "E1: {} cases x {} orders in {secs:.1}s -> {}",
        a.cases,
        a.orders,
        dir.join(&run_id).display()
    );
    println!("totals: {totals}");
    Ok(failures == 0)
}
