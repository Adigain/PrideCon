//! Client for PrideConcurrent; a local mirror ledger must agree with the chain after every block.

use crate::chain::{position, Chain};
use alloy::network::ReceiptResponse;
use alloy::primitives::Address;
use alloy_sol_types::{SolCall, SolValue};
use btp_algebra::bls::Bls;
use btp_protocol::abi::{self, sol::IPrideConcurrent as C};
use btp_protocol::crs::setup_from_seed;
use btp_protocol::ledger::{Ledger, TraceOp};
use btp_protocol::transact::{build_compaction, fresh_account, keygen, transact, SenderView};
use btp_protocol::wallet::Wallet;
use btp_protocol::{Compaction, Params, ProtocolError, Tx};
use eyre::{bail, Result};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

type B = Bls;

pub struct Op {
    pub from: Address,
    pub trace: TraceOp<B>,
    pub pay: Option<(u32, Vec<u32>, Vec<u64>)>,
}

#[derive(Clone, Debug)]
pub struct OpResult {
    pub ok: bool,
    pub error: Option<ProtocolError>,
    pub gas_used: u64,
    pub calldata_bytes: usize,
    pub block: u64,
}

pub struct Ours<'a> {
    chain: &'a Chain,
    pub addr: Address,
    pub local: Ledger<B>,
    wallets: Vec<Wallet<B>>,
    pub live: Vec<u64>,
    snap: Vec<u64>,
    seen: usize,
    rng: ChaCha20Rng,
    epoch_len: u64,
}

