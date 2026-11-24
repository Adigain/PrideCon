//! Attacks A1-A5, run against every design; each answers only if exactly one candidate fits.

use crate::designs::PublicView;
use crate::{Attack, Design, Game};
use btp_algebra::{Backend, GroupElem};
use btp_protocol::read::cross_term;

impl Attack {
    pub const ALL: [Attack; 5] = [Attack::A1, Attack::A2, Attack::A3, Attack::A4, Attack::A5];

    pub fn game(self) -> Game {
        match self {
            Attack::A4 => Game::OldBalance,
            _ => Game::Payload,
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Attack::A1 => "payload check with public [s]_2 (3.4.1)",
            Attack::A2 => "rebuild [s]_2 = [s']_2 - [Delta]_2, then payload check (3.4.3)",
            Attack::A3 => "e(D-C*s', g2) + e(C, [Delta]_2) = [pl]_T (3.4.3 item 1)",
            Attack::A4 => "oD + E_old - oC*s' = [b]_1, old balance (3.4.3 item 2)",
            Attack::A5 => "known x strips the mask of E (Remark 1 iv)",
        }
    }
}

pub fn applies(design: Design, attack: Attack) -> bool {
    match design {
        Design::DualKey => matches!(attack, Attack::A1 | Attack::A2 | Attack::A3),
        Design::MethodA => matches!(attack, Attack::A2 | Attack::A3 | Attack::A4),
        Design::Final | Design::FinalTrapdoor | Design::FinalX0 => true,
    }
}

pub fn expected_break(design: Design, attack: Attack) -> bool {
    match design {
        Design::DualKey => true,
        Design::MethodA => true,
        Design::Final => false,
        Design::FinalTrapdoor => matches!(attack, Attack::A2 | Attack::A3),
        Design::FinalX0 => matches!(attack, Attack::A5),
    }
}

fn decide(cands: [u64; 2], fits: impl Fn(u64) -> bool) -> Option<usize> {
    match (fits(cands[0]), fits(cands[1])) {
        (true, false) => Some(0),
        (false, true) => Some(1),
        _ => None,
    }
}

fn delta2_estimate<B: Backend>(v: &PublicView<B>) -> Option<B::G2> {
    let sum = |f: &dyn Fn(&crate::designs::CreditView<B>) -> Option<B::G2>| {
        v.credits
            .iter()
            .map(f)
            .try_fold(B::G2::zero(), |a, x| x.map(|x| a + x))
    };
    if let Some(beta) = v.trapdoor_beta {
        return sum(&|c| c.record.as_ref().map(|r| r.h0 - r.h2.smul(&(beta * beta))));
    }
    sum(&|c| c.dpk2).or_else(|| sum(&|c| c.record.as_ref().map(|r| r.h0)))
}

fn payload_check<B: Backend>(v: &PublicView<B>, pk2: B::G2, cands: [u64; 2]) -> Option<usize> {
    let ch = &v.credits[0];
    let lhs = B::pair(&ch.c, &pk2);
    decide(cands, |m| {
        lhs == B::pair(&(ch.d - B::g1().smul(&B::Fr::from(m))), &B::g2())
    })
}

pub fn run_attack<B: Backend>(attack: Attack, v: &PublicView<B>, cands: [u64; 2]) -> Option<usize> {
    let (g1, g2) = (B::g1(), B::g2());
    let s1 = v.leaked_key;
    let ch = &v.credits[0];
    let gt_of = |m: u64| B::gt().smul(&B::Fr::from(m));
    match attack {
        Attack::A1 => payload_check(v, v.pk2?, cands),
        Attack::A2 => {
            let pk2 = g2.smul(&s1) - delta2_estimate(v)?;
            payload_check(v, pk2, cands)
        }
        Attack::A3 => {
            let val = B::pair(&(ch.d - ch.c.smul(&s1)), &g2) + B::pair(&ch.c, &delta2_estimate(v)?);
            decide(cands, |m| val == gt_of(m))
        }
        Attack::A4 => {
            let e_sum = v
                .credits
                .iter()
                .map(|c| c.e_old.or(c.e_comb))
                .try_fold(B::G1::zero(), |a, x| x.map(|x| a + x))?;
            let val = v.od + e_sum - v.oc.smul(&s1);
            decide(cands, |b| val == g1.smul(&B::Fr::from(b)))
        }
        Attack::A5 => {
            let x = v.known_x?;
            let rec0 = ch.record.as_ref()?;
            let stripped = ch.e_comb? - ch.dpk1.smul(&x);
            let cross = v.credits[1..]
                .iter()
                .filter_map(|c| c.record.as_ref())
                .fold(B::Gt::zero(), |a, rm| a + cross_term(rec0, rm));
            let val = B::pair(&(ch.d - ch.c.smul(&s1) + stripped), &g2) + cross;
            decide(cands, |m| val == gt_of(m))
        }
    }
}
