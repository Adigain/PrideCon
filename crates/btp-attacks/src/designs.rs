//! What each design publishes in one trial of the two-candidate game.

use crate::{Design, Game};
use ark_ff::UniformRand;
use btp_algebra::{Backend, GroupElem};
use btp_protocol::apply::{credit_record, new_account};
use btp_protocol::crs::setup_from_seed;
use btp_protocol::encode::tx_tag;
use btp_protocol::transact::{fresh_account, keygen, transact, SenderView};
use btp_protocol::{Crs, Record, Trapdoor};
use rand::{Rng, RngCore};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct CreditView<B: Backend> {
    pub c: B::G1,
    pub d: B::G1,
    pub dpk1: B::G1,
    pub dpk2: Option<B::G2>,
    pub e_old: Option<B::G1>,
    pub e_comb: Option<B::G1>,
    pub record: Option<Record<B>>,
}

#[derive(Clone, Debug)]
pub struct PublicView<B: Backend> {
    pub oc: B::G1,
    pub od: B::G1,
    pub pk2: Option<B::G2>,
    pub credits: Vec<CreditView<B>>,
    pub leaked_key: B::Fr,
    pub trapdoor_beta: Option<B::Fr>,
    pub known_x: Option<B::Fr>,
}

pub struct Ctx<B: Backend> {
    pub crs: Crs<B>,
    pub td: Trapdoor<B>,
}

impl<B: Backend> Ctx<B> {
    pub fn new() -> Self {
        let (crs, td) = setup_from_seed::<B>(b"e6 attack harness");
        Ctx { crs, td }
    }
}

impl<B: Backend> Default for Ctx<B> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn make_view<B: Backend, R: RngCore>(
    ctx: &Ctx<B>,
    design: Design,
    game: Game,
    cands: [u64; 2],
    gamma: usize,
    rng: &mut R,
) -> PublicView<B> {
    let fr = |v: u64| B::Fr::from(v);
    let (g1, g2) = (B::g1(), B::g2());
    let s = B::Fr::rand(rng);
    let pk = g1.smul(&s);
    let x = if design == Design::FinalX0 {
        B::Fr::from(0u64)
    } else {
        B::Fr::rand(rng)
    };
    // Payload game: prior balance hidden and uniform; old-balance game: payloads uniform in [0, 2^62) (Remark 4).
    let theta = match game {
        Game::Payload => B::Fr::rand(rng),
        Game::OldBalance => fr(cands[gamma]),
    };
    let (oc, od) = (g1.smul(&x), pk.smul(&x) + g1.smul(&theta));
    let payloads: [u64; 2] = match game {
        Game::Payload => [cands[gamma], rng.gen_range(0..1 << 16)],
        Game::OldBalance => [rng.gen_range(0..1 << 62), rng.gen_range(0..1 << 62)],
    };

    let (credits, deltas): (Vec<CreditView<B>>, Vec<B::Fr>) = match design {
        Design::DualKey | Design::MethodA => payloads
            .iter()
            .map(|&pl| {
                let (r, delta) = (B::Fr::rand(rng), B::Fr::rand(rng));
                let cv = CreditView {
                    c: g1.smul(&r),
                    d: pk.smul(&r) + g1.smul(&fr(pl)),
                    dpk1: g1.smul(&delta),
                    dpk2: Some(g2.smul(&delta)),
                    e_old: (design == Design::MethodA).then(|| oc.smul(&delta)),
                    e_comb: None,
                    record: None,
                };
                (cv, delta)
            })
            .unzip(),
        Design::Final | Design::FinalTrapdoor | Design::FinalX0 => {
            final_credits(ctx, pk, oc, od, &payloads, rng)
        }
    };

    let delta_sum = deltas.iter().fold(B::Fr::from(0u64), |a, d| a + d);
    PublicView {
        oc,
        od,
        pk2: (design == Design::DualKey).then(|| g2.smul(&s)),
        credits,
        leaked_key: s + delta_sum,
        trapdoor_beta: (design == Design::FinalTrapdoor).then_some(ctx.td.beta),
        known_x: (design == Design::FinalX0).then_some(x),
    }
}

fn final_credits<B: Backend, R: RngCore>(
    ctx: &Ctx<B>,
    pk: B::G1,
    oc: B::G1,
    od: B::G1,
    payloads: &[u64; 2],
    rng: &mut R,
) -> (Vec<CreditView<B>>, Vec<B::Fr>) {
    let target = new_account::<B>(pk, oc, od, 1);
    let decoy_keys = keygen::<B, _>(rng);
    let (dc, dd) = fresh_account::<B, _>(&decoy_keys.pk, rng);
    let decoy = new_account::<B>(decoy_keys.pk, dc, dd, 1);
    let no_records = BTreeMap::new();
    let balance = u64::MAX / 2;

    payloads
        .iter()
        .enumerate()
        .map(|(k, &pl)| {
            let keys = keygen::<B, _>(rng);
            let (c, d) = fresh_account::<B, _>(&keys.pk, rng);
            let sender = new_account::<B>(keys.pk, c, d + B::g1().smul(&B::Fr::from(balance)), 1);
            let view = SenderView {
                crs: &ctx.crs,
                epoch: 1,
                sender_id: 10 + k as u32,
                sender_keys: &keys,
                sender_balance: balance,
                sender_state: &sender,
                sender_records: &no_records,
                ring: vec![(1, &target), (2, &decoy)],
                payloads: vec![pl, 0],
            };
            let (tx, sec) = transact(&view, rng).expect("honest transaction");
            let rv = &tx.recv[0];
            let cv = CreditView {
                c: tx.c,
                d: rv.d,
                dpk1: rv.dpk,
                dpk2: None,
                e_old: None,
                e_comb: Some(rv.e),
                record: Some(credit_record(&tx, 0, tx_tag(&tx))),
            };
            (cv, sec.delta[0])
        })
        .unzip()
}
