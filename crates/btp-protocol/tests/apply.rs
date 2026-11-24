//! Apply: unit checks, and the invariants after every single operation.

use btp_algebra::exp::Exp61;
use btp_algebra::{Backend, GroupElem};
use btp_protocol::apply::*;
use btp_protocol::crs::setup_from_seed;
use btp_protocol::ledger::{Event, Ledger};
use btp_protocol::read::BsgsTable;
use btp_protocol::transact::{fresh_account, keygen};
use btp_protocol::workload::{generate, run, Order, RunConfig, WorkloadParams};
use btp_protocol::*;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

type B = Exp61;

fn account(r: &mut ChaCha20Rng) -> AccountState<B> {
    let k = keygen::<B, _>(r);
    let (c, d) = fresh_account::<B, _>(&k.pk, r);
    new_account(k.pk, c, d, 1)
}

#[test]
fn rollover_moves_key_and_snapshot() {
    let mut r = ChaCha20Rng::seed_from_u64(1);
    let mut a = account(&mut r);
    assert!(rollover(&mut a, 1).is_none(), "same epoch: nothing to do");
    let g = B::g1();
    a.c = a.c + g;
    a.d = a.d + g.smul(&5u64.into());
    a.dpk = g.smul(&7u64.into());
    a.up_agg = (g, g.smul(&3u64.into()));
    a.credits_this_epoch = 4;
    let (pk, c, d) = (a.pk_base, a.c, a.d);

    let log = rollover(&mut a, 3).expect("new epoch");
    assert_eq!(log.epoch, 1);
    assert_eq!(log.dpk, g.smul(&7u64.into()));
    assert_eq!(log.up_agg, (g, g.smul(&3u64.into())));
    assert_eq!(
        a.pk_base,
        pk + g.smul(&7u64.into()),
        "pk_base ← pk_base + dPK"
    );
    assert_eq!((a.c_base, a.d_base), (c, d), "snapshot ← live");
    assert!(a.dpk.is_zero() && a.up_agg.0.is_zero() && a.up_agg.1.is_zero());
    assert_eq!((a.credits_this_epoch, a.snap_epoch), (0, 3));
    let again = a.clone();
    assert!(rollover(&mut a, 3).is_none());
    assert_eq!(a, again, "rollover is idempotent within an epoch");
}

#[test]
fn mint_adds_to_live_and_snapshot() {
    let mut r = ChaCha20Rng::seed_from_u64(2);
    let mut a = account(&mut r);
    let (d, db) = (a.d, a.d_base);
    mint(&mut a, 1, 42);
    let m = B::g1().smul(&42u64.into());
    assert_eq!((a.d, a.d_base), (d + m, db + m));
}

#[test]
fn register_refuses_zero_ciphertext() {
    let (crs, _) = setup_from_seed::<B>(b"r");
    let mut l = Ledger::new_mirror(crs, Params::default());
    let mut r = ChaCha20Rng::seed_from_u64(3);
    let k = keygen::<B, _>(&mut r);
    let z = <B as Backend>::G1::zero();
    assert_eq!(l.register(k.pk, z, z), Err(ProtocolError::BadRegistration));
    let (c, d) = fresh_account::<B, _>(&k.pk, &mut r);
    assert_eq!(l.register(k.pk, c, d), Ok(1));
    assert!(matches!(l.events[0], Event::Registered { id: 1, .. }));
}

#[test]
fn normalisation_drops_old_epochs() {
    let p = WorkloadParams {
        accounts: 5,
        epochs: 4,
        pays_per_epoch: 5,
        ring_size: 2,
        compact_prob: 0.0,
        ..Default::default()
    };
    let mut r = ChaCha20Rng::seed_from_u64(4);
    let plan = generate(&p, &mut r);
    let cfg = RunConfig {
        build_seed: 4,
        order: Order::AsPlanned,
        check_reads: false,
        check_each_op: false,
    };
    let out = run::<B>(&plan, &p, &cfg, &BsgsTable::new(1));
    assert!(out.stats.all_ok(), "{:?}", out.failures);
    for (i, a) in out.accounts.iter().enumerate() {
        let id = i as u32 + 1;
        let debited = plan
            .last()
            .unwrap()
            .ops
            .iter()
            .any(|op| matches!(op, workload::Op::Pay { sender, .. } if *sender == id));
        if debited {
            assert_eq!(a.debited_epoch, 4);
            assert!(
                a.open_epochs.iter().all(|&e| e == 4),
                "account {id}: {:?}",
                a.open_epochs
            );
        }
        let keys: Vec<u64> = a.chain.keys().copied().collect();
        assert_eq!(
            keys, a.open_epochs,
            "chain entries exist exactly for open epochs"
        );
    }
}

#[test]
fn invariants_after_every_operation() {
    for seed in 0..12 {
        let p = WorkloadParams {
            accounts: 6,
            epochs: 4,
            pays_per_epoch: 5,
            ring_size: 2,
            compact_prob: 0.3,
            mint_prob: 0.3,
            ..Default::default()
        };
        let mut r = ChaCha20Rng::seed_from_u64(100 + seed);
        let plan = generate(&p, &mut r);
        let cfg = RunConfig {
            build_seed: seed,
            order: Order::Seeded(seed),
            check_reads: true,
            check_each_op: true,
        };
        let out = run::<B>(&plan, &p, &cfg, &BsgsTable::new(p.max_balance()));
        assert!(
            out.stats.all_ok(),
            "seed {seed}: {:?} {:?}",
            out.stats,
            out.failures
        );
        assert!(out.stats.pays > 0 && out.stats.compactions > 0);
    }
}
