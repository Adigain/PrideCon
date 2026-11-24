// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {BLS12381} from "./BLS12381.sol";
import {Encode} from "./Encode.sol";
import {Crs, Recv, Record, EpochWitness, Tx, Compaction, AccountView} from "./Types.sol";

/// @title Concurrent PriDeFUL CT with tagged cross-term handles, proof level L0.
contract PrideConcurrent {
    using BLS12381 for BLS12381.G1Point;

    uint256 internal constant R = BLS12381.R;
    uint256 internal constant MAX_RING = 32;
    uint256 internal constant CRS_BYTES = 7 * 128 + 5 * 256;

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

    struct Account {
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
    }

    event Registered(uint32 indexed id, BLS12381.G1Point pk);
    event KeyUpdated(uint32 indexed id, uint64 epoch, BLS12381.G1Point dPK, BLS12381.G1Point up1, BLS12381.G1Point up2);
    event Credit(
        uint32 indexed id,
        uint64 indexed epoch,
        BLS12381.G1Point up1,
        BLS12381.G1Point up2,
        BLS12381.G1Point dpk,
        bytes32 side
    );
    event RecordAdded(uint32 indexed id, uint64 indexed epoch, Record record);

    address internal immutable crsData;
    uint64 public immutable epochLen;
    uint32 public immutable kMax;

    mapping(uint32 => Account) internal accounts;
    mapping(uint32 => mapping(uint64 => bytes32)) public chain;
    mapping(uint256 => bool) public usedTag;
    uint32 public numAccounts;

    constructor(Crs memory crs_, uint64 epochLen_, uint32 kMax_) {
        bytes memory data = abi.encode(crs_);
        // The CRS becomes the code of a data contract (read back with one EXTCODECOPY). Init code:
        // PUSH4 len, DUP1, PUSH1 0x0e, PUSH1 0, CODECOPY, PUSH1 0, RETURN; the code starts with STOP.
        // forge-lint: disable-next-line(unsafe-typecast)
        bytes memory init = abi.encodePacked(hex"63", uint32(data.length + 1), hex"80600e6000396000f3", hex"00", data);
        address a;
        assembly ("memory-safe") {
            a := create(0, add(init, 32), mload(init))
        }
        require(a != address(0), "CRS deploy failed");
        crsData = a;
        epochLen = epochLen_;
        kMax = kMax_;
    }

    function currentEpoch() public view returns (uint64) {
        // forge-lint: disable-next-line(unsafe-typecast)
        return uint64(block.number / epochLen) + 1;
    }

    function register(BLS12381.G1Point memory pk, BLS12381.G1Point memory c, BLS12381.G1Point memory d)
        external
        returns (uint32 id)
    {
        if (pk.isZero() || c.isZero()) revert BadRegistration();
        id = ++numAccounts;
        Account storage a = accounts[id];
        a.pkBase = pk;
        a.c = c;
        a.d = d;
        a.cBase = c;
        a.dBase = d;
        a.snapEpoch = currentEpoch();
        emit Registered(id, pk);
    }

    function mint(uint32 id, uint64 amount) external {
        if (!_registered(id)) revert UnknownAccount();
        _rollover(id, currentEpoch());
        Account storage a = accounts[id];
        BLS12381.G1Point memory m = BLS12381.g1Mul(BLS12381.g1Generator(), amount);
        a.d = BLS12381.g1Add(a.d, m);
        a.dBase = BLS12381.g1Add(a.dBase, m);
    }

    function transact(Tx memory txn) external {
        uint64 e = currentEpoch();
        if (txn.epoch != e) revert WrongEpoch();
        _checkRing(txn);
        (uint256 t, bytes32 h) = Encode.tagAndTxhash(txn);
        if (usedTag[t]) revert DuplicateTag();
        _rollover(txn.sender, e);
        Account storage s = accounts[txn.sender];
        if (s.debitedEpoch == e) revert AlreadyDebited();
        for (uint256 j = 0; j < txn.ring.length; ++j) {
            _rollover(txn.ring[j], e);
            if (accounts[txn.ring[j]].creditsThisEpoch >= kMax) revert CreditCap();
        }
        Crs memory crs = _crs();
        if (!_verifyBatched(crs, txn, t, h)) revert HandleCheck();
        BLS12381.G1Point memory lhs = BLS12381.g1Sub(BLS12381.g1Add(s.dBase, txn.d0), txn.n0);
        _spendCheck(txn.sender, e, txn.spend, lhs);
        usedTag[t] = true;
        _applyDebit(txn, t, e);
        for (uint256 j = 0; j < txn.ring.length; ++j) {
            _applyCredit(txn, j, t, e);
        }
    }

    function compact(Compaction memory cp) external {
        uint64 e = currentEpoch();
        if (cp.epoch != e) revert WrongEpoch();
        if (!_registered(cp.account)) revert UnknownAccount();
        _rollover(cp.account, e);
        Account storage a = accounts[cp.account];
        _spendCheck(cp.account, e, cp.witness, BLS12381.g1Sub(a.dBase, cp.dPrime));
        a.d = BLS12381.g1Sub(BLS12381.g1Add(a.d, cp.dPrime), a.dBase);
        a.dBase = cp.dPrime;
        _dropEpochsBefore(cp.account, e);
    }

    function getAccount(uint32 id) external view returns (AccountView memory v) {
        Account storage a = accounts[id];
        v.pkBase = a.pkBase;
        v.c = a.c;
        v.d = a.d;
        v.cBase = a.cBase;
        v.dBase = a.dBase;
        v.dPK = a.dPK;
        v.up1 = a.up1;
        v.up2 = a.up2;
        v.snapEpoch = a.snapEpoch;
        v.creditsThisEpoch = a.creditsThisEpoch;
        v.debitedEpoch = a.debitedEpoch;
        v.openEpochs = a.openEpochs;
        v.chains = new bytes32[](a.openEpochs.length);
        for (uint256 i = 0; i < v.chains.length; ++i) {
            v.chains[i] = chain[id][a.openEpochs[i]];
        }
    }

    function _registered(uint32 id) internal view returns (bool) {
        return id != 0 && id <= numAccounts;
    }

    function _checkRing(Tx memory txn) internal view {
        uint256 n = txn.ring.length;
        if (n == 0 || n > MAX_RING || txn.recv.length != n) revert BadRing();
        if (!_registered(txn.sender)) revert BadRing();
        for (uint256 i = 0; i < n; ++i) {
            uint32 id = txn.ring[i];
            if (!_registered(id) || id == txn.sender) revert BadRing();
            for (uint256 k = 0; k < i; ++k) {
                if (txn.ring[k] == id) revert BadRing();
            }
        }
    }

    function _verifyBatched(Crs memory crs, Tx memory txn, uint256 t, bytes32 h) internal view returns (bool) {
        uint256 n = txn.recv.length;
        uint256 w1 = Encode.weight(h, 0);
        uint256 w2 = Encode.weight(h, 1);
        BLS12381.G1Point[] memory a = new BLS12381.G1Point[](4 + 3 * n);
        BLS12381.G2Point[] memory b = new BLS12381.G2Point[](4 + 3 * n);

        BLS12381.G1Point[] memory pts = new BLS12381.G1Point[](2 + n);
        uint256[] memory sc = new uint256[](2 + n);
        uint256[] memory w3 = new uint256[](n);
        uint256[] memory w4 = new uint256[](n);
        (pts[0], sc[0], pts[1], sc[1]) = (txn.r1, w1, txn.r2, w2);
        for (uint256 j = 0; j < n; ++j) {
            // forge-lint: disable-next-line(unsafe-typecast)
            w3[j] = Encode.weight(h, uint32(2 + 2 * j));
            // forge-lint: disable-next-line(unsafe-typecast)
            w4[j] = Encode.weight(h, uint32(3 + 2 * j));
            pts[2 + j] = txn.recv[j].dpk;
            sc[2 + j] = R - w3[j];
        }
        (a[0], b[0]) = (BLS12381.g1Msm(pts, sc), crs.g2);
        (a[1], b[1]) = (_msm2(txn.c, R - w1, txn.r1, R - mulmod(w2, t, R)), crs.beta_2);
        (a[2], b[2]) = (BLS12381.g1Mul(txn.r1, R - w2), crs.eta_2);
        (a[3], b[3]) = (BLS12381.g1Mul(txn.r1, R - mulmod(w2, txn.epoch, R)), crs.kappa_2);

        BLS12381.G1Point memory phi = _msm3(crs.beta1, t, crs.eta1, 1, crs.kappa1, txn.epoch);
        for (uint256 j = 0; j < n; ++j) {
            uint256 k = 4 + 3 * j;
            (a[k], b[k]) = (BLS12381.g1Mul(crs.g1, w3[j]), txn.recv[j].h0);
            (a[k + 1], b[k + 1]) = (BLS12381.g1Mul(crs.g1, w4[j]), txn.recv[j].h1);
            (a[k + 2], b[k + 2]) = (_msm2(crs.beta2_1, R - w3[j], phi, R - w4[j]), txn.recv[j].h2);
        }
        return BLS12381.pairingCheck(a, b);
    }

    function _spendCheck(uint32 id, uint64 e, EpochWitness[] memory w, BLS12381.G1Point memory lhs) internal view {
        uint256 bound = 1;
        for (uint256 k = 0; k < w.length; ++k) {
            bound += 3 * w[k].records.length;
        }
        BLS12381.G1Point[] memory ps = new BLS12381.G1Point[](bound);
        BLS12381.G2Point[] memory qs = new BLS12381.G2Point[](bound);
        uint256 np = 0;

        uint64[] storage open = accounts[id].openEpochs;
        for (uint256 i = 0; i < open.length; ++i) {
            uint64 ep = open[i];
            if (ep >= e) continue;
            (bool found, uint256 k) = _findEpoch(w, ep);
            if (!found) revert RecordChain();
            if (_chainOf(w[k].records) != chain[id][ep]) revert RecordChain();
            np = _addEpochPairs(w[k], ps, qs, np);
        }
        (ps[np], qs[np]) = (lhs, BLS12381.g2Generator());
        np += 1;
        assembly ("memory-safe") {
            mstore(ps, np)
            mstore(qs, np)
        }
        if (!BLS12381.pairingCheck(ps, qs)) revert SpendCheck();
    }

    function _addEpochPairs(
        EpochWitness memory ew,
        BLS12381.G1Point[] memory ps,
        BLS12381.G2Point[] memory qs,
        uint256 np
    ) internal view returns (uint256) {
        uint256 len = ew.records.length;
        if (ew.hints.length != len) revert BadHint();
        if (len < 2) return np;
        for (uint256 m = 0; m < len; ++m) {
            Record memory rm = ew.records[m];
            if (!rm.isUpdate) continue;
            uint256[] memory hm = ew.hints[m];
            if (hm.length != len - 1) revert BadHint();
            BLS12381.G1Point[] memory r1s = new BLS12381.G1Point[](len - 1);
            BLS12381.G1Point[] memory r2s = new BLS12381.G1Point[](len - 1);
            uint256[] memory neg = new uint256[](len - 1);
            BLS12381.G1Point memory sum;
            uint256 k = 0;
            for (uint256 i = 0; i < len; ++i) {
                if (i == m) continue;
                Record memory ri = ew.records[i];
                if (mulmod(hm[k], addmod(rm.t, R - ri.t, R), R) != 1) revert BadHint();
                sum = k == 0 ? ri.c : BLS12381.g1Add(sum, ri.c);
                (r1s[k], r2s[k], neg[k]) = (ri.r1, ri.r2, R - (hm[k] % R));
                k += 1;
            }
            (ps[np], qs[np]) = (sum, rm.h0);
            (ps[np + 1], qs[np + 1]) = (BLS12381.g1Msm(r1s, neg), rm.h1);
            (ps[np + 2], qs[np + 2]) = (BLS12381.g1Msm(r2s, hm), rm.h2);
            np += 3;
        }
        return np;
    }

    function _findEpoch(EpochWitness[] memory w, uint64 ep) internal pure returns (bool, uint256) {
        for (uint256 k = 0; k < w.length; ++k) {
            if (w[k].epoch == ep) return (true, k);
        }
        return (false, 0);
    }

    function _chainOf(Record[] memory records) internal pure returns (bytes32 c) {
        for (uint256 i = 0; i < records.length; ++i) {
            c = Encode.chainStep(c, records[i]);
        }
    }

    function _rollover(uint32 id, uint64 e) internal {
        Account storage a = accounts[id];
        if (a.snapEpoch >= e) return;
        emit KeyUpdated(id, a.snapEpoch, a.dPK, a.up1, a.up2);
        a.pkBase = BLS12381.g1Add(a.pkBase, a.dPK);
        delete a.dPK;
        delete a.up1;
        delete a.up2;
        a.creditsThisEpoch = 0;
        a.cBase = a.c;
        a.dBase = a.d;
        a.snapEpoch = e;
    }

    function _applyDebit(Tx memory txn, uint256 t, uint64 e) internal {
        Account storage s = accounts[txn.sender];
        s.c = BLS12381.g1Add(s.c, txn.c);
        s.d = BLS12381.g1Sub(BLS12381.g1Add(s.d, txn.n0), s.dBase);
        s.dBase = BLS12381.g1Sub(txn.n0, txn.d0);
        _dropEpochsBefore(txn.sender, e);
        s.debitedEpoch = e;
        BLS12381.G2Point memory z;
        _appendRecord(txn.sender, e, Record(t, txn.c, txn.r1, txn.r2, z, z, z, false));
    }

    function _applyCredit(Tx memory txn, uint256 j, uint256 t, uint64 e) internal {
        uint32 id = txn.ring[j];
        Recv memory r = txn.recv[j];
        Account storage a = accounts[id];
        a.c = BLS12381.g1Add(a.c, txn.c);
        a.d = BLS12381.g1Add(BLS12381.g1Add(a.d, r.d), r.e);
        a.dPK = BLS12381.g1Add(a.dPK, r.dpk);
        a.up1 = BLS12381.g1Add(a.up1, r.up1);
        a.up2 = BLS12381.g1Add(a.up2, r.up2);
        a.creditsThisEpoch += 1;
        emit Credit(id, e, r.up1, r.up2, r.dpk, r.side);
        _appendRecord(id, e, Record(t, txn.c, txn.r1, txn.r2, r.h0, r.h1, r.h2, true));
    }

    function _appendRecord(uint32 id, uint64 e, Record memory rec) internal {
        chain[id][e] = Encode.chainStep(chain[id][e], rec);
        uint64[] storage open = accounts[id].openEpochs;
        if (open.length == 0 || open[open.length - 1] != e) open.push(e);
        emit RecordAdded(id, e, rec);
    }

    function _dropEpochsBefore(uint32 id, uint64 e) internal {
        uint64[] storage open = accounts[id].openEpochs;
        uint256 keep = 0;
        for (uint256 i = 0; i < open.length; ++i) {
            uint64 ep = open[i];
            if (ep < e) {
                delete chain[id][ep];
            } else {
                open[keep] = ep;
                keep += 1;
            }
        }
        while (open.length > keep) open.pop();
    }

    function _crs() internal view returns (Crs memory) {
        address a = crsData;
        bytes memory data = new bytes(CRS_BYTES);
        assembly ("memory-safe") {
            extcodecopy(a, add(data, 32), 1, CRS_BYTES)
        }
        return abi.decode(data, (Crs));
    }

    function _msm2(BLS12381.G1Point memory p, uint256 x, BLS12381.G1Point memory q, uint256 y)
        internal
        view
        returns (BLS12381.G1Point memory)
    {
        BLS12381.G1Point[] memory pts = new BLS12381.G1Point[](2);
        uint256[] memory sc = new uint256[](2);
        (pts[0], sc[0], pts[1], sc[1]) = (p, x, q, y);
        return BLS12381.g1Msm(pts, sc);
    }

    function _msm3(
        BLS12381.G1Point memory p,
        uint256 x,
        BLS12381.G1Point memory q,
        uint256 y,
        BLS12381.G1Point memory u,
        uint256 z
    ) internal view returns (BLS12381.G1Point memory) {
        BLS12381.G1Point[] memory pts = new BLS12381.G1Point[](3);
        uint256[] memory sc = new uint256[](3);
        (pts[0], sc[0], pts[1], sc[1], pts[2], sc[2]) = (p, x, q, y, u, z);
        return BLS12381.g1Msm(pts, sc);
    }
}
