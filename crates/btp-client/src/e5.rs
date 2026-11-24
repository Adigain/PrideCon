//! Experiment E5: concurrent senders per block on anvil, ours vs the baseline.

use crate::baseline::Base;
use crate::chain::Chain;
use crate::ours::Ours;
use eyre::Result;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub struct E5Args {
    pub accounts: usize,
    pub ring: usize,
    pub rounds: usize,
    pub senders: Vec<usize>,
    pub sweep_rings: Vec<usize>,
    pub sweep_senders: usize,
    pub seed: u64,
}

#[derive(Serialize)]
pub struct Row {
    contract: &'static str,
    senders: usize,
    ring_size: usize,
    round: usize,
    submitted: usize,
    succeeded: usize,
    reverted: usize,
    revert_reasons: String,
    gas_used_total: u64,
    blocks_spanned: usize,
}

fn draw(
    rng: &mut ChaCha20Rng,
    accounts: usize,
    senders: usize,
    n: usize,
) -> Vec<(u32, Vec<u32>, Vec<u64>)> {
    let ids: Vec<u32> = (1..=accounts as u32).collect();
    let chosen: Vec<u32> = ids.choose_multiple(rng, senders).copied().collect();
    chosen
        .into_iter()
        .map(|s| {
            let others: Vec<u32> = ids.iter().copied().filter(|&x| x != s).collect();
            let ring: Vec<u32> = others.choose_multiple(rng, n).copied().collect();
            let mut pls = vec![0u64; n];
            pls[0] = rng.gen_range(1..=100);
            (s, ring, pls)
        })
        .collect()
}

pub async fn run(chain: &Chain, a: &E5Args) -> Result<Vec<Row>> {
    let mut o = Ours::deploy(chain, a.seed, btp_protocol::Params::default().k_max, 16).await?;
    o.register(a.accounts, 20).await?;
    o.mint_all(1_000_000, 20).await?;
    let mut b = Base::deploy(chain, a.seed).await?;
    b.register(a.accounts, 20).await?;
    b.mint_all(1_000_000, 20).await?;

    let mut configs: Vec<(usize, usize)> = a.senders.iter().map(|&s| (s, a.ring)).collect();
    configs.extend(a.sweep_rings.iter().map(|&n| (a.sweep_senders, n)));
    let mut rng = ChaCha20Rng::seed_from_u64(a.seed);
    let mut rows = vec![];

    for (s, n) in configs {
        for round in 0..a.rounds {
            let pays = draw(&mut rng, a.accounts, s, n);

            o.advance_epoch().await?;
            let ops = pays
                .iter()
                .map(|(snd, ring, pls)| o.pay(*snd, ring, pls))
                .collect();
            let res = o.execute(ops).await?;
            let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
            for r in res.iter().filter(|r| !r.ok) {
                *reasons
                    .entry(format!("{:?}", r.error.expect("mirror gives a reason")))
                    .or_default() += 1;
            }
            rows.push(Row {
                contract: "PrideConcurrent",
                senders: s,
                ring_size: n,
                round,
                submitted: res.len(),
                succeeded: res.iter().filter(|r| r.ok).count(),
                reverted: res.iter().filter(|r| !r.ok).count(),
                revert_reasons: reasons
                    .iter()
                    .map(|(k, v)| format!("{k}:{v}"))
                    .collect::<Vec<_>>()
                    .join(";"),
                gas_used_total: res.iter().map(|r| r.gas_used).sum(),
                blocks_spanned: res.iter().map(|r| r.block).collect::<BTreeSet<_>>().len(),
            });

            let mut txs = vec![];
            for (snd, ring, pls) in &pays {
                txs.push(b.pay(*snd, ring, pls).await?);
            }
            let res = b.execute_pays(txs).await?;
            let reverted = res.iter().filter(|r| !r.ok).count();
            rows.push(Row {
                contract: "PriDeFULBaseline",
                senders: s,
                ring_size: n,
                round,
                submitted: res.len(),
                succeeded: res.len() - reverted,
                reverted,
                revert_reasons: if reverted > 0 {
                    format!("StaleVersion:{reverted}")
                } else {
                    String::new()
                },
                gas_used_total: res.iter().map(|r| r.gas_used).sum(),
                blocks_spanned: res.iter().map(|r| r.block).collect::<BTreeSet<_>>().len(),
            });
            eprintln!(
                "S={s:>2} n={n} round {round}: ours {}/{} baseline {}/{}",
                rows[rows.len() - 2].succeeded,
                s,
                rows[rows.len() - 1].succeeded,
                s
            );
        }
    }
    Ok(rows)
}
