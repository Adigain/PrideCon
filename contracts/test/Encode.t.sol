// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {Encode} from "../src/Encode.sol";
import {Tx, Record} from "../src/Types.sol";

/// Solidity encodings match the Rust values in fixtures/encoding.json.
contract EncodeTest is Test {
    string internal json;

    function setUp() public {
        json = vm.readFile("../fixtures/encoding.json");
    }

    function _k(string memory arr, uint256 i, string memory field) internal pure returns (string memory) {
        return string.concat(".", arr, "[", vm.toString(i), "].", field);
    }

    function test_TagTxhashAndWeights() public view {
        uint256 n = vm.parseJsonUint(json, ".txCount");
        assertGt(n, 0);
        for (uint256 i = 0; i < n; ++i) {
            Tx memory txn = abi.decode(vm.parseJsonBytes(json, _k("txs", i, "data")), (Tx));
            (uint256 t, bytes32 h) = Encode.tagAndTxhash(txn);
            assertEq(bytes32(t), vm.parseJsonBytes32(json, _k("txs", i, "t")), "tag");
            assertEq(h, vm.parseJsonBytes32(json, _k("txs", i, "txhash")), "txhash");
            bytes32[] memory w = vm.parseJsonBytes32Array(json, _k("txs", i, "weights"));
            assertEq(w.length, 2 * txn.recv.length + 2);
            for (uint256 k = 0; k < w.length; ++k) {
                assertEq(bytes32(Encode.weight(h, uint32(k))), w[k], "weight");
            }
        }
    }

    function test_RecordHashesAndChains() public view {
        uint256 n = vm.parseJsonUint(json, ".recordCount");
        assertGt(n, 0);
        bytes32 chainAll;
        for (uint256 i = 0; i < n; ++i) {
            Record memory r = abi.decode(vm.parseJsonBytes(json, _k("records", i, "data")), (Record));
            assertEq(Encode.recordHash(r), vm.parseJsonBytes32(json, _k("records", i, "hash")), "record hash");
            assertEq(Encode.chainStep(bytes32(0), r), vm.parseJsonBytes32(json, _k("records", i, "chainFromZero")));
            chainAll = Encode.chainStep(chainAll, r);
        }
        assertEq(chainAll, vm.parseJsonBytes32(json, ".chainOfAll"), "chain over all records");
    }
}
