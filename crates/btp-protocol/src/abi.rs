//! Solidity ABI view of the protocol types; must match contracts/src/Types.sol field for field.

use crate::types::{AccountState, Compaction, Crs, EpochWitness, Record, Tx};
use alloy_primitives::{FixedBytes, U256};
use btp_algebra::bls::{Bls, G1Projective, G2Projective};
use btp_algebra::codec::{fr_to_bytes32, g1_to_eip2537, g2_to_eip2537};

type Fr = <Bls as btp_algebra::Backend>::Fr;

pub mod sol {
    alloy_sol_types::sol! {
        #[derive(Debug, PartialEq, Eq)]
        struct G1Point { bytes32[4] w; }
        #[derive(Debug, PartialEq, Eq)]
        struct G2Point { bytes32[8] w; }

        #[derive(Debug, PartialEq, Eq)]
        struct Crs {
            G1Point g1; G1Point beta1; G1Point beta2_1; G1Point eta1; G1Point etabeta1;
            G1Point kappa1; G1Point kappabeta1;
            G2Point g2; G2Point beta_2; G2Point beta2_2; G2Point eta_2; G2Point kappa_2;
        }

        #[derive(Debug, PartialEq, Eq)]
        struct Recv {
            G1Point d; G1Point up1; G1Point up2; G1Point dpk; G1Point e; bytes32 side;
            G2Point h0; G2Point h1; G2Point h2;
        }

        #[derive(Debug, PartialEq, Eq)]
        struct Record {
            uint256 t; G1Point c; G1Point r1; G1Point r2; G2Point h0; G2Point h1; G2Point h2; bool isUpdate;
        }

        #[derive(Debug, PartialEq, Eq)]
        struct EpochWitness { uint64 epoch; Record[] records; uint256[][] hints; }

        #[derive(Debug, PartialEq, Eq)]
        struct Tx {
            uint64 epoch; uint32 sender; uint32[] ring;
            G1Point c; G1Point d0; G1Point n0; G1Point r1; G1Point r2;
            Recv[] recv; EpochWitness[] spend;
        }

        #[derive(Debug, PartialEq, Eq)]
        struct Compaction { uint32 account; uint64 epoch; G1Point dPrime; EpochWitness[] witness; }

        #[derive(Debug, PartialEq, Eq)]
        struct AccountView {
            G1Point pkBase; G1Point c; G1Point d; G1Point cBase; G1Point dBase; G1Point dPK;
            G1Point up1; G1Point up2;
            uint64 snapEpoch; uint32 creditsThisEpoch; uint64 debitedEpoch;
            uint64[] openEpochs; bytes32[] chains;
        }

        #[derive(Debug, PartialEq, Eq)]
        interface IPrideConcurrent {
            function register(G1Point pk, G1Point c, G1Point d) external returns (uint32 id);
            function mint(uint32 id, uint64 amount) external;
            function transact(Tx txn) external;
            function compact(Compaction cp) external;
            function getAccount(uint32 id) external view returns (AccountView v);
            function currentEpoch() external view returns (uint64);
            function numAccounts() external view returns (uint32);
            function epochLen() external view returns (uint64);
            function kMax() external view returns (uint32);
            function usedTag(uint256 t) external view returns (bool);

            event Registered(uint32 indexed id, G1Point pk);
            event KeyUpdated(uint32 indexed id, uint64 epoch, G1Point dPK, G1Point up1, G1Point up2);
            event Credit(uint32 indexed id, uint64 indexed epoch, G1Point up1, G1Point up2, G1Point dpk, bytes32 side);
            event RecordAdded(uint32 indexed id, uint64 indexed epoch, Record record);

            error WrongEpoch();
            error DuplicateTag();
            error AlreadyDebited();
            error CreditCap();
            error BadRing();
            error HandleCheck();
            error SpendCheck();
            error RecordChain();
            error BadHint();
            error UnknownAccount();
            error BadRegistration();
        }

        #[derive(Debug, PartialEq, Eq)]
        struct BaseRecv { G1Point d; G1Point up1; G1Point up2; G1Point dpk; G1Point e; bytes32 side; }
        #[derive(Debug, PartialEq, Eq)]
        struct BaseTx { uint32 sender; uint32[] ring; uint64[] versions; G1Point c; G1Point d0; BaseRecv[] recv; }
        #[derive(Debug, PartialEq, Eq)]
        struct BaseAccountView { G1Point pk; G1Point c; G1Point d; uint64 version; }

        #[derive(Debug, PartialEq, Eq)]
        interface IPriDeFULBaseline {
            function register(G1Point pk, G1Point c, G1Point d) external returns (uint32 id);
            function mint(uint32 id, uint64 amount) external;
            function transact(BaseTx txn) external;
            function getAccount(uint32 id) external view returns (BaseAccountView v);
            function numAccounts() external view returns (uint32);

            event Credit(uint32 indexed id, G1Point up1, G1Point up2, G1Point dpk, bytes32 side);

            error BadRing();
            error StaleVersion();
            error UnknownAccount();
            error BadRegistration();
        }
    }
}

