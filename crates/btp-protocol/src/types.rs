//! Data types of SPEC 5-10.

use btp_algebra::Backend;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crs<B: Backend> {
    pub g1: B::G1,
    pub beta1: B::G1,
    pub beta2_1: B::G1,
    pub eta1: B::G1,
    pub etabeta1: B::G1,
    pub kappa1: B::G1,
    pub kappabeta1: B::G1,
    pub g2: B::G2,
    pub beta_2: B::G2,
    pub beta2_2: B::G2,
    pub eta_2: B::G2,
    pub kappa_2: B::G2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trapdoor<B: Backend> {
    pub beta: B::Fr,
    pub eta: B::Fr,
    pub kappa: B::Fr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keypair<B: Backend> {
    pub sk: B::Fr,
    pub pk: B::G1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record<B: Backend> {
    pub t: B::Fr,
    pub c: B::G1,
    pub r1: B::G1,
    pub r2: B::G1,
    pub h0: B::G2,
    pub h1: B::G2,
    pub h2: B::G2,
    pub is_update: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recv<B: Backend> {
    pub d: B::G1,
    pub up: (B::G1, B::G1),
    pub dpk: B::G1,
    pub e: B::G1,
    pub side: [u8; 32],
    pub h0: B::G2,
    pub h1: B::G2,
    pub h2: B::G2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpochWitness<B: Backend> {
    pub epoch: u64,
    pub records: Vec<Record<B>>,
    pub hints: Vec<Vec<B::Fr>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendWitness<B: Backend> {
    pub epochs: Vec<EpochWitness<B>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tx<B: Backend> {
    pub epoch: u64,
    pub sender: u32,
    pub ring: Vec<u32>,
    pub c: B::G1,
    pub d0: B::G1,
    pub n0: B::G1,
    pub r1: B::G1,
    pub r2: B::G1,
    pub recv: Vec<Recv<B>>,
    pub spend: SpendWitness<B>,
}

#[derive(Clone, Debug)]
pub struct TxSecrets<B: Backend> {
    pub r: B::Fr,
    pub delta: Vec<B::Fr>,
    pub rho: Vec<B::Fr>,
    pub tau: Vec<B::Fr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compaction<B: Backend> {
    pub account: u32,
    pub epoch: u64,
    pub d_prime: B::G1,
    pub witness: SpendWitness<B>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyUpdate<B: Backend> {
    pub epoch: u64,
    pub dpk: B::G1,
    pub up_agg: (B::G1, B::G1),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountState<B: Backend> {
    pub pk_base: B::G1,
    pub c: B::G1,
    pub d: B::G1,
    pub c_base: B::G1,
    pub d_base: B::G1,
    pub dpk: B::G1,
    pub up_agg: (B::G1, B::G1),
    pub snap_epoch: u64,
    pub credits_this_epoch: u32,
    pub debited_epoch: u64,
    pub open_epochs: Vec<u64>,
    pub chain: BTreeMap<u64, [u8; 32]>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpCounters {
    pub pairings: u64,
    pub msm_terms: u64,
    pub g1_adds: u64,
}
