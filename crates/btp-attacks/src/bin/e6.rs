//! Experiment E6: attack success rates and the generic-group rank test, written to results/e6.

use btp_algebra::bls::Bls;
use btp_algebra::exp::Exp61;
use btp_algebra::Backend;
use btp_attacks::attacks::{applies, expected_break};
use btp_attacks::designs::Ctx;
use btp_attacks::game::run_row;
use btp_attacks::rank::{cases, run_case};
use btp_attacks::{Attack, Design, Game};
use clap::Parser;
use serde::Serialize;
use serde_json::json;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(about = "E6: attacks on the dual-key, Method A and final designs")]
struct Args {
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(
        long,
        default_value_t = 100_000,
        help = "Trials per row on the exponent backend."
    )]
    trials_exp: u64,
    #[arg(long, default_value_t = 2000, help = "Trials per row on BLS12-381.")]
    trials_bls: u64,
    #[arg(
        long,
        default_value_t = 5,
        help = "Random instantiations per rank-test case."
    )]
    rank_seeds: u64,
    #[arg(long, default_value = "results")]
    out: String,
}

#[derive(Serialize)]
struct AttackRow {
    backend: &'static str,
    design: &'static str,
    attack: String,
    game: &'static str,
    description: &'static str,
    trials: u64,
    successes: u64,
    rate: f64,
    ci95_low: f64,
    ci95_high: f64,
    decided: u64,
    expected: &'static str,
    as_expected: bool,
}

#[derive(Serialize)]
struct RankRow {
    game: &'static str,
    case: &'static str,
    seed: u64,
    g1_real: usize,
    g2_real: usize,
    gt_real: usize,
    g1_rand: usize,
    g2_rand: usize,
    gt_rand: usize,
    leak: bool,
    expected_leak: bool,
}

fn attack_rows<B: Backend>(trials: u64, seed: u64) -> Vec<AttackRow> {
    let ctx = Ctx::<B>::new();
    let mut rows = vec![];
    for d in Design::ALL {
        for a in Attack::ALL.into_iter().filter(|&a| applies(d, a)) {
            let res = run_row(&ctx, d, a, trials, seed);
            let (lo, hi) = res.wilson95();
            let brk = expected_break(d, a);
            let ok = if brk {
                res.successes == trials
            } else {
                res.decided == 0
            };
            rows.push(AttackRow {
                backend: B::NAME,
                design: d.name(),
                attack: format!("{a:?}"),
                game: match a.game() {
                    Game::Payload => "payload",
                    Game::OldBalance => "old balance",
                },
                description: a.describe(),
                trials,
                successes: res.successes,
                rate: res.rate(),
                ci95_low: lo,
                ci95_high: hi,
                decided: res.decided,
                expected: if brk { "break" } else { "guess" },
                as_expected: ok,
            });
            eprintln!(
                "{:>10} | {:<30} | {a:?} | {:.4} [{lo:.4}, {hi:.4}]",
                B::NAME,
                d.name(),
                res.rate()
            );
        }
    }
    rows
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() -> ExitCode {
    let a = Args::parse();
    let git_state = json!({
        "commit": git(&["rev-parse", "HEAD"]),
        "dirty": git(&["status", "--porcelain", "--", ":(top,exclude)results"]).map(|s| !s.is_empty()),
    });
    let start = Instant::now();

    let mut attacks = attack_rows::<Exp61>(a.trials_exp, a.seed);
    attacks.extend(attack_rows::<Bls>(a.trials_bls, a.seed));

    let mut ranks = vec![];
    for c in cases() {
        for s in 0..a.rank_seeds {
            let r = run_case(&c, a.seed.wrapping_mul(1000).wrapping_add(s));
            ranks.push(RankRow {
                game: c.game,
                case: c.name,
                seed: s,
                g1_real: r.real.0,
                g2_real: r.real.1,
                gt_real: r.real.2,
                g1_rand: r.rand.0,
                g2_rand: r.rand.1,
                gt_rand: r.rand.2,
                leak: r.leak(),
                expected_leak: c.expected_leak,
            });
        }
    }
    let secs = start.elapsed().as_secs_f64();

    let dir = PathBuf::from(&a.out).join("e6");
    let run_id = format!("seed{}-exp{}-bls{}", a.seed, a.trials_exp, a.trials_bls);
    let write = || -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(&dir)?;
        let mut w = csv::Writer::from_path(dir.join(format!("{run_id}-attacks.csv")))?;
        attacks.iter().try_for_each(|r| w.serialize(r))?;
        w.flush()?;
        let mut w = csv::Writer::from_path(dir.join(format!("{run_id}-rank.csv")))?;
        ranks.iter().try_for_each(|r| w.serialize(r))?;
        w.flush()?;
        Ok(())
    };
    if let Err(e) = write() {
        eprintln!("E6: {e}");
        return ExitCode::FAILURE;
    }
    let attacks_ok = attacks.iter().all(|r| r.as_expected);
    let ranks_ok = ranks.iter().all(|r| r.leak == r.expected_leak);
    let meta = json!({
        "experiment": "E6 attack regression",
        "run_id": run_id,
        "seed": a.seed,
        "trials_per_row": { "exponent": a.trials_exp, "bls12-381": a.trials_bls },
        "rank_seeds_per_case": a.rank_seeds,
        "game": "two candidates (Def. 2); prior balance hidden uniform (payload game), payloads uniform in [0,2^62) (old-balance game); candidates in [0,2^16); one concurrent honest credit; updated key revealed after the epoch",
        "git": git_state,
        "runtime_seconds": secs,
        "attack_rows_as_expected": attacks_ok,
        "rank_cases_as_expected": ranks_ok,
    });
    let meta_path = dir.join(format!("{run_id}.meta.json"));
    if let Err(e) = std::fs::write(
        &meta_path,
        serde_json::to_string_pretty(&meta).unwrap() + "\n",
    ) {
        eprintln!("E6: {e}");
        return ExitCode::FAILURE;
    }
    println!("E6 done in {secs:.1}s -> {}", dir.join(&run_id).display());
    println!("attack rows as expected: {attacks_ok}; rank cases as expected: {ranks_ok}");
    if attacks_ok && ranks_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