pub fn g1_from(p: &sol::G1Point) -> Option<G1Projective> {
    let mut b = [0u8; 128];
    for (i, w) in p.w.iter().enumerate() {
        b[32 * i..32 * (i + 1)].copy_from_slice(w.as_slice());
    }
    btp_algebra::codec::g1_from_eip2537(&b)
}

fn words<const N: usize>(b: &[u8]) -> [FixedBytes<32>; N] {
    std::array::from_fn(|i| FixedBytes::from_slice(&b[32 * i..32 * (i + 1)]))
}

pub fn g1(p: &G1Projective) -> sol::G1Point {
    sol::G1Point {
        w: words(&g1_to_eip2537(p)),
    }
}

pub fn g2(p: &G2Projective) -> sol::G2Point {
    sol::G2Point {
        w: words(&g2_to_eip2537(p)),
    }
}

pub fn fr(x: &Fr) -> U256 {
    U256::from_be_bytes(fr_to_bytes32(x))
}

pub fn crs(c: &Crs<Bls>) -> sol::Crs {
    sol::Crs {
        g1: g1(&c.g1),
        beta1: g1(&c.beta1),
        beta2_1: g1(&c.beta2_1),
        eta1: g1(&c.eta1),
        etabeta1: g1(&c.etabeta1),
        kappa1: g1(&c.kappa1),
        kappabeta1: g1(&c.kappabeta1),
        g2: g2(&c.g2),
        beta_2: g2(&c.beta_2),
        beta2_2: g2(&c.beta2_2),
        eta_2: g2(&c.eta_2),
        kappa_2: g2(&c.kappa_2),
    }
}

pub fn record(r: &Record<Bls>) -> sol::Record {
    sol::Record {
        t: fr(&r.t),
        c: g1(&r.c),
        r1: g1(&r.r1),
        r2: g1(&r.r2),
        h0: g2(&r.h0),
        h1: g2(&r.h1),
        h2: g2(&r.h2),
        isUpdate: r.is_update,
    }
}

pub fn witness(w: &[EpochWitness<Bls>]) -> Vec<sol::EpochWitness> {
    w.iter()
        .map(|ew| sol::EpochWitness {
            epoch: ew.epoch,
            records: ew.records.iter().map(record).collect(),
            hints: ew
                .hints
                .iter()
                .map(|h| h.iter().map(fr).collect())
                .collect(),
        })
        .collect()
}

pub fn tx(t: &Tx<Bls>) -> sol::Tx {
    sol::Tx {
        epoch: t.epoch,
        sender: t.sender,
        ring: t.ring.clone(),
        c: g1(&t.c),
        d0: g1(&t.d0),
        n0: g1(&t.n0),
        r1: g1(&t.r1),
        r2: g1(&t.r2),
        recv: t
            .recv
            .iter()
            .map(|r| sol::Recv {
                d: g1(&r.d),
                up1: g1(&r.up.0),
                up2: g1(&r.up.1),
                dpk: g1(&r.dpk),
                e: g1(&r.e),
                side: FixedBytes::from(r.side),
                h0: g2(&r.h0),
                h1: g2(&r.h1),
                h2: g2(&r.h2),
            })
            .collect(),
        spend: witness(&t.spend.epochs),
    }
}

pub fn compaction(c: &Compaction<Bls>) -> sol::Compaction {
    sol::Compaction {
        account: c.account,
        epoch: c.epoch,
        dPrime: g1(&c.d_prime),
        witness: witness(&c.witness.epochs),
    }
}

pub fn account(a: &AccountState<Bls>) -> sol::AccountView {
    sol::AccountView {
        pkBase: g1(&a.pk_base),
        c: g1(&a.c),
        d: g1(&a.d),
        cBase: g1(&a.c_base),
        dBase: g1(&a.d_base),
        dPK: g1(&a.dpk),
        up1: g1(&a.up_agg.0),
        up2: g1(&a.up_agg.1),
        snapEpoch: a.snap_epoch,
        creditsThisEpoch: a.credits_this_epoch,
        debitedEpoch: a.debited_epoch,
        openEpochs: a.open_epochs.clone(),
        chains: a
            .open_epochs
            .iter()
            .map(|e| FixedBytes::from(a.chain[e]))
            .collect(),
    }
}
