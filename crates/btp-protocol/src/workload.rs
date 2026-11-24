//! Random honest workloads, run on a native and a mirror ledger in lockstep.

use crate::crs::setup_from_seed;
use crate::ledger::{Ledger, TraceOp};
use crate::params::Params;
use crate::read::BsgsTable;
use crate::transact::{build_compaction, fresh_account, keygen, transact, SenderView};
use crate::types::{AccountState, Compaction, Tx};
use crate::wallet::Wallet;
use btp_algebra::Backend;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

#[derive(Clone, Debug)]
pub struct WorkloadParams {
    pub accounts: usize,
    pub epochs: u64,
    pub pays_per_epoch: usize,
    pub ring_size: usize,
    pub max_payload: u64,
    pub initial_mint: u64,
    pub mint_prob: f64,
    pub compact_prob: f64,
    pub k_max: u32,
}

impl Default for WorkloadParams {
    fn default() -> Self {
        WorkloadParams {
            accounts: 8,
            epochs: 4,
            pays_per_epoch: 6,
            ring_size: 3,
            max_payload: 50,
            initial_mint: 1000,
            mint_prob: 0.1,
            compact_prob: 0.2,
            k_max: Params::default().k_max,
        }
    }
}

impl WorkloadParams {
    pub fn max_balance(&self) -> u64 {
        let mints = self.initial_mint + self.epochs * 100;
        mints * self.accounts as u64
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Pay {
        sender: u32,
        ring: Vec<u32>,
        payloads: Vec<u64>,
    },
    Compact {
        account: u32,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EpochPlan {
    pub mints: Vec<(u32, u64)>,
    pub ops: Vec<Op>,
}

pub fn generate(p: &WorkloadParams, rng: &mut impl Rng) -> Vec<EpochPlan> {
    let n = p.accounts as u32;
    assert!(n >= 2 && p.ring_size >= 1 && p.ring_size < p.accounts);
    let mut live = vec![0u64; p.accounts];
    let mut plan = Vec::new();
    for e in 1..=p.epochs {
        let mut ep = EpochPlan::default();
        for id in 1..=n {
            let amount = if e == 1 {
                p.initial_mint
            } else if rng.gen_bool(p.mint_prob) {
                rng.gen_range(1..=100)
            } else {
                0
            };
            if amount > 0 {
                ep.mints.push((id, amount));
                live[id as usize - 1] += amount;
            }
        }
        let snapshot = live.clone();
        let mut credits = vec![0u32; p.accounts];
        let mut senders: Vec<u32> = (1..=n).collect();
        senders.shuffle(rng);
        for &s in senders.iter().take(p.pays_per_epoch) {
            let mut others: Vec<u32> = (1..=n)
                .filter(|&x| x != s && credits[x as usize - 1] < p.k_max)
                .collect();
            if others.len() < p.ring_size {
                continue;
            }
            others.shuffle(rng);
            let ring: Vec<u32> = others[..p.ring_size].to_vec();
            let cap = p.max_payload.min(snapshot[s as usize - 1]);
            let mut payloads = vec![0u64; p.ring_size];
            payloads[0] = rng.gen_range(0..=cap);
            for &r in &ring {
                credits[r as usize - 1] += 1;
            }
            live[s as usize - 1] -= payloads[0];
            live[ring[0] as usize - 1] += payloads[0];
            ep.ops.push(Op::Pay {
                sender: s,
                ring,
                payloads,
            });
        }
        for id in 1..=n {
            if e > 1 && rng.gen_bool(p.compact_prob) {
                ep.ops.push(Op::Compact { account: id });
            }
        }
        plan.push(ep);
    }
    plan
}

#[derive(Clone, Debug)]
pub enum Order {
    AsPlanned,
    Seeded(u64),
    Fixed(Vec<Vec<usize>>),
}

#[derive(Clone, Debug)]
pub struct RunConfig {
    pub build_seed: u64,
    pub order: Order,
    pub check_reads: bool,
    pub check_each_op: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunStats {
    pub pays: u64,
    pub credits: u64,
    pub compactions: u64,
    pub mints: u64,
    pub rejected: u64,
    pub native_mirror_mismatches: u64,
    pub read_failures: u64,
    pub w_mismatches: u64,
    pub key_failures: u64,
}

impl RunStats {
    pub fn all_ok(&self) -> bool {
        self.rejected == 0
            && self.native_mirror_mismatches == 0
            && self.read_failures == 0
            && self.w_mismatches == 0
            && self.key_failures == 0
    }
}

pub struct Outcome<B: Backend> {
    pub accounts: Vec<AccountState<B>>,
    pub w: Vec<B::Gt>,
    pub balances: Vec<u64>,
    pub stats: RunStats,
    pub failures: Vec<String>,
    pub mirror: Ledger<B>,
    pub trace: Vec<TraceOp<B>>,
}

impl<B: Backend> Outcome<B> {
    /// Final state without the record hash chains, which commit to arrival order by design.
    pub fn algebraic_state(&self) -> (Vec<AccountState<B>>, Vec<B::Gt>) {
        let accts = self
            .accounts
            .iter()
            .map(|a| AccountState {
                chain: Default::default(),
                ..a.clone()
            })
            .collect();
        (accts, self.w.clone())
    }
}

enum Built<B: Backend> {
    Pay(Tx<B>, u32, Vec<u32>, Vec<u64>),
    Compact(Compaction<B>),
}

struct Runner<'a, B: Backend> {
    native: Ledger<B>,
    mirror: Ledger<B>,
    wallets: Vec<Wallet<B>>,
    live: Vec<u64>,
    seen_events: usize,
    stats: RunStats,
    failures: Vec<String>,
    table: &'a BsgsTable<B>,
    trace: Vec<TraceOp<B>>,
}

impl<B: Backend> Runner<'_, B> {
    fn fail(&mut self, msg: String) {
        if self.failures.len() < 5 {
            self.failures.push(msg);
        }
    }

    fn sync_wallets(&mut self) {
        for ev in &self.native.events[self.seen_events..] {
            for w in &mut self.wallets {
                w.observe(ev);
            }
        }
        self.seen_events = self.native.events.len();
    }

    fn check(&mut self, when: &str) {
        self.sync_wallets();
        if self.native.accounts != self.mirror.accounts {
            self.stats.native_mirror_mismatches += 1;
            self.fail(format!("{when}: native and mirror account state differ"));
        }
        let e = self.native.epoch;
        for i in 0..self.wallets.len() {
            let id = i as u32 + 1;
            let acct = self.native.accounts[i].clone();
            let w_native = self.native.w(id).expect("native ledger");
            if self.wallets[i].w_live(&acct) != w_native {
                self.stats.w_mismatches += 1;
                self.fail(format!("{when}: account {id}: native W != W from records"));
            }
            match self.wallets[i].read_live(&acct, e, self.table) {
                Some(b) if b == self.live[i] => {}
                got => {
                    self.stats.read_failures += 1;
                    self.fail(format!(
                        "{when}: account {id}: read {got:?}, plaintext {}",
                        self.live[i]
                    ));
                }
            }
        }
    }

    fn check_epoch_start(&mut self, check_reads: bool) {
        let e = self.native.epoch;
        for i in 0..self.wallets.len() {
            let acct = self.native.accounts[i].clone();
            let pk = crate::apply::rolled(&acct, e).pk_base;
            let ok = self.wallets[i].key(e).map(|k| k.pk == pk).unwrap_or(false);
            if !ok || self.wallets[i].key_update_mismatches > 0 {
                self.stats.key_failures += 1;
                self.fail(format!(
                    "epoch {e}: account {}: key does not match pk_base",
                    i + 1
                ));
                self.wallets[i].key_update_mismatches = 0;
            }
            if check_reads {
                let got = self.wallets[i].read_snapshot(&acct, e, self.table);
                if got != Some(self.live[i]) {
                    self.stats.read_failures += 1;
                    self.fail(format!(
                        "epoch {e}: account {}: snapshot read {got:?}, plaintext {}",
                        i + 1,
                        self.live[i]
                    ));
                }
            }
        }
    }

    fn build(&mut self, op: &Op, rng: &mut ChaCha20Rng) -> Option<Built<B>> {
        let e = self.native.epoch;
        match op {
            Op::Pay {
                sender,
                ring,
                payloads,
            } => {
                let si = *sender as usize - 1;
                let keys = self.wallets[si].key(e).ok()?;
                let view = SenderView {
                    crs: &self.native.crs,
                    epoch: e,
                    sender_id: *sender,
                    sender_keys: &keys,
                    sender_balance: self.live[si],
                    sender_state: &self.native.accounts[si],
                    sender_records: &self.wallets[si].records,
                    ring: ring
                        .iter()
                        .map(|&id| (id, &self.native.accounts[id as usize - 1]))
                        .collect(),
                    payloads: payloads.clone(),
                };
                let (tx, _) = transact(&view, rng).ok()?;
                Some(Built::Pay(tx, *sender, ring.clone(), payloads.clone()))
            }
            Op::Compact { account } => {
                let i = *account as usize - 1;
                let keys = self.wallets[i].key(e).ok()?;
                Some(Built::Compact(build_compaction(
                    *account,
                    e,
                    &keys,
                    self.live[i],
                    &self.native.accounts[i],
                    &self.wallets[i].records,
                )))
            }
        }
    }

    fn apply(&mut self, b: &Built<B>) {
        let op = match b {
            Built::Pay(tx, ..) => TraceOp::Transact(tx.clone()),
            Built::Compact(c) => TraceOp::Compact(c.clone()),
        };
        let (rn, rm) = (self.native.apply_op(&op), self.mirror.apply_op(&op));
        self.trace.push(op);
        if rn != rm {
            self.stats.native_mirror_mismatches += 1;
            self.fail(format!(
                "epoch {}: native {rn:?} but mirror {rm:?}",
                self.native.epoch
            ));
        }
        if let Err(err) = rn {
            self.stats.rejected += 1;
            self.fail(format!(
                "epoch {}: honest operation rejected: {err}",
                self.native.epoch
            ));
            return;
        }
        match b {
            Built::Pay(_, s, ring, pls) => {
                self.stats.pays += 1;
                self.stats.credits += ring.len() as u64;
                self.live[*s as usize - 1] -= pls.iter().sum::<u64>();
                for (&r, &pl) in ring.iter().zip(pls) {
                    self.live[r as usize - 1] += pl;
                }
            }
            Built::Compact(_) => self.stats.compactions += 1,
        }
    }
}

pub fn run<B: Backend>(
    plan: &[EpochPlan],
    p: &WorkloadParams,
    cfg: &RunConfig,
    table: &BsgsTable<B>,
) -> Outcome<B> {
    let (crs, _) = setup_from_seed::<B>(b"workload");
    let params = Params {
        k_max: p.k_max,
        ..Params::default()
    };
    let mut runner = Runner {
        native: Ledger::new_native(crs.clone(), params.clone()),
        mirror: Ledger::new_mirror(crs, params),
        wallets: vec![],
        live: vec![0; p.accounts],
        seen_events: 0,
        stats: RunStats::default(),
        failures: vec![],
        table,
        trace: vec![],
    };
    let mut rng = ChaCha20Rng::seed_from_u64(cfg.build_seed);
    for _ in 0..p.accounts {
        let keys = keygen::<B, _>(&mut rng);
        let (c, d) = fresh_account::<B, _>(&keys.pk, &mut rng);
        let id = runner.native.register(keys.pk, c, d).expect("register");
        runner.mirror.register(keys.pk, c, d).expect("register");
        runner.trace.push(TraceOp::Register { pk: keys.pk, c, d });
        runner.wallets.push(Wallet::new(id, &keys));
    }

    for (k, ep) in plan.iter().enumerate() {
        if k > 0 {
            runner.native.next_epoch();
            runner.mirror.next_epoch();
            runner.trace.push(TraceOp::NextEpoch);
        }
        let e = runner.native.epoch;
        for &(id, amount) in &ep.mints {
            runner.native.mint(id, amount).expect("mint");
            runner.mirror.mint(id, amount).expect("mint");
            runner.trace.push(TraceOp::Mint { id, amount });
            runner.live[id as usize - 1] += amount;
            runner.stats.mints += 1;
        }
        runner.sync_wallets();
        runner.check_epoch_start(cfg.check_reads);

        let mut build_rng = ChaCha20Rng::seed_from_u64(cfg.build_seed ^ (e << 32));
        let built: Vec<Option<Built<B>>> = ep
            .ops
            .iter()
            .map(|op| runner.build(op, &mut build_rng))
            .collect();

        let order: Vec<usize> = match &cfg.order {
            Order::AsPlanned => (0..built.len()).collect(),
            Order::Seeded(seed) => {
                let mut idx: Vec<usize> = (0..built.len()).collect();
                idx.shuffle(&mut ChaCha20Rng::seed_from_u64(seed ^ (e << 40)));
                idx
            }
            Order::Fixed(perms) => perms
                .get(k)
                .cloned()
                .unwrap_or_else(|| (0..built.len()).collect()),
        };
        for i in order {
            match &built[i] {
                Some(b) => runner.apply(b),
                None => {
                    runner.stats.rejected += 1;
                    runner.fail(format!("epoch {e}: could not build operation {i}"));
                }
            }
            if cfg.check_each_op {
                runner.check(&format!("epoch {e}, after op {i}"));
            }
        }
        if cfg.check_reads && !cfg.check_each_op {
            runner.check(&format!("end of epoch {e}"));
        } else if runner.native.accounts != runner.mirror.accounts {
            runner.stats.native_mirror_mismatches += 1;
            runner.fail(format!(
                "end of epoch {e}: native and mirror account state differ"
            ));
        }
    }
    let ids = 1..=p.accounts as u32;
    Outcome {
        w: ids.map(|id| runner.native.w(id).expect("native")).collect(),
        accounts: runner.native.accounts.clone(),
        balances: runner.live.clone(),
        stats: runner.stats,
        failures: runner.failures,
        mirror: runner.mirror,
        trace: runner.trace,
    }
}
