// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {BLS12381} from "../src/BLS12381.sol";

/// Rust's EIP-2537 encoding agrees with the precompiles (fixtures/codec.json).
contract CodecTest is Test {
    string internal json;
    uint256 internal count;

    function setUp() public {
        json = vm.readFile("../fixtures/codec.json");
        count = vm.parseJsonUint(json, ".count");
    }

    function _key(uint256 i, string memory field) internal pure returns (string memory) {
        return string.concat(".cases[", vm.toString(i), "].", field);
    }

    function _scalar(uint256 i) internal view returns (uint256) {
        return vm.parseJsonUint(json, _key(i, "k"));
    }

    function _g1(uint256 i) internal view returns (BLS12381.G1Point memory p) {
        bytes memory b = vm.parseJsonBytes(json, _key(i, "g1"));
        assertEq(b.length, 128, "G1 fixture must be 128 bytes");
        p.w = abi.decode(b, (bytes32[4]));
    }

    function _g2(uint256 i) internal view returns (BLS12381.G2Point memory q) {
        bytes memory b = vm.parseJsonBytes(json, _key(i, "g2"));
        assertEq(b.length, 256, "G2 fixture must be 256 bytes");
        q.w = abi.decode(b, (bytes32[8]));
    }

    function test_FixtureIsNotEmpty() public view {
        assertGt(count, 0);
    }

    function test_GeneratorsMatchRust() public view {
        assertEq(_scalar(1), 1, "case 1 must be k = 1");
        assertEq(abi.encodePacked(BLS12381.g1Generator().w), abi.encodePacked(_g1(1).w));
        assertEq(abi.encodePacked(BLS12381.g2Generator().w), abi.encodePacked(_g2(1).w));
    }

    function test_G1MulMatchesRust() public view {
        for (uint256 i = 0; i < count; ++i) {
            BLS12381.G1Point memory got = BLS12381.g1Mul(BLS12381.g1Generator(), _scalar(i));
            assertEq(abi.encodePacked(got.w), abi.encodePacked(_g1(i).w), string.concat("G1 case ", vm.toString(i)));
        }
    }

    function test_G2MulMatchesRust() public view {
        for (uint256 i = 0; i < count; ++i) {
            BLS12381.G2Point memory got = BLS12381.g2Mul(BLS12381.g2Generator(), _scalar(i));
            assertEq(abi.encodePacked(got.w), abi.encodePacked(_g2(i).w), string.concat("G2 case ", vm.toString(i)));
        }
    }

    function test_PairingAcceptsRustPoints() public view {
        BLS12381.G1Point[] memory a = new BLS12381.G1Point[](2);
        BLS12381.G2Point[] memory b = new BLS12381.G2Point[](2);
        for (uint256 i = 0; i < count; ++i) {
            a[0] = _g1(i);
            b[0] = BLS12381.g2Generator();
            a[1] = BLS12381.g1Neg(BLS12381.g1Generator());
            b[1] = _g2(i);
            assertTrue(BLS12381.pairingCheck(a, b), string.concat("pairing case ", vm.toString(i)));
        }
    }

    function test_PairingRejectsMismatchedPoints() public view {
        BLS12381.G1Point[] memory a = new BLS12381.G1Point[](2);
        BLS12381.G2Point[] memory b = new BLS12381.G2Point[](2);
        a[0] = _g1(2);
        b[0] = BLS12381.g2Generator();
        a[1] = BLS12381.g1Neg(BLS12381.g1Generator());
        b[1] = _g2(3);
        assertFalse(BLS12381.pairingCheck(a, b));
    }
}
