//! Client for the contracts on a local anvil chain: experiments E4 and E5.

mod baseline;
mod chain;
mod costmodel;
mod e4;
mod e5;
mod ours;

use chain::Chain;
use clap::{Parser, Subcommand};
use eyre::Result;
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser, Debug)]
#[command(about = "Client for the PrideConcurrent and PriDeFULBaseline contracts on anvil")]
struct Cli {
    #[arg(
        long,
        help = "RPC endpoint of a running anvil; if absent, the client starts its own."
    )]
    rpc: Option<String>,
    #[arg(
        long,
        default_value_t = 100_000_000,
        help = "Block gas limit for the anvil the client starts."
    )]
    block_gas_limit: u64,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long, default_value = "results")]
    out: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    #[command(
        about = "E4: gas per operation against ring size n and credits k, ours and baseline."
    )]
    E4,
    #[command(about = "E5: concurrent senders per block, ours vs baseline.")]
    E5 {
        #[arg(long, default_value_t = 64)]
        accounts: usize,
        #[arg(long, default_value_t = 4)]
        ring: usize,
        #[arg(long, default_value_t = 5)]
        rounds: usize,
        #[arg(long, value_delimiter = ',', default_value = "1,2,4,8,16,32")]
        senders: Vec<usize>,
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "2,8",
            help = "Extra ring sizes, run with `--sweep-senders` senders."
        )]
        sweep_rings: Vec<usize>,
        #[arg(long, default_value_t = 16)]
        sweep_senders: usize,
    },
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn write_csv<T: Serialize>(path: &Path, rows: &[T]) -> Result<()> {
    let mut w = csv::Writer::from_path(path)?;
    for r in rows {
        w.serialize(r)?;
    }
    w.flush()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let git_state = json!({
        "commit": git(&["rev-parse", "HEAD"]),
        "dirty": git(&["status", "--porcelain", "--", ":(top,exclude)results"]).map(|s| !s.is_empty()),
    });
    let chain = match &cli.rpc {
        Some(url) => Chain::connect(url).await?,
        None => Chain::spawn(128, cli.block_gas_limit).await?,
    };
    let chain_meta = json!({
        "anvil": chain::anvil_version(),
        "rpc": if cli.rpc.is_some() { "external" } else { "spawned by btp-client" },
        "hardfork": "prague",
        "block_gas_limit": cli.block_gas_limit,
    });
    let start = std::time::Instant::now();

    let (exp, run_id, meta) = match &cli.cmd {
        Cmd::E4 => {
            let rows = e4::run(&chain, cli.seed).await?;
            let dir = PathBuf::from(&cli.out).join("e4");
            std::fs::create_dir_all(&dir)?;
            let run_id = format!("seed{}", cli.seed);
            write_csv(&dir.join(format!("{run_id}.csv")), &rows)?;
            let meta = json!({
                "experiment": "E4 gas",
                "contracts": {
                    "PrideConcurrent": { "kMax": 32, "epochLen": 1000, "proof_level": "L0" },
                    "PriDeFULBaseline": {}
                },
                "ring_sizes": e4::NS, "credits_k": e4::KS,
                "gas": "receipt gasUsed (includes the 21000 base fee and calldata)",
                "model": "precompile-only cost model of reference/onchain_check.py part 5 (BLS12-381)",
                "per_transaction_cap_flag": 1u64 << 24,
            });
            ("e4", run_id, meta)
        }
        Cmd::E5 {
            accounts,
            ring,
            rounds,
            senders,
            sweep_rings,
            sweep_senders,
        } => {
            let a = e5::E5Args {
                accounts: *accounts,
                ring: *ring,
                rounds: *rounds,
                senders: senders.clone(),
                sweep_rings: sweep_rings.clone(),
                sweep_senders: *sweep_senders,
                seed: cli.seed,
            };
            let rows = e5::run(&chain, &a).await?;
            let dir = PathBuf::from(&cli.out).join("e5");
            std::fs::create_dir_all(&dir)?;
            let run_id = format!("seed{}-n{}-acc{}", cli.seed, ring, accounts);
            write_csv(&dir.join(format!("{run_id}.csv")), &rows)?;
            let meta = json!({
                "experiment": "E5 load on a chain",
                "accounts": accounts, "ring_size": ring, "rounds": rounds,
                "senders_per_block": senders, "sweep_rings": sweep_rings, "sweep_senders": sweep_senders,
                "contracts": {
                    "PrideConcurrent": { "kMax": btp_protocol::Params::default().k_max, "epochLen": 16 },
                    "PriDeFULBaseline": {}
                },
                "round": "S senders, each one payment to n random members, all built against the same state, sent into one block; ours starts a fresh epoch per round",
            });
            ("e5", run_id, meta)
        }
    };
    let mut meta = meta;
    meta["run_id"] = json!(run_id);
    meta["seed"] = json!(cli.seed);
    meta["git"] = git_state;
    meta["chain"] = chain_meta;
    meta["runtime_seconds"] = json!(start.elapsed().as_secs_f64());
    let path = PathBuf::from(&cli.out)
        .join(exp)
        .join(format!("{run_id}.meta.json"));
    std::fs::write(&path, serde_json::to_string_pretty(&meta)? + "\n")?;
    println!(
        "{exp} done in {:.1}s -> {}",
        start.elapsed().as_secs_f64(),
        path.display()
    );
    Ok(())
}
