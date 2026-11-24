//! Experiment E4: gas per operation on anvil against ring size n and credits k.

use crate::baseline::Base;
use crate::chain::Chain;
use crate::costmodel::{spend_gas, verify_gas};
use crate::ours::{OpResult, Ours};
use eyre::{ensure, Result};
use serde::Serialize;

pub const NS: [u32; 6] = [1, 2, 4, 8, 16, 32];
pub const KS: [u32; 7] = [0, 1, 2, 4, 8, 16, 32];
const TX_CAP: u64 = 1 << 24;

#[derive(Serialize)]
pub struct Row {
    contract: &'static str,
    operation: &'static str,
    n: u32,
    k: u32,
    gas_used: u64,
    calldata_bytes: usize,
    over_2_24: bool,
    model_precompile_gas: Option<u64>,
    model_note: &'static str,
}

fn row(contract: &'static str, operation: &'static str, n: u32, k: u32, r: &OpResult) -> Row {
    let (model, note) = match operation {
        "transact" | "transact_warm" => (
            Some(verify_gas(n as u64) + spend_gas(0).0),
            "V1-V4 + V5 with nothing deferred",
        ),
        "spend" => (
            Some(verify_gas(n as u64) + spend_gas(k as u64).0),
            "V1-V4 + V5 over one epoch of k credits",
        ),
        "compact" => (
            Some(spend_gas(k as u64).0),
            "compaction check over one epoch of k credits",
        ),
        _ => (None, ""),
    };
    Row {
        contract,
        operation,
        n,
        k,
        gas_used: r.gas_used,
        calldata_bytes: r.calldata_bytes,
        over_2_24: r.gas_used > TX_CAP,
        model_precompile_gas: model,
        model_note: note,
    }
}

async fn one(o: &mut Ours<'_>, op: crate::ours::Op) -> Result<OpResult> {
    let r = o.execute(vec![op]).await?.remove(0);
    ensure!(r.ok, "an honest E4 operation reverted: {:?}", r.error);
    Ok(r)
}

// Account ids: 1-6 senders, 7-69 fresh receivers, 70-75 warm-row senders, 76-82 T_k, 83-89 U_k,
// 90-152 credit senders, 153-159 spend receivers.
pub async fn run(chain: &Chain, seed: u64) -> Result<Vec<Row>> {
    let mut rows = vec![];
    let mut o = Ours::deploy(chain, seed, 32, 1000).await?;
    for r in o.register(159, 20).await? {
        rows.push(row("PrideConcurrent", "register", 0, 0, &r));
    }
    for r in o.mint_all(1_000_000, 20).await? {
        rows.push(row("PrideConcurrent", "mint", 0, 0, &r));
    }

    let mut rings = vec![];
    let mut next = 7;
    for &n in &NS {
        rings.push((next..next + n).collect::<Vec<u32>>());
        next += n;
    }
    for (operation, first_sender) in [("transact", 1), ("transact_warm", 70)] {
        for (i, (&n, ring)) in NS.iter().zip(&rings).enumerate() {
            let mut pls = vec![0u64; n as usize];
            pls[0] = 5;
            let op = o.pay(first_sender + i as u32, ring, &pls);
            rows.push(row(
                "PrideConcurrent",
                operation,
                n,
                0,
                &one(&mut o, op).await?,
            ));
        }
    }

    o.advance_epoch().await?;
    let mut sender = 90;
    let mut ops = vec![];
    for (idx, &k) in KS.iter().enumerate() {
        for _ in 0..k {
            ops.push(o.pay(sender, &[76 + idx as u32, 83 + idx as u32], &[1, 1]));
            sender += 1;
        }
    }
    while !ops.is_empty() {
        let rest = ops.split_off(ops.len().min(16));
        let res = o.execute(std::mem::replace(&mut ops, rest)).await?;
        ensure!(res.iter().all(|r| r.ok), "a credit of epoch 2 reverted");
    }

    o.advance_epoch().await?;
    for (idx, &k) in KS.iter().enumerate() {
        let op = o.pay(76 + idx as u32, &[153 + idx as u32], &[1]);
        rows.push(row(
            "PrideConcurrent",
            "spend",
            1,
            k,
            &one(&mut o, op).await?,
        ));
        let op = o.compact(83 + idx as u32);
        rows.push(row(
            "PrideConcurrent",
            "compact",
            0,
            k,
            &one(&mut o, op).await?,
        ));
    }

    let mut b = Base::deploy(chain, seed).await?;
    for r in b.register(69, 20).await? {
        rows.push(base_row("register", 0, &r));
    }
    for r in b.mint_all(1_000_000, 20).await? {
        rows.push(base_row("mint", 0, &r));
    }
    for (i, (&n, ring)) in NS.iter().zip(&rings).enumerate() {
        let mut pls = vec![0u64; n as usize];
        pls[0] = 5;
        let tx = b.pay(i as u32 + 1, ring, &pls).await?;
        let r = b.execute_pays(vec![tx]).await?.remove(0);
        ensure!(r.ok, "baseline transact reverted");
        rows.push(base_row("transact", n, &r));
    }
    Ok(rows)
}

fn base_row(operation: &'static str, n: u32, r: &crate::baseline::BaseResult) -> Row {
    Row {
        contract: "PriDeFULBaseline",
        operation,
        n,
        k: 0,
        gas_used: r.gas_used,
        calldata_bytes: r.calldata_bytes,
        over_2_24: r.gas_used > TX_CAP,
        model_precompile_gas: None,
        model_note: "",
    }
}
