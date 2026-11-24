//! The conflict model against the real ledger, and simple facts about each baseline.

use btp_algebra::exp::Exp61;
use btp_protocol::driver::Driver;
use btp_protocol::ProtocolError;
use btp_sim::model::*;

#[test]
fn ours_matches_the_real_ledger() {
    let mut seen = (0, 0, 0);
    for seed in 0..6u64 {
        let cfg = Config {
            accounts: 10,
            ring: 3,
            rate: 4.0,
            epoch_len: 4,
            k_max: 2,
            epochs: 5,
            drain_epochs: 20,
            ..Config::default()
        };
        let arr = arrivals(&cfg, seed);
        let mut log = vec![];
        simulate(Scheme::Ours, &cfg, &arr, seed, Some(&mut log), None);

        let mut d = Driver::<Exp61>::new(b"sim validation", cfg.accounts, cfg.k_max, seed);
        d.mint_all(1_000_000);
        for (block, p, decision) in log {
            while d.epoch() < block / cfg.epoch_len + 1 {
                d.next_epoch();
            }
            let sender = arr.sender[p] + 1;
            let ring: Vec<u32> = arr.ring_of(p).iter().map(|m| m + 1).collect();
            let mut pls = vec![0u64; ring.len()];
            pls[0] = 1;
            let tx = d.build_pay(sender, &ring, &pls);
            let got = d.try_submit(tx, &pls);
            let want = match decision {
                Decision::Land => Ok(()),
                Decision::DeferDebited => Err(ProtocolError::AlreadyDebited),
                Decision::DeferCap => Err(ProtocolError::CreditCap),
            };
            assert_eq!(got, want, "seed {seed}, block {block}, payment {p}");
            match decision {
                Decision::Land => seen.0 += 1,
                Decision::DeferDebited => seen.1 += 1,
                Decision::DeferCap => seen.2 += 1,
            }
        }
    }
    assert!(
        seen.0 > 100 && seen.1 > 20 && seen.2 > 20,
        "decisions seen: {seen:?}"
    );
}

fn run(scheme: Scheme, cfg: &Config, seed: u64) -> (Arrivals, Vec<Outcome>) {
    let arr = arrivals(cfg, seed);
    let out = simulate(scheme, cfg, &arr, seed, None, None);
    (arr, out)
}

#[test]
fn baseline_facts() {
    let cfg = Config {
        accounts: 2_000,
        rate: 30.0,
        epochs: 40,
        ..Config::default()
    };
    for scheme in Scheme::ALL {
        let (arr, out) = run(scheme, &cfg, 1);
        let st = stats(scheme, &arr, &out);
        assert_eq!(st.payments, arr.len());
        assert_eq!(st.landed + st.unfinished, st.payments);
        for (i, o) in out.iter().enumerate() {
            if let Some(b) = o.landed {
                assert!(b > arr.block[i], "lands after it arrives");
            }
            if let (Some(f), Some(b)) = (o.fs, o.landed) {
                assert!(f >= b, "{scheme:?}: forward-secure before landing");
            }
        }
        match scheme {
            Scheme::PrideCt => assert_eq!(st.fs_fraction, 0.0, "no key evolution"),
            Scheme::ConstructionII => assert_eq!(st.messages_per_payment, 2.0),
            Scheme::Ours | Scheme::PriDeFulOption1 => assert_eq!(st.attempts_mean, 1.0),
            Scheme::PriDeFulOption2 => {
                assert!(st.attempts_mean > 1.0, "conflicts force resubmissions")
            }
        }
        assert_eq!(st.unfinished, 0, "{scheme:?} drains at this load");
    }
}

#[test]
fn no_concurrency_no_conflict() {
    let cfg = Config {
        accounts: 1_000,
        rate: 0.05,
        epochs: 50,
        ..Config::default()
    };
    let (arr, out) = run(Scheme::PriDeFulOption2, &cfg, 3);
    let s2 = stats(Scheme::PriDeFulOption2, &arr, &out);
    let (arr1, out1) = run(Scheme::PriDeFulOption1, &cfg, 3);
    let s1 = stats(Scheme::PriDeFulOption1, &arr1, &out1);
    assert!(s2.first_try_rate > 0.99, "{s2:?}");
    assert!(s1.fs_fraction > 0.99, "{s1:?}");
}

#[test]
fn e5_round_model() {
    use rand::SeedableRng;
    let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(9);
    let rate = |s: usize, rng: &mut rand_chacha::ChaCha20Rng| {
        let mut ok = 0;
        for _ in 0..2000 {
            ok += e5_round(rng, 64, s, 4, 8).0;
        }
        ok as f64 / (2000 * s) as f64
    };
    assert_eq!(rate(1, &mut rng), 1.0);
    let (r4, r16) = (rate(4, &mut rng), rate(16, &mut rng));
    assert!(r4 < 1.0 && r16 < r4, "{r4} {r16}");
}
