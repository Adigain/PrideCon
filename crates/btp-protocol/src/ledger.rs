//! The ledger in two modes: native (W in GT) and mirror (handle records, as the contract).

use crate::apply::{self, rollover};
use crate::params::Params;
use crate::read::cross_term;
use crate::types::{AccountState, Compaction, Crs, KeyUpdate, OpCounters, Record, Recv, Tx};
use crate::verify::{verify_compaction, verify_tx, ChainView, SpendEval};
use crate::ProtocolError;
use btp_algebra::codec::fr_to_bytes32;
use btp_algebra::{Backend, GroupElem};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event<B: Backend> {
    Registered {
        id: u32,
        pk: B::G1,
    },
    KeyUpdated {
        account: u32,
        update: KeyUpdate<B>,
    },
    Credit {
        account: u32,
        epoch: u64,
        recv: Recv<B>,
    },
    RecordAdded {
        account: u32,
        epoch: u64,
        record: Record<B>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceOp<B: Backend> {
    Register { pk: B::G1, c: B::G1, d: B::G1 },
    Mint { id: u32, amount: u64 },
    NextEpoch,
    Transact(Tx<B>),
    Compact(Compaction<B>),
}

#[derive(Clone, Debug)]
struct NativeW<B: Backend> {
    w: Vec<B::Gt>,
    w_base: Vec<B::Gt>,
    current: Vec<Vec<Record<B>>>,
}

#[derive(Clone, Debug)]
pub struct Ledger<B: Backend> {
    pub crs: Crs<B>,
    pub params: Params,
    pub accounts: Vec<AccountState<B>>,
    pub tags: HashSet<[u8; 32]>,
    pub epoch: u64,
    pub events: Vec<Event<B>>,
    pub counters: OpCounters,
    native: Option<NativeW<B>>,
}

impl<B: Backend> ChainView<B> for Ledger<B> {
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
        self.params.k_max
    }
}

impl<B: Backend> Ledger<B> {
    fn new(crs: Crs<B>, params: Params, native: bool) -> Self {
        Ledger {
            crs,
            params,
            accounts: vec![],
            tags: HashSet::new(),
            epoch: 1,
            events: vec![],
            counters: OpCounters::default(),
            native: native.then(|| NativeW {
                w: vec![],
                w_base: vec![],
                current: vec![],
            }),
        }
    }

    pub fn new_native(crs: Crs<B>, params: Params) -> Self {
        Self::new(crs, params, true)
    }

    pub fn new_mirror(crs: Crs<B>, params: Params) -> Self {
        Self::new(crs, params, false)
    }

    pub fn is_native(&self) -> bool {
        self.native.is_some()
    }

    pub fn w(&self, id: u32) -> Option<B::Gt> {
        self.native.as_ref().map(|n| n.w[id as usize - 1])
    }

    pub fn next_epoch(&mut self) {
        self.epoch += 1;
    }

    pub fn register(&mut self, pk: B::G1, c: B::G1, d: B::G1) -> Result<u32, ProtocolError> {
        if c.is_zero() || pk.is_zero() {
            return Err(ProtocolError::BadRegistration);
        }
        self.accounts.push(apply::new_account(pk, c, d, self.epoch));
        if let Some(n) = &mut self.native {
            n.w.push(B::Gt::zero());
            n.w_base.push(B::Gt::zero());
            n.current.push(vec![]);
        }
        let id = self.accounts.len() as u32;
        self.events.push(Event::Registered { id, pk });
        Ok(id)
    }

    fn touch(&mut self, id: u32) {
        let (i, e) = (id as usize - 1, self.epoch);
        if let Some(update) = rollover(&mut self.accounts[i], e) {
            if let Some(n) = &mut self.native {
                n.w_base[i] = n.w[i];
                n.current[i].clear();
            }
            self.events.push(Event::KeyUpdated {
                account: id,
                update,
            });
        }
    }

    fn w_base_rolled(&self, id: u32) -> Option<B::Gt> {
        let i = id as usize - 1;
        self.native.as_ref().map(|n| {
            if self.accounts[i].snap_epoch < self.epoch {
                n.w[i]
            } else {
                n.w_base[i]
            }
        })
    }

    fn add_record(&mut self, id: u32, rec: Record<B>) {
        let i = id as usize - 1;
        if let Some(n) = &mut self.native {
            for old in &n.current[i] {
                n.w[i] = n.w[i] + cross_term(&rec, old) + cross_term(old, &rec);
            }
            n.current[i].push(rec.clone());
        }
        self.events.push(Event::RecordAdded {
            account: id,
            epoch: self.epoch,
            record: rec,
        });
    }

    fn normalise_w(&mut self, id: u32) {
        if let Some(n) = &mut self.native {
            let i = id as usize - 1;
            n.w[i] = n.w[i] - n.w_base[i];
            n.w_base[i] = B::Gt::zero();
        }
    }

    pub fn mint(&mut self, id: u32, amount: u64) -> Result<(), ProtocolError> {
        self.account(id).ok_or(ProtocolError::UnknownAccount)?;
        self.touch(id);
        apply::mint(&mut self.accounts[id as usize - 1], self.epoch, amount);
        Ok(())
    }

    pub fn submit(&mut self, tx: &Tx<B>) -> Result<B::Fr, ProtocolError> {
        let mut counters = OpCounters::default();
        let w_base = self
            .account(tx.sender)
            .and_then(|_| self.w_base_rolled(tx.sender));
        let eval = match &w_base {
            Some(w) => SpendEval::Native(w),
            None => SpendEval::Records,
        };
        let t = verify_tx(&self.crs, self, tx, &eval, &mut counters)?;
        add_counters(&mut self.counters, &counters);

        let e = self.epoch;
        self.touch(tx.sender);
        let rec = apply::apply_debit(&mut self.accounts[tx.sender as usize - 1], tx, t);
        self.add_record(tx.sender, rec);
        self.normalise_w(tx.sender);
        for (j, &id) in tx.ring.iter().enumerate() {
            self.touch(id);
            self.events.push(Event::Credit {
                account: id,
                epoch: e,
                recv: tx.recv[j].clone(),
            });
            let rec = apply::apply_credit(&mut self.accounts[id as usize - 1], tx, j, t);
            self.add_record(id, rec);
        }
        self.tags.insert(fr_to_bytes32(&t));
        Ok(t)
    }

    pub fn compact(&mut self, comp: &Compaction<B>) -> Result<(), ProtocolError> {
        let mut counters = OpCounters::default();
        let w_base = self
            .account(comp.account)
            .and_then(|_| self.w_base_rolled(comp.account));
        let eval = match &w_base {
            Some(w) => SpendEval::Native(w),
            None => SpendEval::Records,
        };
        verify_compaction(&self.crs, self, comp, &eval, &mut counters)?;
        add_counters(&mut self.counters, &counters);
        self.touch(comp.account);
        apply::compact(
            &mut self.accounts[comp.account as usize - 1],
            comp.epoch,
            comp.d_prime,
        );
        self.normalise_w(comp.account);
        Ok(())
    }
}

impl<B: Backend> Ledger<B> {
    pub fn apply_op(&mut self, op: &TraceOp<B>) -> Result<(), ProtocolError> {
        match op {
            TraceOp::Register { pk, c, d } => self.register(*pk, *c, *d).map(|_| ()),
            TraceOp::Mint { id, amount } => self.mint(*id, *amount),
            TraceOp::NextEpoch => {
                self.next_epoch();
                Ok(())
            }
            TraceOp::Transact(tx) => self.submit(tx).map(|_| ()),
            TraceOp::Compact(c) => self.compact(c),
        }
    }
}

fn add_counters(total: &mut OpCounters, add: &OpCounters) {
    total.pairings += add.pairings;
    total.msm_terms += add.msm_terms;
    total.g1_adds += add.g1_adds;
}
