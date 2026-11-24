// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {Test, Vm} from "forge-std/Test.sol";
import {BLS12381} from "../src/BLS12381.sol";
import {PrideConcurrent} from "../src/PrideConcurrent.sol";
import {Crs, Tx, Compaction} from "../src/Types.sol";

/// Replays the Rust fixtures; every account's state must equal Rust's after every step.
contract PrideConcurrentTest is Test {
    function _k(uint256 i, string memory field) internal pure returns (string memory) {
        return string.concat(".steps[", vm.toString(i), "].", field);
    }

    function _eq(string memory a, string memory b) internal pure returns (bool) {
        return keccak256(bytes(a)) == keccak256(bytes(b));
    }

    function _deploy(string memory json) internal returns (PrideConcurrent c, uint64 epochLen) {
        Crs memory crs = abi.decode(vm.parseJsonBytes(json, ".crs"), (Crs));
        epochLen = uint64(vm.parseJsonUint(json, ".epochLen"));
        c = new PrideConcurrent(crs, epochLen, uint32(vm.parseJsonUint(json, ".kMax")));
    }

    function _call(PrideConcurrent c, string memory kind, bytes memory data) internal {
        if (_eq(kind, "register")) {
            (BLS12381.G1Point memory pk, BLS12381.G1Point memory cc, BLS12381.G1Point memory d) =
                abi.decode(data, (BLS12381.G1Point, BLS12381.G1Point, BLS12381.G1Point));
            c.register(pk, cc, d);
        } else if (_eq(kind, "mint")) {
            (uint32 id, uint64 amount) = abi.decode(data, (uint32, uint64));
            c.mint(id, amount);
        } else if (_eq(kind, "transact")) {
            Tx memory txn = abi.decode(data, (Tx));
            c.transact(txn);
        } else if (_eq(kind, "compact")) {
            Compaction memory cp = abi.decode(data, (Compaction));
            c.compact(cp);
        } else {
            revert(string.concat("unknown step kind ", kind));
        }
    }

    function _replay(string memory name) internal {
        string memory json = vm.readFile(string.concat("../fixtures/protocol_", name, ".json"));
        (PrideConcurrent c, uint64 epochLen) = _deploy(json);
        uint256 n = vm.parseJsonUint(json, ".count");
        for (uint256 i = 0; i < n; ++i) {
            string memory kind = vm.parseJsonString(json, _k(i, "kind"));
            string memory expect = vm.parseJsonString(json, _k(i, "expect"));
            uint64 epoch = uint64(vm.parseJsonUint(json, _k(i, "epoch")));
            string memory where = string.concat(name, " step ", vm.toString(i), " (", kind, ")");

            if (_eq(kind, "nextEpoch")) {
                vm.roll((epoch - 1) * epochLen);
            } else {
                bytes memory data = vm.parseJsonBytes(json, _k(i, "data"));
                if (!_eq(expect, "ok")) {
                    vm.expectRevert(bytes4(keccak256(bytes(string.concat(expect, "()")))));
                }
                _call(c, kind, data);
            }
            assertEq(c.currentEpoch(), epoch, where);
            if (_eq(kind, "transact") && _eq(expect, "ok")) {
                assertTrue(c.usedTag(uint256(vm.parseJsonBytes32(json, _k(i, "tag")))), where);
            }
            bytes[] memory state = vm.parseJsonBytesArray(json, _k(i, "state"));
            assertEq(c.numAccounts(), state.length, where);
            for (uint256 a = 0; a < state.length; ++a) {
                assertEq(
                    abi.encode(c.getAccount(uint32(a + 1))),
                    state[a],
                    string.concat(where, ": state of account ", vm.toString(a + 1))
                );
            }
        }
    }

    function test_Credits() public {
        _replay("credits");
    }

    function test_Spend() public {
        _replay("spend");
    }

    function test_Compaction() public {
        _replay("compaction");
    }

    function test_Rejects() public {
        _replay("rejects");
    }

    function test_Workload() public {
        _replay("workload");
    }

    function test_CreditEventsCarryUpdateMaterial() public {
        string memory json = vm.readFile("../fixtures/protocol_credits.json");
        (PrideConcurrent c,) = _deploy(json);
        uint256 n = vm.parseJsonUint(json, ".count");
        for (uint256 i = 0; i < n; ++i) {
            string memory kind = vm.parseJsonString(json, _k(i, "kind"));
            bytes memory data = vm.parseJsonBytes(json, _k(i, "data"));
            if (!_eq(kind, "transact")) {
                _call(c, kind, data);
                continue;
            }
            Tx memory txn = abi.decode(data, (Tx));
            vm.recordLogs();
            c.transact(txn);
            Vm.Log[] memory logs = vm.getRecordedLogs();
            bytes32 sig = keccak256("Credit(uint32,uint64,(bytes32[4]),(bytes32[4]),(bytes32[4]),bytes32)");
            uint256 seen = 0;
            for (uint256 l = 0; l < logs.length; ++l) {
                if (logs[l].topics[0] != sig) continue;
                assertEq(uint256(logs[l].topics[1]), txn.ring[seen], "credited account");
                (BLS12381.G1Point memory up1, BLS12381.G1Point memory up2, BLS12381.G1Point memory dpk, bytes32 side) =
                    abi.decode(logs[l].data, (BLS12381.G1Point, BLS12381.G1Point, BLS12381.G1Point, bytes32));
                assertEq(abi.encode(up1), abi.encode(txn.recv[seen].up1));
                assertEq(abi.encode(up2), abi.encode(txn.recv[seen].up2));
                assertEq(abi.encode(dpk), abi.encode(txn.recv[seen].dpk));
                assertEq(side, txn.recv[seen].side);
                seen += 1;
            }
            assertEq(seen, txn.ring.length, "one Credit event per ring member");
            return;
        }
        revert("no transaction in fixture");
    }
}
