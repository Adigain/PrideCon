//! Test chain: verify then apply, with each δ recovered from its side value.
#![allow(dead_code)]

use btp_algebra::codec::fr_to_bytes32;
use btp_algebra::{Backend, GroupElem};
use btp_protocol::apply::{self, rollover};
use btp_protocol::crs::setup_from_seed;
use btp_protocol::transact::{
    build_compaction, fresh_account, keygen, side_decrypt, transact, SenderView,
};
use btp_protocol::verify::{verify_compaction, verify_tx, ChainView, SpendEval};
use btp_protocol::*;
use rand::RngCore;
use std::collections::{BTreeMap, HashSet};

pub struct TestChain<B: Backend> {
    pub crs: Crs<B>,
    pub td: Trapdoor<B>,
    pub accounts: Vec<AccountState<B>>,
    pub keys: Vec<Keypair<B>>,
    pub deltas: Vec<BTreeMap<u64, B::Fr>>,
    pub changes: Vec<Vec<(u64, i64, bool)>>,
    pub records: Vec<BTreeMap<u64, Vec<Record<B>>>>,
    pub tags: HashSet<[u8; 32]>,
    pub epoch: u64,
    pub k_max: u32,
    pub counters: OpCounters,
}

impl<B: Backend> ChainView<B> for TestChain<B> {
    fn account(&self, id: u32) -> Option<&AccountState<B>> {
        (id >= 1)
            .then(|| self.accounts.get(id as usize - 1))
            .flatten()
    }
    fn tag_used(&self, tag: &[u8; 32]) -> bool {
        self.tags.contains(tag)
    }
    fn current_epoch(&self) -> u64 {
        self.epoch
    }
    fn k_max(&self) -> u32 {
        self.k_max
    }
}

impl<B: Backend> TestChain<B> {
    pub fn new<R: RngCore>(n_accounts: usize, rng: &mut R) -> Self {
        let (crs, td) = setup_from_seed::<B>(b"test chain");
        let mut c = TestChain {
            crs,
            td,
            accounts: vec![],
            keys: vec![],
            deltas: vec![],
            changes: vec![],
            records: vec![],
            tags: HashSet::new(),
            epoch: 1,
            k_max: 8,
            counters: OpCounters::default(),
        };
        for _ in 0..n_accounts {
            let kp = keygen::<B, _>(rng);
            let (cc, dd) = fresh_account::<B, _>(&kp.pk, rng);
            c.accounts.push(apply::new_account(kp.pk, cc, dd, c.epoch));
            c.keys.push(kp);
            c.deltas.push(BTreeMap::new());
            c.changes.push(vec![]);
            c.records.push(BTreeMap::new());
        }
        c
    }

    fn idx(id: u32) -> usize {
        id as usize - 1
    }

    pub fn next_epoch(&mut self) {
        self.epoch += 1;
    }

    pub fn mint(&mut self, id: u32, amount: u64) {
        let e = self.epoch;
        let a = &mut self.accounts[Self::idx(id)];
        rollover(a, e);
        apply::mint(a, e, amount);
        self.changes[Self::idx(id)].push((e, amount as i64, true));
    }

    pub fn key(&self, id: u32, e: u64) -> Keypair<B> {
        let i = Self::idx(id);
        let sk = self.deltas[i]
            .range(..e)
            .fold(self.keys[i].sk, |acc, (_, d)| acc + d);
        Keypair {
            sk,
            pk: B::g1().smul(&sk),
        }
    }

    pub fn snapshot_balance(&self, id: u32, e: u64) -> u64 {
        let total: i64 = self.changes[Self::idx(id)]
            .iter()
            .filter(|(ep, _, mint)| *ep < e || (*ep == e && *mint))
            .map(|(_, a, _)| a)
            .sum();
        total as u64
    }

    pub fn build<R: RngCore>(
        &self,
        sender: u32,
        ring: &[u32],
        payloads: &[u64],
        rng: &mut R,
    ) -> Result<(Tx<B>, TxSecrets<B>), ProtocolError> {
        self.build_with_balance(
            sender,
            ring,
            payloads,
            self.snapshot_balance(sender, self.epoch),
            rng,
        )
    }

    pub fn build_with_balance<R: RngCore>(
        &self,
        sender: u32,
        ring: &[u32],
        payloads: &[u64],
        balance: u64,
        rng: &mut R,
    ) -> Result<(Tx<B>, TxSecrets<B>), ProtocolError> {
        let keys = self.key(sender, self.epoch);
        let view = SenderView {
            crs: &self.crs,
            epoch: self.epoch,
            sender_id: sender,
            sender_keys: &keys,
            sender_balance: balance,
            sender_state: &self.accounts[Self::idx(sender)],
            sender_records: &self.records[Self::idx(sender)],
            ring: ring
                .iter()
                .map(|&id| (id, &self.accounts[Self::idx(id)]))
                .collect(),
            payloads: payloads.to_vec(),
        };
        transact(&view, rng)
    }

    pub fn submit(&mut self, tx: &Tx<B>) -> Result<B::Fr, ProtocolError> {
        let mut counters = OpCounters::default();
        let t = verify_tx(&self.crs, self, tx, &SpendEval::Records, &mut counters)?;
        self.counters.pairings += counters.pairings;
        let e = tx.epoch;
        let s = Self::idx(tx.sender);
        rollover(&mut self.accounts[s], e);
        let rec = apply::apply_debit(&mut self.accounts[s], tx, t);
        self.records[s].entry(e).or_default().push(rec);
        for (j, &id) in tx.ring.iter().enumerate() {
            let i = Self::idx(id);
            let key = self.key(id, e);
            let delta = side_decrypt::<B>(&tx.recv[j].side, &tx.recv[j].up.0, &key.sk);
            assert_eq!(
                B::g1().smul(&delta),
                tx.recv[j].dpk,
                "side value must decrypt to δ"
            );
            *self.deltas[i].entry(e).or_insert(B::Fr::from(0u64)) += delta;
            rollover(&mut self.accounts[i], e);
            let rec = apply::apply_credit(&mut self.accounts[i], tx, j, t);
            self.records[i].entry(e).or_default().push(rec);
        }
        self.tags.insert(fr_to_bytes32(&t));
        Ok(t)
    }

    pub fn pay<R: RngCore>(
        &mut self,
        sender: u32,
        ring: &[u32],
        payloads: &[u64],
        rng: &mut R,
    ) -> Result<B::Fr, ProtocolError> {
        let (tx, _) = self.build(sender, ring, payloads, rng)?;
        let t = self.submit(&tx)?;
        let e = self.epoch;
        self.changes[Self::idx(sender)].push((e, -(payloads.iter().sum::<u64>() as i64), false));
        for (&id, &pl) in ring.iter().zip(payloads) {
            self.changes[Self::idx(id)].push((e, pl as i64, false));
        }
        Ok(t)
    }

    pub fn build_compaction(&self, id: u32) -> Compaction<B> {
        let e = self.epoch;
        build_compaction(
            id,
            e,
            &self.key(id, e),
            self.snapshot_balance(id, e),
            &self.accounts[Self::idx(id)],
            &self.records[Self::idx(id)],
        )
    }

    pub fn submit_compaction(&mut self, comp: &Compaction<B>) -> Result<(), ProtocolError> {
        let mut counters = OpCounters::default();
        verify_compaction(&self.crs, self, comp, &SpendEval::Records, &mut counters)?;
        let a = &mut self.accounts[Self::idx(comp.account)];
        rollover(a, comp.epoch);
        apply::compact(a, comp.epoch, comp.d_prime);
        Ok(())
    }
}
