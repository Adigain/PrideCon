// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {BLS12381} from "./BLS12381.sol";
import {Tx, Recv, Record} from "./Types.sol";

/// @title Encodings and hashes of SPEC 4, 7.3 and 8 (must match btp-protocol).
library Encode {
    bytes10 internal constant TAG_TX = "BTP/tag/v1";
    bytes8 internal constant TAG_W = "BTP/w/v1";
    bytes10 internal constant TAG_REC = "BTP/rec/v1";

    uint256 internal constant CORE_PER_MEMBER = 5 * 128 + 32;
    uint256 internal constant HANDLES_PER_MEMBER = 3 * 256;

    function tagAndTxhash(Tx memory txn) internal pure returns (uint256 t, bytes32 h) {
        uint256 n = txn.recv.length;
        uint256 coreLen = 8 + 4 + 4 + 4 * txn.ring.length + 2 * 128 + n * CORE_PER_MEMBER;
        uint256 total = 10 + coreLen + 2 * 128 + n * HANDLES_PER_MEMBER + 128;
        bytes memory buf = new bytes(total + 32);
        uint256 p;
        assembly ("memory-safe") {
            p := add(buf, 32)
        }
        p = _put(p, bytes32(TAG_TX), 10);
        p = _put(p, bytes32(uint256(txn.epoch) << 192), 8);
        p = _put(p, bytes32(uint256(txn.sender) << 224), 4);
        p = _put(p, bytes32(txn.ring.length << 224), 4);
        for (uint256 i = 0; i < txn.ring.length; ++i) {
            p = _put(p, bytes32(uint256(txn.ring[i]) << 224), 4);
        }
        p = _g1(p, txn.c);
        p = _g1(p, txn.d0);
        for (uint256 j = 0; j < n; ++j) {
            Recv memory r = txn.recv[j];
            p = _g1(p, r.d);
            p = _g1(p, r.up1);
            p = _g1(p, r.up2);
            p = _g1(p, r.dpk);
            p = _g1(p, r.e);
            p = _put(p, r.side, 32);
        }
        p = _g1(p, txn.r1);
        p = _g1(p, txn.r2);
        for (uint256 j = 0; j < n; ++j) {
            p = _g2(p, txn.recv[j].h0);
            p = _g2(p, txn.recv[j].h1);
            p = _g2(p, txn.recv[j].h2);
        }
        _g1(p, txn.n0);
        bytes32 tagHash;
        assembly ("memory-safe") {
            tagHash := keccak256(add(buf, 32), add(10, coreLen))
            h := keccak256(add(buf, 42), sub(total, 10))
        }
        t = uint256(tagHash) % BLS12381.R;
    }

    function weight(bytes32 h, uint32 i) internal pure returns (uint256) {
        return uint256(keccak256(abi.encodePacked(TAG_W, h, i))) % BLS12381.R;
    }

    function recordHash(Record memory r) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(TAG_REC, r.t, r.c.w, r.r1.w, r.r2.w, r.h0.w, r.h1.w, r.h2.w, r.isUpdate));
    }

    function chainStep(bytes32 prev, Record memory r) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(prev, recordHash(r)));
    }

    function _put(uint256 p, bytes32 word, uint256 len) private pure returns (uint256) {
        assembly ("memory-safe") {
            mstore(p, word)
        }
        return p + len;
    }

    function _g1(uint256 p, BLS12381.G1Point memory pt) private pure returns (uint256) {
        for (uint256 k = 0; k < 4; ++k) {
            p = _put(p, pt.w[k], 32);
        }
        return p;
    }

    function _g2(uint256 p, BLS12381.G2Point memory pt) private pure returns (uint256) {
        for (uint256 k = 0; k < 8; ++k) {
            p = _put(p, pt.w[k], 32);
        }
        return p;
    }
}
