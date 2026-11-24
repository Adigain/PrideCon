//! Honest driver: builds operations as wallets would and applies them to a mirror ledger.

use crate::crs::setup_from_seed;
use crate::ledger::{Ledger, TraceOp};
use crate::transact::{build_compaction, fresh_account, keygen, transact, SenderView};
use crate::wallet::Wallet;
use crate::{Compaction, Params, ProtocolError, Tx};
use btp_algebra::Backend;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

pub struct Driver<B: Backend> {
    pub ledger: Ledger<B>,
    pub wallets: Vec<Wallet<B>>,
    pub live: Vec<u64>,
    pub snap: Vec<u64>,
    pub trace: Vec<TraceOp<B>>,
    seen: usize,
    rng: ChaCha20Rng,
}

impl<B: Backend> Driver<B> {
    pub fn new(crs_seed: &[u8], accounts: usize, k_max: u32, seed: u64) -> Self {
        let (crs, _) = setup_from_seed::<B>(crs_seed);
        let mut d = Driver {
            ledger: Ledger::new_mirror(
                crs,
                Params {
                    k_max,
                    ..Params::default()
                },
            ),
            wallets: vec![],
            live: vec![],
            snap: vec![],
            trace: vec![],
            seen: 0,
            rng: ChaCha20Rng::seed_from_u64(seed),
        };
        for _ in 0..accounts {
            let keys = keygen::<B, _>(&mut d.rng);
            let (c, dd) = fresh_account::<B, _>(&keys.pk, &mut d.rng);
            d.op(TraceOp::Register {
                pk: keys.pk,
                c,
                d: dd,
            })
            .expect("register");
            d.wallets
                .push(Wallet::new(d.wallets.len() as u32 + 1, &keys));
            d.live.push(0);
            d.snap.push(0);
        }
        d
    }

    pub fn op(&mut self, op: TraceOp<B>) -> Result<(), ProtocolError> {
        let r = self.ledger.apply_op(&op);
        self.trace.push(op);
        for ev in &self.ledger.events[self.seen..] {
            for w in &mut self.wallets {
                w.observe(ev);
            }
        }
        self.seen = self.ledger.events.len();
        r
    }

    pub fn epoch(&self) -> u64 {
        self.ledger.epoch
    }

    pub fn mint(&mut self, id: u32, amount: u64) {
        self.op(TraceOp::Mint { id, amount }).expect("mint");
        self.live[id as usize - 1] += amount;
        self.snap[id as usize - 1] += amount;
    }

    pub fn mint_all(&mut self, amount: u64) {
        for id in 1..=self.wallets.len() as u32 {
            self.mint(id, amount);
        }
    }

    pub fn next_epoch(&mut self) {
        self.op(TraceOp::NextEpoch).unwrap();
        self.snap = self.live.clone();
    }

    pub fn build_pay_with(
        &mut self,
        sender: u32,
        ring: &[u32],
        pls: &[u64],
        balance: u64,
    ) -> Tx<B> {
        let (e, si) = (self.epoch(), sender as usize - 1);
        let keys = self.wallets[si].key(e).expect("key");
        let view = SenderView {
            crs: &self.ledger.crs,
            epoch: e,
            sender_id: sender,
            sender_keys: &keys,
            sender_balance: balance,
            sender_state: &self.ledger.accounts[si],
            sender_records: &self.wallets[si].records,
            ring: ring
                .iter()
                .map(|&id| (id, &self.ledger.accounts[id as usize - 1]))
                .collect(),
            payloads: pls.to_vec(),
        };
        transact(&view, &mut self.rng).expect("build").0
    }

    pub fn build_pay(&mut self, sender: u32, ring: &[u32], pls: &[u64]) -> Tx<B> {
        let b = self.snap[sender as usize - 1];
        self.build_pay_with(sender, ring, pls, b)
    }

    pub fn try_submit(&mut self, tx: Tx<B>, pls: &[u64]) -> Result<(), ProtocolError> {
        let (s, ring) = (tx.sender, tx.ring.clone());
        self.op(TraceOp::Transact(tx))?;
        self.live[s as usize - 1] -= pls.iter().sum::<u64>();
        for (&id, &pl) in ring.iter().zip(pls) {
            self.live[id as usize - 1] += pl;
        }
        Ok(())
    }

    pub fn submit(&mut self, tx: Tx<B>, pls: &[u64]) {
        self.try_submit(tx, pls).expect("honest transaction");
    }

    pub fn pay(&mut self, sender: u32, ring: &[u32], pls: &[u64]) {
        let tx = self.build_pay(sender, ring, pls);
        self.submit(tx, pls);
    }

    pub fn build_compact(&mut self, id: u32) -> Compaction<B> {
        let (e, i) = (self.epoch(), id as usize - 1);
        let keys = self.wallets[i].key(e).expect("key");
        build_compaction(
            id,
            e,
            &keys,
            self.snap[i],
            &self.ledger.accounts[i],
            &self.wallets[i].records,
        )
    }

    pub fn reject(&mut self, op: TraceOp<B>, why: ProtocolError) {
        assert_eq!(self.op(op), Err(why));
    }
}
