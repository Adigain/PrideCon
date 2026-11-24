// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {BLS12381} from "../src/BLS12381.sol";

/// Precompile smoke tests.
contract BLS12381Test is Test {
    function test_PairingWithNegationIsOne() public view {
        BLS12381.G1Point[] memory a = new BLS12381.G1Point[](2);
        BLS12381.G2Point[] memory b = new BLS12381.G2Point[](2);
        a[0] = BLS12381.g1Generator();
        a[1] = BLS12381.g1Neg(BLS12381.g1Generator());
        b[0] = BLS12381.g2Generator();
        b[1] = BLS12381.g2Generator();
        assertTrue(BLS12381.pairingCheck(a, b), "e(g1,g2) * e(-g1,g2) should be 1");
    }

    function test_SinglePairingIsNotOne() public view {
        BLS12381.G1Point[] memory a = new BLS12381.G1Point[](1);
        BLS12381.G2Point[] memory b = new BLS12381.G2Point[](1);
        a[0] = BLS12381.g1Generator();
        b[0] = BLS12381.g2Generator();
        assertFalse(BLS12381.pairingCheck(a, b), "e(g1,g2) should not be 1");
    }

    function test_AddAndMulAgree() public view {
        BLS12381.G1Point memory g = BLS12381.g1Generator();
        BLS12381.G1Point memory twoG = BLS12381.g1Add(g, g);
        BLS12381.G1Point memory twoGmul = BLS12381.g1Mul(g, 2);
        for (uint256 i = 0; i < 4; ++i) {
            assertEq(twoG.w[i], twoGmul.w[i]);
        }
    }

    function test_BilinearitySmall() public view {
        BLS12381.G1Point memory g = BLS12381.g1Generator();
        BLS12381.G1Point[] memory a = new BLS12381.G1Point[](3);
        BLS12381.G2Point[] memory b = new BLS12381.G2Point[](3);
        a[0] = BLS12381.g1Mul(g, 3);
        a[1] = BLS12381.g1Neg(BLS12381.g1Mul(g, 2));
        a[2] = BLS12381.g1Neg(g);
        for (uint256 i = 0; i < 3; ++i) {
            b[i] = BLS12381.g2Generator();
        }
        assertTrue(BLS12381.pairingCheck(a, b));
    }
}
