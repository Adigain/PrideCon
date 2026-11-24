//! Command-line runner for E1-E3.

use btp_sim::{e1, e2, e3};
use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(about = "Concurrent PriDeFUL CT simulator")]
struct Args {
    #[arg(long, default_value = "e1", help = "Experiment to run: e1, e2 or e3.")]
    experiment: String,
    #[arg(long, default_value_t = 1000, help = "Number of accounts (E2/E3).")]
    accounts: u32,
    #[arg(
        long,
        default_value_t = 4,
        help = "Ring size (receivers per transaction, E2/E3)."
    )]
    ring: u32,
    #[arg(long, default_value_t = 1, help = "Random seed.")]
    seed: u64,
    #[arg(long, default_value = "results", help = "Output directory.")]
    out: String,
    #[arg(long, default_value_t = 1000, help = "E1: number of random workloads.")]
    cases: u64,
    #[arg(long, default_value_t = 3, help = "E1: orders tried per workload.")]
    orders: u64,
    #[arg(
        long,
        default_value_t = 10,
        help = "E1: largest number of accounts in a workload."
    )]
    max_accounts: usize,
    #[arg(
        long,
        default_value_t = 5,
        help = "E1: largest number of epochs in a workload."
    )]
    max_epochs: u64,
    #[arg(long, default_value_t = 5, help = "E2/E3: seeds per configuration.")]
    seeds: u64,
    #[arg(
        long,
        default_value_t = 200,
        help = "E2/E3: epochs with arrivals per run."
    )]
    epochs: u64,
    #[arg(
        long,
        default_value = "results/e4/seed1.csv",
        help = "E2: E4 results used for the gas-capacity variant."
    )]
    e4_csv: String,
    #[arg(
        long,
        default_value_t = 100_000_000,
        help = "E2: block gas limit for the gas-capacity variant."
    )]
    block_gas_limit: u64,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let out = PathBuf::from(&args.out);
    match args.experiment.as_str() {
        "e1" => {
            let a = e1::E1Args {
                seed: args.seed,
                cases: args.cases,
                orders: args.orders,
                max_accounts: args.max_accounts,
                max_epochs: args.max_epochs,
            };
            match e1::run_e1(&a, &out) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => {
                    eprintln!("E1: some checks FAILED (see the CSV)");
                    ExitCode::FAILURE
                }
                Err(e) => {
                    eprintln!("E1: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        "e2" => {
            let a = e2::E2Args {
                seed: args.seed,
                seeds: args.seeds,
                epochs: args.epochs,
                e4_csv: args.e4_csv.clone(),
                block_gas_limit: args.block_gas_limit,
            };
            report(e2::run(&a, &out))
        }
        "e3" => {
            let a = e3::E3Args {
                seed: args.seed,
                seeds: args.seeds,
                epochs: args.epochs,
            };
            report(e3::run(&a, &out))
        }
        other => {
            eprintln!("btp-sim: unknown experiment {other} (use e1, e2 or e3)");
            ExitCode::FAILURE
        }
    }
}

fn report(r: Result<(), Box<dyn std::error::Error>>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("btp-sim: {e}");
            ExitCode::FAILURE
        }
    }
}
