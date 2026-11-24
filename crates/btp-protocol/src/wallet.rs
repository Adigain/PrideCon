//! A wallet: derives its keys from side values, keeps its records and reads balances (SPEC 12).

use crate::apply::rolled;
use crate::ledger::Event;
use crate::read::{balance_gt, key_update, w_of_epochs, BsgsTable};
use crate::transact::side_decrypt;
use crate::types::{AccountState, KeyUpdate, Keypair, Record, Recv};
use crate::ProtocolError;
use ark_ff::Zero;
use btp_algebra::{Backend, GroupElem};
use std::collections::BTreeMap;

pub struct Wallet<B: Backend> {
    pub id: u32,
    sk0: B::Fr,
    credits: BTreeMap<u64, Vec<Recv<B>>>,
    deltas: BTreeMap<u64, B::Fr>,
    pub records: BTreeMap<u64, Vec<Record<B>>>,
    pub key_update_mismatches: u32,
}

impl<B: Backend> Wallet<B> {
    pub fn new(id: u32, keys: &Keypair<B>) -> Self {
        Wallet {
            id,
            sk0: keys.sk,
            credits: BTreeMap::new(),
            deltas: BTreeMap::new(),
            records: BTreeMap::new(),
            key_update_mismatches: 0,
        }
    }

    pub fn observe(&mut self, ev: &Event<B>) {
        match ev {
            Event::Credit {
                account,
                epoch,
                recv,
            } if *account == self.id => {
                self.credits.entry(*epoch).or_default().push(recv.clone());
            }
            Event::RecordAdded {
                account,
                epoch,
                record,
            } if *account == self.id => {
                self.records.entry(*epoch).or_default().push(record.clone());
            }
            Event::KeyUpdated { account, update } if *account == self.id => {
                let ok = self.check_key_update(update);
                self.key_update_mismatches += u32::from(!ok);
            }
            _ => {}
        }
    }

    pub fn key(&mut self, e: u64) -> Result<Keypair<B>, ProtocolError> {
        let mut sk = self.sk0;
        for (&ep, recvs) in self.credits.range(..e) {
            let delta = match self.deltas.get(&ep) {
                Some(d) => *d,
                None => {
                    let logged = recvs.iter().fold(B::G1::zero(), |a, r| a + r.dpk);
                    let d = key_update(&sk, recvs, &logged)? - sk;
                    self.deltas.insert(ep, d);
                    d
                }
            };
            sk += delta;
        }
        Ok(Keypair {
            sk,
            pk: B::g1().smul(&sk),
        })
    }

    pub fn live_key(&mut self, e: u64) -> Result<B::Fr, ProtocolError> {
        let sk = self.key(e)?.sk;
        let partial = self.credits.get(&e).map_or(B::Fr::zero(), |rs| {
            rs.iter().fold(B::Fr::zero(), |a, r| {
                a + side_decrypt::<B>(&r.side, &r.up.0, &sk)
            })
        });
        Ok(sk + partial)
    }

    pub fn check_key_update(&mut self, up: &KeyUpdate<B>) -> bool {
        let Ok(old) = self.key(up.epoch) else {
            return false;
        };
        let Ok(new) = self.key(up.epoch + 1) else {
            return false;
        };
        let delta = new.sk - old.sk;
        B::g1().smul(&delta) == up.dpk && up.up_agg.1 - up.up_agg.0.smul(&old.sk) == up.dpk
    }

    pub fn w_of(&self, epochs: impl IntoIterator<Item = u64>) -> B::Gt {
        let recs: Vec<&[Record<B>]> = epochs
            .into_iter()
            .map(|ep| self.records.get(&ep).map_or(&[][..], |v| v.as_slice()))
            .collect();
        w_of_epochs(recs)
    }

    pub fn w_live(&self, acct: &AccountState<B>) -> B::Gt {
        self.w_of(acct.open_epochs.iter().copied())
    }

    pub fn read_live(
        &mut self,
        acct: &AccountState<B>,
        e: u64,
        table: &BsgsTable<B>,
    ) -> Option<u64> {
        let sk = self.live_key(e).ok()?;
        table.solve(&balance_gt::<B>(&acct.c, &acct.d, &sk, &self.w_live(acct)))
    }

    pub fn read_snapshot(
        &mut self,
        acct: &AccountState<B>,
        e: u64,
        table: &BsgsTable<B>,
    ) -> Option<u64> {
        let a = rolled(acct, e);
        let sk = self.key(e).ok()?.sk;
        let w_base = self.w_of(a.open_epochs.iter().copied().filter(|&ep| ep < e));
        table.solve(&balance_gt::<B>(&a.c_base, &a.d_base, &sk, &w_base))
    }
}