impl<'a> Ours<'a> {
    pub async fn deploy(
        chain: &'a Chain,
        seed: u64,
        k_max: u32,
        epoch_len: u64,
    ) -> Result<Ours<'a>> {
        let (crs, _) = setup_from_seed::<B>(b"btp-client");
        let ctor = (abi::crs(&crs), epoch_len, k_max).abi_encode_params();
        let addr = chain.deploy("PrideConcurrent", ctor).await?;
        let params = Params {
            k_max,
            epoch_len_blocks: epoch_len,
            ..Params::default()
        };
        let mut o = Ours {
            chain,
            addr,
            local: Ledger::new_mirror(crs, params),
            wallets: vec![],
            live: vec![],
            snap: vec![],
            seen: 0,
            rng: ChaCha20Rng::seed_from_u64(seed),
            epoch_len,
        };
        o.sync_epoch().await?;
        Ok(o)
    }

    pub fn eoa(&self, id: u32) -> Address {
        self.chain.eoas[id as usize % self.chain.eoas.len()]
    }

    pub async fn sync_epoch(&mut self) -> Result<u64> {
        let next = (self.chain.block_number().await? + 1) / self.epoch_len + 1;
        if self.local.epoch > next {
            bail!("mirror is ahead of the chain");
        }
        while self.local.epoch < next {
            self.local
                .apply_op(&TraceOp::NextEpoch)
                .expect("next epoch");
            self.snap = self.live.clone();
        }
        Ok(next)
    }

    pub async fn advance_epoch(&mut self) -> Result<u64> {
        let e = self.sync_epoch().await?;
        let bn = self.chain.block_number().await?;
        self.chain.mine_blocks(e * self.epoch_len - 1 - bn).await?;
        let e2 = self.sync_epoch().await?;
        assert_eq!(e2, e + 1);
        Ok(e2)
    }

    fn calldata(op: &TraceOp<B>) -> Vec<u8> {
        match op {
            TraceOp::Register { pk, c, d } => C::registerCall {
                pk: abi::g1(pk),
                c: abi::g1(c),
                d: abi::g1(d),
            }
            .abi_encode(),
            TraceOp::Mint { id, amount } => C::mintCall {
                id: *id,
                amount: *amount,
            }
            .abi_encode(),
            TraceOp::Transact(tx) => C::transactCall { txn: abi::tx(tx) }.abi_encode(),
            TraceOp::Compact(c) => C::compactCall {
                cp: abi::compaction(c),
            }
            .abi_encode(),
            TraceOp::NextEpoch => unreachable!("not a transaction"),
        }
    }

    pub async fn execute(&mut self, ops: Vec<Op>) -> Result<Vec<OpResult>> {
        if ops.is_empty() {
            return Ok(vec![]);
        }
        let epoch = self.sync_epoch().await?;
        let mut reqs = vec![];
        let mut sizes = vec![];
        for op in &ops {
            let data = Self::calldata(&op.trace);
            sizes.push(data.len());
            let req = self.chain.request(op.from, self.addr, data);
            let gas = self.chain.gas_for(&req).await;
            reqs.push(req.gas_limit(gas));
        }
        let receipts = self.chain.run_block(reqs).await?;

        let mut order: Vec<usize> = (0..ops.len()).collect();
        order.sort_by_key(|&i| position(&receipts[i]));
        let mut results = vec![None; ops.len()];
        for i in order {
            let r = &receipts[i];
            let block = r.block_number().unwrap_or(0);
            while self.local.epoch < block / self.epoch_len + 1 {
                self.local
                    .apply_op(&TraceOp::NextEpoch)
                    .expect("next epoch");
                self.snap = self.live.clone();
            }
            let local = self.local.apply_op(&ops[i].trace);
            if local.is_ok() != r.status() {
                bail!(
                    "chain and mirror disagree on op {i} (epoch {epoch}): chain {}, mirror {local:?}",
                    if r.status() { "success" } else { "revert" }
                );
            }
            match (&ops[i].trace, &local) {
                (TraceOp::Register { .. }, Ok(())) => {
                    self.live.push(0);
                    self.snap.push(0);
                }
                (TraceOp::Mint { id, amount }, Ok(())) => {
                    self.live[*id as usize - 1] += amount;
                    self.snap[*id as usize - 1] += amount;
                }
                (TraceOp::Transact(_), Ok(())) => {
                    let (s, ring, pls) = ops[i].pay.as_ref().expect("payment plaintext");
                    self.live[*s as usize - 1] -= pls.iter().sum::<u64>();
                    for (id, pl) in ring.iter().zip(pls) {
                        self.live[*id as usize - 1] += pl;
                    }
                }
                _ => {}
            }
            results[i] = Some(OpResult {
                ok: r.status(),
                error: local.err(),
                gas_used: r.gas_used(),
                calldata_bytes: sizes[i],
                block,
            });
        }
        for ev in &self.local.events[self.seen..] {
            for w in &mut self.wallets {
                w.observe(ev);
            }
        }
        self.seen = self.local.events.len();
        self.check_state().await?;
        Ok(results.into_iter().map(Option::unwrap).collect())
    }

    pub async fn check_state(&self) -> Result<()> {
        let n = self.chain.view(self.addr, C::numAccountsCall {}).await?;
        if n as usize != self.local.accounts.len() {
            bail!(
                "chain has {n} accounts, mirror {}",
                self.local.accounts.len()
            );
        }
        for (i, a) in self.local.accounts.iter().enumerate() {
            let on_chain = self
                .chain
                .view(self.addr, C::getAccountCall { id: i as u32 + 1 })
                .await?;
            if on_chain != abi::account(a) {
                bail!(
                    "state of account {} differs between chain and mirror",
                    i + 1
                );
            }
        }
        Ok(())
    }

    pub async fn register(&mut self, n: usize, chunk: usize) -> Result<Vec<OpResult>> {
        let mut out = vec![];
        let mut pending = vec![];
        for _ in 0..n {
            let keys = keygen::<B, _>(&mut self.rng);
            let (c, d) = fresh_account::<B, _>(&keys.pk, &mut self.rng);
            let id = (self.wallets.len() + 1) as u32;
            self.wallets.push(Wallet::new(id, &keys));
            pending.push(Op {
                from: self.eoa(id),
                trace: TraceOp::Register { pk: keys.pk, c, d },
                pay: None,
            });
        }
        while !pending.is_empty() {
            let rest = pending.split_off(pending.len().min(chunk));
            out.extend(self.execute(std::mem::replace(&mut pending, rest)).await?);
        }
        Ok(out)
    }

    pub async fn mint_all(&mut self, amount: u64, chunk: usize) -> Result<Vec<OpResult>> {
        let ids: Vec<u32> = (1..=self.local.accounts.len() as u32).collect();
        let mut out = vec![];
        for part in ids.chunks(chunk) {
            let ops = part
                .iter()
                .map(|&id| Op {
                    from: self.eoa(id),
                    trace: TraceOp::Mint { id, amount },
                    pay: None,
                })
                .collect();
            out.extend(self.execute(ops).await?);
        }
        Ok(out)
    }

    pub fn pay(&mut self, sender: u32, ring: &[u32], pls: &[u64]) -> Op {
        let tx = self.build_tx(sender, ring, pls);
        Op {
            from: self.eoa(sender),
            trace: TraceOp::Transact(tx),
            pay: Some((sender, ring.to_vec(), pls.to_vec())),
        }
    }

    fn build_tx(&mut self, sender: u32, ring: &[u32], pls: &[u64]) -> Tx<B> {
        let (e, si) = (self.local.epoch, sender as usize - 1);
        let keys = self.wallets[si].key(e).expect("key");
        let view = SenderView {
            crs: &self.local.crs,
            epoch: e,
            sender_id: sender,
            sender_keys: &keys,
            sender_balance: self.snap[si],
            sender_state: &self.local.accounts[si],
            sender_records: &self.wallets[si].records,
            ring: ring
                .iter()
                .map(|&id| (id, &self.local.accounts[id as usize - 1]))
                .collect(),
            payloads: pls.to_vec(),
        };
        transact(&view, &mut self.rng)
            .expect("honest transaction")
            .0
    }

    pub fn compact(&mut self, id: u32) -> Op {
        let (e, i) = (self.local.epoch, id as usize - 1);
        let keys = self.wallets[i].key(e).expect("key");
        let c: Compaction<B> = build_compaction(
            id,
            e,
            &keys,
            self.snap[i],
            &self.local.accounts[i],
            &self.wallets[i].records,
        );
        Op {
            from: self.eoa(id),
            trace: TraceOp::Compact(c),
            pay: None,
        }
    }
}
