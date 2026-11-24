//! Experiment E2: concurrency benefit of each scheme in the conflict model (SPEC 17).

use crate::meta;
use crate::model::{arrivals, e5_round, simulate, stats, Config, Scheme, Stats};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::json;
use std::path::Path;
use std::time::Instant;

pub struct E2Args {
    pub seed: u64,
    pub seeds: u64,
    pub epochs: u64,
    pub e4_csv: String,
    pub block_gas_limit: u64,
}

#[derive(Serialize)]
struct Row {
    scheme: &'static str,
    variant: &'static str,
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
    payments: usize,
    landed: usize,
    unfinished: usize,
    first_try_rate: f64,
    attempts_mean: f64,
    latency_mean: f64,
    latency_p50: u64,
    latency_p95: u64,
    fs_fraction: f64,
    fs_time_mean: f64,
    fs_time_p95: u64,
    messages_per_payment: f64,
}

#[derive(Serialize)]
struct E5ModelRow {
    senders: usize,
    ring_size: usize,
    accounts: usize,
    rounds: usize,
    option2_success_rate: f64,
    ours_success_rate: f64,
}

fn sweeps(epochs: u64) -> Vec<(&'static str, Config)> {
    let d = Config {
        epochs,
        ..Config::default()
    };
    let mut v = vec![("default", d.clone())];
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
    for s in [0.8, 1.2] {
        v.push((
            "zipf",
            Config {
                zipf_s: s,
                ..d.clone()
            },
        ));
    }
    for l in [4, 64] {
        v.push((
            "epoch_len",
            Config {
                epoch_len: l,
                ..d.clone()
            },
        ));
    }
    for k in [4, 16] {
        v.push((
            "k_max",
            Config {
                k_max: k,
                ..d.clone()
            },
        ));
    }
    v
}

type GasPoints = Vec<(u64, u64)>;

fn gas_from_e4(path: &str) -> Option<(GasPoints, GasPoints)> {
    let mut rdr = csv::Reader::from_path(path).ok()?;
    let (mut ours, mut base) = (vec![], vec![]);
    for rec in rdr.deserialize::<std::collections::HashMap<String, String>>() {
        let r = rec.ok()?;
        let (n, g) = (
            r["n"].parse::<u64>().ok()?,
            r["gas_used"].parse::<u64>().ok()?,
        );
        match (r["contract"].as_str(), r["operation"].as_str()) {
            ("PrideConcurrent", "transact") => ours.push((n, g)),
            ("PriDeFULBaseline", "transact") => base.push((n, g)),
            _ => {}
        }
    }
    (!ours.is_empty() && !base.is_empty()).then_some((ours, base))
}

fn gas_at(points: &[(u64, u64)], t: u64) -> u64 {
    let mut p = points.to_vec();
    p.sort_unstable();
    let i = p
        .iter()
        .position(|&(n, _)| n >= t)
        .unwrap_or(p.len() - 1)
        .max(1);
    let ((x0, y0), (x1, y1)) = (p[i - 1], p[i]);
    let y = y0 as f64 + (y1 as f64 - y0 as f64) * (t as f64 - x0 as f64) / (x1 as f64 - x0 as f64);
    y.max(1.0) as u64
}

fn run_point(
    scheme: Scheme,
    variant: &'static str,
    sweep: &'static str,
    cfg: &Config,
    seed: u64,
) -> Row {
    let arr = arrivals(cfg, seed);
    let out = simulate(scheme, cfg, &arr, seed, None, None);
    let st: Stats = stats(scheme, &arr, &out);
    Row {
        scheme: scheme.name(),
        variant,
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
        payments: st.payments,
        landed: st.landed,
        unfinished: st.unfinished,
        first_try_rate: st.first_try_rate,
        attempts_mean: st.attempts_mean,
        latency_mean: st.latency_mean,
        latency_p50: st.latency_p50,
        latency_p95: st.latency_p95,
        fs_fraction: st.fs_fraction,
        fs_time_mean: st.fs_time_mean,
        fs_time_p95: st.fs_time_p95,
        messages_per_payment: st.messages_per_payment,
    }
}

