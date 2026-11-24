//! Native and mirror ledgers, and order-independence at test scale.

use btp_algebra::bls::Bls;
use btp_algebra::exp::{Exp61, ExpBls};
use btp_algebra::Backend;
use btp_protocol::read::BsgsTable;
use btp_protocol::workload::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

fn permutations(n: usize) -> Vec<Vec<usize>> {
    if n == 0 {
        return vec![vec![]];
    }
    let mut out = vec![];
    for p in permutations(n - 1) {
        for k in 0..=p.len() {
            let mut q = p.clone();
            q.insert(k, n - 1);
            out.push(q);
        }
    }
    out
}

fn pay(sender: u32, ring: &[u32], payloads: &[u64]) -> Op {
    Op::Pay {
        sender,
        ring: ring.to_vec(),
        payloads: payloads.to_vec(),
    }
}

fn cfg(build_seed: u64, order: Order) -> RunConfig {
    RunConfig {
        build_seed,
        order,
        check_reads: true,
        check_each_op: false,
    }
}

fn all_orders_agree<B: Backend>(
    plan: &[EpochPlan],
    p: &WorkloadParams,
    orders: Vec<Order>,
) -> usize {
    let table = BsgsTable::<B>::new(p.max_balance());
    let mut first: Option<_> = None;
    let mut chains = std::collections::HashSet::new();
    for (k, order) in orders.into_iter().enumerate() {
        let out = run::<B>(plan, p, &cfg(7, order), &table);
        assert!(
            out.stats.all_ok(),
            "order {k}: {:?} {:?}",
            out.stats,
            out.failures
        );
        chains.insert(
            out.accounts
                .iter()
                .map(|a| a.chain.clone())
                .collect::<Vec<_>>(),
        );
        let state = (out.algebraic_state(), out.balances.clone());
        match &first {
            None => first = Some(state),
            Some(f) => assert!(*f == state, "order {k} ends in a different state"),
        }
    }
    chains.len()
}

#[test]
fn port_debit_check_24_orders() {
    let p = WorkloadParams {
        accounts: 6,
        epochs: 2,
        ring_size: 1,
        ..Default::default()
    };
    let mints: Vec<(u32, u64)> = (1..=6).map(|id| (id, 5000)).collect();
    let plan = vec![
        EpochPlan {
            mints,
            ops: vec![
                pay(2, &[1], &[10]),
                pay(3, &[1], &[20]),
                pay(4, &[1], &[30]),
                pay(5, &[1], &[40]),
            ],
        },
        EpochPlan {
            mints: vec![],
            ops: vec![
                pay(2, &[1], &[7]),
                pay(3, &[1], &[11]),
                pay(1, &[6], &[300]),
                pay(4, &[1], &[13]),
            ],
        },
    ];
    let orders = permutations(4)
        .into_iter()
        .map(|q| Order::Fixed(vec![vec![0, 1, 2, 3], q]))
        .collect();
    assert_eq!(all_orders_agree::<Exp61>(&plan, &p, orders), 24);
}

#[test]
fn port_tdddh_120_orders() {
    let p = WorkloadParams {
        accounts: 6,
        epochs: 1,
        ring_size: 1,
        ..Default::default()
    };
    let plan = vec![EpochPlan {
        mints: (1..=6).map(|id| (id, 1000)).collect(),
        ops: (2..=6).map(|s| pay(s, &[1], &[s as u64 * 3])).collect(),
    }];
    let orders = permutations(5)
        .into_iter()
        .map(|q| Order::Fixed(vec![q]))
        .collect();
    assert_eq!(
        all_orders_agree::<Exp61>(&plan, &p, orders),
        120,
        "all 120 orders really differ"
    );
}

#[test]
fn port_onchain_check_30_workloads_20_orders() {
    for seed in 0..30u64 {
        let p = WorkloadParams {
            accounts: 6,
            epochs: 4,
            pays_per_epoch: 5,
            ring_size: 2,
            compact_prob: 0.5,
            ..Default::default()
        };
        let plan = generate(&p, &mut ChaCha20Rng::seed_from_u64(seed));
        let orders = (0..20).map(|o| Order::Seeded(seed * 1000 + o)).collect();
        all_orders_agree::<Exp61>(&plan, &p, orders);
    }
}

#[test]
fn property_random_workloads() {
    let cases = 200u64;
    let mut r = ChaCha20Rng::seed_from_u64(2026);
    for case in 0..cases {
        let accounts = r.gen_range(3..=10);
        let p = WorkloadParams {
            accounts,
            epochs: r.gen_range(1..=5),
            pays_per_epoch: r.gen_range(1..=accounts),
            ring_size: r.gen_range(1..accounts.min(5)),
            max_payload: r.gen_range(1..=200),
            mint_prob: 0.2,
            compact_prob: r.gen_range(0.0..0.5),
            k_max: r.gen_range(1..=8),
            ..Default::default()
        };
        let plan = generate(&p, &mut ChaCha20Rng::seed_from_u64(case));
        let orders = (0..3).map(|o| Order::Seeded(case * 10 + o)).collect();
        all_orders_agree::<Exp61>(&plan, &p, orders);
    }
}

#[test]
fn bls_small_workload() {
    let p = WorkloadParams {
        accounts: 4,
        epochs: 3,
        pays_per_epoch: 3,
        ring_size: 2,
        compact_prob: 0.4,
        max_payload: 20,
        initial_mint: 100,
        mint_prob: 0.0,
        ..Default::default()
    };
    let plan = generate(&p, &mut ChaCha20Rng::seed_from_u64(5));
    all_orders_agree::<Bls>(&plan, &p, vec![Order::Seeded(1), Order::Seeded(2)]);
}

#[test]
fn expbls_and_bls_agree() {
    let p = WorkloadParams {
        accounts: 4,
        epochs: 3,
        pays_per_epoch: 3,
        ring_size: 2,
        compact_prob: 0.4,
        max_payload: 20,
        initial_mint: 100,
        mint_prob: 0.0,
        ..Default::default()
    };
    let plan = generate(&p, &mut ChaCha20Rng::seed_from_u64(9));
    let c = cfg(3, Order::Seeded(3));
    let a = run::<ExpBls>(&plan, &p, &c, &BsgsTable::new(p.max_balance()));
    let b = run::<Bls>(&plan, &p, &c, &BsgsTable::new(p.max_balance()));
    assert!(
        a.stats.all_ok() && b.stats.all_ok(),
        "{:?} {:?}",
        a.failures,
        b.failures
    );
    assert_eq!(a.stats, b.stats);
    assert_eq!(a.balances, b.balances);
    assert_eq!(a.mirror.counters, b.mirror.counters);
    use btp_algebra::GroupElem;
    for (x, y) in a.accounts.iter().zip(&b.accounts) {
        assert_eq!(Bls::g1().smul(&x.c.log), y.c);
        assert_eq!(Bls::g1().smul(&x.pk_base.log), y.pk_base);
    }
}
