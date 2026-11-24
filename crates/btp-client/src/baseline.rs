//! Client for PriDeFULBaseline: payments built on live state, outcomes checked against the version rule.

use crate::chain::{position, Chain};
use alloy::network::ReceiptResponse;
use alloy::primitives::{Address, FixedBytes};
use alloy_sol_types::SolCall;
use ark_ff::UniformRand;
use btp_algebra::bls::Bls;
use btp_algebra::{Backend, GroupElem};
use btp_protocol::abi::{self, g1_from, sol, sol::IPriDeFULBaseline as Bc};
use btp_protocol::transact::{fresh_account, keygen, side_encrypt};
use eyre::{bail, eyre, Result};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use std::collections::HashSet;

type Fr = <Bls as Backend>::Fr;

pub struct BaseResult {
    pub ok: bool,
    pub gas_used: u64,
    pub calldata_bytes: usize,
    pub block: u64,
    pub position: (u64, u64),
}

pub struct Base<'a> {
    chain: &'a Chain,
    pub addr: Address,
    pub accounts: usize,
    rng: ChaCha20Rng,
}

impl<'a> Base<'a> {
    pub async fn deploy(chain: &'a Chain, seed: u64) -> Result<Base<'a>> {
        let addr = chain.deploy("PriDeFULBaseline", vec![]).await?;
        Ok(Base {
            chain,
            addr,
            accounts: 0,
            rng: ChaCha20Rng::seed_from_u64(seed),
        })
    }

    pub fn eoa(&self, id: u32) -> Address {
        self.chain.eoas[id as usize % self.chain.eoas.len()]
    }

    async fn execute(&self, calls: Vec<(Address, Vec<u8>)>) -> Result<Vec<BaseResult>> {
        let mut reqs = vec![];
        let mut sizes = vec![];
        for (from, data) in calls {
            sizes.push(data.len());
            let req = self.chain.request(from, self.addr, data);
            let gas = self.chain.gas_for(&req).await;
            reqs.push(req.gas_limit(gas));
        }
        let receipts = self.chain.run_block(reqs).await?;
        Ok(receipts
            .iter()
            .zip(sizes)
            .map(|(r, size)| BaseResult {
                ok: r.status(),
                gas_used: r.gas_used(),
                calldata_bytes: size,
                block: r.block_number().unwrap_or(0),
                position: position(r),
            })
            .collect())
    }

    pub async fn register(&mut self, n: usize, chunk: usize) -> Result<Vec<BaseResult>> {
        let mut out = vec![];
        let mut start = self.accounts;
        while start < self.accounts + n {
            let end = (start + chunk).min(self.accounts + n);
            let mut calls = vec![];
            for id in start + 1..=end {
                let keys = keygen::<Bls, _>(&mut self.rng);
                let (c, d) = fresh_account::<Bls, _>(&keys.pk, &mut self.rng);
                let call = Bc::registerCall {
                    pk: abi::g1(&keys.pk),
                    c: abi::g1(&c),
                    d: abi::g1(&d),
                };
                calls.push((self.eoa(id as u32), call.abi_encode()));
            }
            out.extend(self.execute(calls).await?);
            start = end;
        }
        self.accounts += n;
        if out.iter().any(|r| !r.ok) {
            bail!("baseline registration failed");
        }
        Ok(out)
    }

    pub async fn mint_all(&self, amount: u64, chunk: usize) -> Result<Vec<BaseResult>> {
        let mut out = vec![];
        let ids: Vec<u32> = (1..=self.accounts as u32).collect();
        for part in ids.chunks(chunk) {
            let calls = part
                .iter()
                .map(|&id| (self.eoa(id), Bc::mintCall { id, amount }.abi_encode()))
                .collect();
            out.extend(self.execute(calls).await?);
        }
        Ok(out)
    }

    async fn view(&self, id: u32) -> Result<sol::BaseAccountView> {
        self.chain.view(self.addr, Bc::getAccountCall { id }).await
    }

    pub async fn pay(
        &mut self,
        sender: u32,
        ring: &[u32],
        pls: &[u64],
    ) -> Result<(Address, sol::BaseTx)> {
        let g1 = Bls::g1();
        let s = self.view(sender).await?;
        let r = Fr::rand(&mut self.rng);
        let c = g1.smul(&r);
        let pk_s = g1_from(&s.pk).ok_or_else(|| eyre!("bad pk"))?;
        let pl0: u64 = pls.iter().sum();
        let mut versions = vec![s.version];
        let mut recv = vec![];
        for (&id, &pl) in ring.iter().zip(pls) {
            let a = self.view(id).await?;
            let pk = g1_from(&a.pk).ok_or_else(|| eyre!("bad pk"))?;
            let c_live = g1_from(&a.c).ok_or_else(|| eyre!("bad C"))?;
            let (delta, rho) = (Fr::rand(&mut self.rng), Fr::rand(&mut self.rng));
            let shared = pk.smul(&rho);
            recv.push(sol::BaseRecv {
                d: abi::g1(&(pk.smul(&r) + g1.smul(&Fr::from(pl)))),
                up1: abi::g1(&g1.smul(&rho)),
                up2: abi::g1(&(shared + g1.smul(&delta))),
                dpk: abi::g1(&g1.smul(&delta)),
                e: abi::g1(&(c_live + c).smul(&delta)),
                side: FixedBytes::from(side_encrypt::<Bls>(&delta, &shared)),
            });
            versions.push(a.version);
        }
        let tx = sol::BaseTx {
            sender,
            ring: ring.to_vec(),
            versions,
            c: abi::g1(&c),
            d0: abi::g1(&(pk_s.smul(&r) - g1.smul(&Fr::from(pl0)))),
            recv,
        };
        Ok((self.eoa(sender), tx))
    }

    pub async fn execute_pays(&self, txs: Vec<(Address, sol::BaseTx)>) -> Result<Vec<BaseResult>> {
        let touched: Vec<Vec<u32>> = txs
            .iter()
            .map(|(_, t)| {
                std::iter::once(t.sender)
                    .chain(t.ring.iter().copied())
                    .collect()
            })
            .collect();
        let calls = txs
            .into_iter()
            .map(|(from, tx)| (from, Bc::transactCall { txn: tx }.abi_encode()))
            .collect();
        let results = self.execute(calls).await?;
        let mut order: Vec<usize> = (0..results.len()).collect();
        order.sort_by_key(|&i| results[i].position);
        let mut changed: HashSet<u32> = HashSet::new();
        for i in order {
            let expect_ok = touched[i].iter().all(|id| !changed.contains(id));
            if expect_ok != results[i].ok {
                bail!("baseline outcome of tx {i} does not follow the version rule");
            }
            if results[i].ok {
                changed.extend(touched[i].iter().copied());
            }
        }
        Ok(results)
    }
}