pub fn run(a: &E2Args, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let git = meta::git_state();
    let start = Instant::now();
    let seeds: Vec<u64> = (0..a.seeds).map(|i| a.seed * 1000 + i).collect();

    let mut jobs: Vec<(Scheme, &'static str, &'static str, Config, u64)> = vec![];
    for (sweep, cfg) in sweeps(a.epochs) {
        for &s in &seeds {
            for scheme in Scheme::ALL {
                jobs.push((scheme, "unlimited", sweep, cfg.clone(), s));
            }
        }
    }
    let gas = gas_from_e4(&a.e4_csv);
    if let Some((ours, base)) = &gas {
        let d = Config {
            epochs: a.epochs,
            ..Config::default()
        };
        for rate in [5.0, 20.0, 50.0, 100.0, 200.0] {
            for (scheme, pts) in [(Scheme::Ours, ours), (Scheme::PriDeFulOption2, base)] {
                let cap = (a.block_gas_limit / gas_at(pts, d.ring as u64)) as usize;
                let cfg = Config {
                    rate,
                    capacity: Some(cap.max(1)),
                    ..d.clone()
                };
                for &s in &seeds {
                    jobs.push((scheme, "gas-capacity", "rate", cfg.clone(), s));
                }
            }
        }
    }
    let rows: Vec<Row> = jobs
        .par_iter()
        .map(|(sc, v, sw, cfg, s)| run_point(*sc, v, sw, cfg, *s))
        .collect();

    let mut e5 = vec![];
    let mut configs: Vec<(usize, usize)> = [1, 2, 4, 8, 16, 32].iter().map(|&s| (s, 4)).collect();
    configs.extend([(16, 2), (16, 8)]);
    for (senders, n) in configs {
        let rounds = 20_000;
        let mut rng = ChaCha20Rng::seed_from_u64(a.seed ^ (senders as u64) << 8 ^ n as u64);
        let (mut o2, mut ours) = (0usize, 0usize);
        for _ in 0..rounds {
            let (x, y) = e5_round(&mut rng, 64, senders, n, 8);
            o2 += x;
            ours += y;
        }
        let total = (rounds * senders) as f64;
        e5.push(E5ModelRow {
            senders,
            ring_size: n,
            accounts: 64,
            rounds,
            option2_success_rate: o2 as f64 / total,
            ours_success_rate: ours as f64 / total,
        });
    }
    let secs = start.elapsed().as_secs_f64();

    let dir = out_dir.join("e2");
    std::fs::create_dir_all(&dir)?;
    let run_id = format!("seed{}-seeds{}-epochs{}", a.seed, a.seeds, a.epochs);
    let mut w = csv::Writer::from_path(dir.join(format!("{run_id}.csv")))?;
    rows.iter().try_for_each(|r| w.serialize(r))?;
    w.flush()?;
    let mut w = csv::Writer::from_path(dir.join(format!("{run_id}-e5model.csv")))?;
    e5.iter().try_for_each(|r| w.serialize(r))?;
    w.flush()?;
    meta::write(
        &dir.join(format!("{run_id}.meta.json")),
        &json!({
            "experiment": "E2 concurrency benefit (conflict model, SPEC 17)",
            "run_id": run_id,
            "seed": a.seed,
            "seeds_per_point": a.seeds,
            "defaults": Config { epochs: a.epochs, ..Config::default() },
            "sweeps": "one parameter at a time: accounts, ring, rate, zipf, epoch_len, k_max",
            "gas_capacity_variant": gas.as_ref().map(|_| json!({
                "source": a.e4_csv, "block_gas_limit": a.block_gas_limit,
                "gas_per_tx": "E4 transact gas at the default ring size (ours: fresh receivers), linearly interpolated"
            })),
            "e5_model": "20000 rounds per point, N = 64, K_MAX = 8, random order within the block",
            "git": git,
            "runtime_seconds": secs,
            "rows": rows.len(),
        }),
    )?;
    println!(
        "E2: {} rows in {secs:.1}s -> {}",
        rows.len(),
        dir.join(&run_id).display()
    );
    Ok(())
}
