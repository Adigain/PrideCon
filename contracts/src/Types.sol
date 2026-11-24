// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {BLS12381} from "./BLS12381.sol";

// Data types of SPEC 5-10 and 13; must match crates/btp-protocol/src/abi.rs field for field.

struct Crs {
    BLS12381.G1Point g1;
    BLS12381.G1Point beta1;
    BLS12381.G1Point beta2_1;
    BLS12381.G1Point eta1;
    BLS12381.G1Point etabeta1;
    BLS12381.G1Point kappa1;
    BLS12381.G1Point kappabeta1;
    BLS12381.G2Point g2;
    BLS12381.G2Point beta_2;
    BLS12381.G2Point beta2_2;
    BLS12381.G2Point eta_2;
    BLS12381.G2Point kappa_2;
}

struct Recv {
    BLS12381.G1Point d;
    BLS12381.G1Point up1;
    BLS12381.G1Point up2;
    BLS12381.G1Point dpk;
    BLS12381.G1Point e;
    bytes32 side;
    BLS12381.G2Point h0;
    BLS12381.G2Point h1;
    BLS12381.G2Point h2;
}

struct Record {
    uint256 t;
    BLS12381.G1Point c;
    BLS12381.G1Point r1;
    BLS12381.G1Point r2;
    BLS12381.G2Point h0;
    BLS12381.G2Point h1;
    BLS12381.G2Point h2;
    bool isUpdate;
}

struct EpochWitness {
    uint64 epoch;
    Record[] records;
    uint256[][] hints;
}

struct Tx {
    uint64 epoch;
    uint32 sender;
    uint32[] ring;
    BLS12381.G1Point c;
    BLS12381.G1Point d0;
    BLS12381.G1Point n0;
    BLS12381.G1Point r1;
    BLS12381.G1Point r2;
    Recv[] recv;
    EpochWitness[] spend;
}

struct Compaction {
    uint32 account;
    uint64 epoch;
    BLS12381.G1Point dPrime;
    EpochWitness[] witness;
}

struct AccountView {
    BLS12381.G1Point pkBase;
    BLS12381.G1Point c;
    BLS12381.G1Point d;
    BLS12381.G1Point cBase;
    BLS12381.G1Point dBase;
    BLS12381.G1Point dPK;
    BLS12381.G1Point up1;
    BLS12381.G1Point up2;
    uint64 snapEpoch;
    uint32 creditsThisEpoch;
    uint64 debitedEpoch;
    uint64[] openEpochs;
    bytes32[] chains;
}

struct BaseRecv {
    BLS12381.G1Point d;
    BLS12381.G1Point up1;
    BLS12381.G1Point up2;
    BLS12381.G1Point dpk;
    BLS12381.G1Point e;
    bytes32 side;
}

struct BaseTx {
    uint32 sender;
    uint32[] ring;
    uint64[] versions;
    BLS12381.G1Point c;
    BLS12381.G1Point d0;
    BaseRecv[] recv;
}

struct BaseAccountView {
    BLS12381.G1Point pk;
    BLS12381.G1Point c;
    BLS12381.G1Point d;
    uint64 version;
}
