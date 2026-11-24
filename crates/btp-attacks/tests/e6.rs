//! E6 at test scale, and the rank test against the reference ranks.

use btp_algebra::bls::Bls;
use btp_algebra::exp::Exp61;
use btp_algebra::Backend;
use btp_attacks::attacks::{applies, expected_break};
use btp_attacks::designs::Ctx;
use btp_attacks::game::run_row;
use btp_attacks::rank::{cases, run_case};
use btp_attacks::{Attack, Design};

fn all_rows<B: Backend>(trials: u64) {
    let ctx = Ctx::<B>::new();
    for d in Design::ALL {
        for a in Attack::ALL {
            if !applies(d, a) {
                continue;
            }
            let res = run_row(&ctx, d, a, trials, 42);
            let row = format!("{} / {a:?}: {res:?}", d.name());
            if expected_break(d, a) {
                assert_eq!(res.successes, trials, "should break: {row}");
                assert_eq!(res.decided, trials, "{row}");
            } else {
                assert_eq!(res.decided, 0, "attack must find nothing: {row}");
                if trials >= 1000 {
                    let r = res.rate();
                    assert!((0.45..=0.55).contains(&r), "should be guessing: {row}");
                }
            }
        }
    }
}

#[test]
fn attack_rows_exp61() {
    all_rows::<Exp61>(2000);
}

#[test]
fn attack_rows_bls() {
    all_rows::<Bls>(6);
}

#[test]
fn same_seed_same_result() {
    let ctx = Ctx::<Exp61>::new();
    let a = run_row(&ctx, Design::Final, Attack::A3, 500, 7);
    let b = run_row(&ctx, Design::Final, Attack::A3, 500, 7);
    assert_eq!(a, b);
}

#[test]
fn rank_test_matches_python() {
    type Ranks = (usize, usize, usize);
    let expected: [(Ranks, Ranks); 16] = [
        ((0, 0, 9), (0, 0, 9)),
        ((0, 0, 8), (0, 0, 9)),
        ((0, 0, 36), (0, 0, 37)),
        ((0, 0, 236), (0, 0, 237)),
        ((0, 0, 248), (0, 0, 248)),
        ((0, 0, 409), (0, 0, 409)),
        ((0, 0, 394), (0, 0, 394)),
        ((0, 0, 198), (0, 0, 198)),
        ((0, 0, 409), (0, 0, 409)),
        ((19, 8, 125), (19, 8, 125)),
        ((18, 8, 117), (18, 8, 117)),
        ((17, 8, 112), (17, 8, 112)),
        ((19, 8, 124), (19, 8, 125)),
        ((19, 6, 82), (19, 6, 84)),
        ((19, 8, 125), (20, 8, 133)),
        ((15, 8, 96), (16, 8, 104)),
    ];
    let cs = cases();
    assert_eq!(cs.len(), expected.len());
    for (c, (real, rand)) in cs.iter().zip(expected) {
        for seed in [1, 2, 3] {
            let r = run_case(c, seed);
            assert_eq!(
                (r.real, r.rand),
                (real, rand),
                "{} / {} (seed {seed})",
                c.game,
                c.name
            );
            assert_eq!(r.leak(), c.expected_leak, "{} / {}", c.game, c.name);
        }
    }
}
