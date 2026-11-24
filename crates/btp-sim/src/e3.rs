//! Experiment E3: distribution of credits per account per epoch.

use crate::meta;
use crate::model::{arrivals, simulate, Config, KHist, Scheme};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::json;
use std::path::Path;
use std::time::Instant;

pub struct E3Args {
    pub seed: u64,
    pub seeds: u64,
    pub epochs: u64,
}

#[derive(Serialize)]
struct HistRow {
    sweep: &'static str,
    accounts: usize,
    ring: usize,
    rate: f64,
    zipf_s: f64,
    epoch_len: u64,
    k_max: Option<u32>,
    epochs: u64,
    capacity: Option<usize>,
    seed: u64,
    k: usize,
    count: u64,
}

#[derive(Serialize)]
struct SummaryRow {
    sweep: &'static str,
    accounts: usize,
    ring: usize,
    rate: f64,
    zipf_s: f64,
    epoch_len: u64,
    k_max: Option<u32>,
    epochs: u64,
    capacity: Option<usize>,
    seed: u64,
    account_epochs_with_credits: u64,
    k_mean: f64,
    k_p50: usize,
    k_p90: usize,
    k_p99: usize,
    k_max_seen: usize,
    share_above_4: f64,
    share_above_8: f64,
    share_above_16: f64,
}

fn sweeps(epochs: u64) -> Vec<(&'static str, Config)> {
    let d = Config {
        epochs,
        k_max: u32::MAX,
        ..Config::default()
    };
    let mut v = vec![];
    for l in [4, 8, 16, 32, 64] {
        for s in [0.0, 0.8, 1.2] {
            v.push((
                "epoch_len x zipf",
                Config {
                    epoch_len: l,
                    zipf_s: s,
                    ..d.clone()
                },
            ));
        }
    }
    for n in [1_000, 100_000, 1_000_000] {
        v.push((
            "accounts",
            Config {
                accounts: n,
                ..d.clone()
            },
        ));
    }
    for t in [2, 4, 16, 32, 64] {
        v.push((
            "ring",
            Config {
                ring: t,
                ..d.clone()
            },
        ));
    }
    for r in [5.0, 20.0, 100.0, 200.0] {
        v.push((
            "rate",
            Config {
                rate: r,
                ..d.clone()
            },
        ));
    }
    v
}

fn summarise(sweep: &'static str, cfg: &Config, seed: u64, h: &KHist) -> SummaryRow {
    let total = h.nonzero.max(1) as f64;
    let quantile = |q: f64| {
        let target = (q * h.nonzero as f64).ceil() as u64;
        let mut acc = 0;
        for (k, &c) in h.counts.iter().enumerate() {
            acc += c;
            if acc >= target.max(1) {
                return k;
            }
        }
        h.counts.len().saturating_sub(1)
    };
    let above = |x: usize| h.counts.iter().skip(x + 1).sum::<u64>() as f64 / total;
    let mean = h
        .counts
        .iter()
        .enumerate()
        .map(|(k, &c)| k as f64 * c as f64)
        .sum::<f64>()
        / total;
    SummaryRow {
        sweep,
        accounts: cfg.accounts,
        ring: cfg.ring,
        rate: cfg.rate,
        zipf_s: cfg.zipf_s,
        epoch_len: cfg.epoch_len,
        k_max: (cfg.k_max != u32::MAX).then_some(cfg.k_max),
        epochs: cfg.epochs,
        capacity: cfg.capacity,
        seed,
        account_epochs_with_credits: h.nonzero,
        k_mean: mean,
        k_p50: quantile(0.5),
        k_p90: quantile(0.9),
        k_p99: quantile(0.99),
        k_max_seen: h.counts.len().saturating_sub(1),
        share_above_4: above(4),
        share_above_8: above(8),
        share_above_16: above(16),
    }
}

pub fn run(a: &E3Args, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let git = meta::git_state();
    let start = Instant::now();
    let jobs: Vec<(&'static str, Config, u64)> = sweeps(a.epochs)
        .into_iter()
        .flat_map(|(sw, cfg)| (0..a.seeds).map(move |i| (sw, cfg.clone(), a.seed * 1000 + i)))
        .collect();
    let results: Vec<(&'static str, Config, u64, KHist)> = jobs
        .into_par_iter()
        .map(|(sw, cfg, seed)| {
            let arr = arrivals(&cfg, seed);
            let mut h = KHist::default();
            simulate(Scheme::Ours, &cfg, &arr, seed, None, Some(&mut h));
            (sw, cfg, seed, h)
        })
        .collect();
    let secs = start.elapsed().as_secs_f64();

    let dir = out_dir.join("e3");
    std::fs::create_dir_all(&dir)?;
    let run_id = format!("seed{}-seeds{}-epochs{}", a.seed, a.seeds, a.epochs);
    let mut hist = csv::Writer::from_path(dir.join(format!("{run_id}-hist.csv")))?;
    let mut summ = csv::Writer::from_path(dir.join(format!("{run_id}-summary.csv")))?;
    for (sw, cfg, seed, h) in &results {
        for (k, &count) in h.counts.iter().enumerate().filter(|(_, &c)| c > 0) {
            hist.serialize(HistRow {
                sweep: sw,
                accounts: cfg.accounts,
                ring: cfg.ring,
                rate: cfg.rate,
                zipf_s: cfg.zipf_s,
                epoch_len: cfg.epoch_len,
                k_max: (cfg.k_max != u32::MAX).then_some(cfg.k_max),
                epochs: cfg.epochs,
                capacity: cfg.capacity,
                seed: *seed,
                k,
                count,
            })?;
        }
        summ.serialize(summarise(sw, cfg, *seed, h))?;
    }
    hist.flush()?;
    summ.flush()?;
    meta::write(
        &dir.join(format!("{run_id}.meta.json")),
        &json!({
            "experiment": "E3 overlap profile (conflict model, SPEC 17)",
            "run_id": run_id,
            "seed": a.seed,
            "seeds_per_point": a.seeds,
            "k": "credits per account per epoch (decoys included), our scheme with no cap",
            "defaults": Config { epochs: a.epochs, k_max: u32::MAX, ..Config::default() },
            "git": git,
            "runtime_seconds": secs,
        }),
    )?;
    println!(
        "E3: {} runs in {secs:.1}s -> {}",
        results.len(),
        dir.join(&run_id).display()
    );
    Ok(())
}
