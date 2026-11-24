// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {BLS12381} from "../src/BLS12381.sol";
import {PriDeFULBaseline} from "../src/PriDeFULBaseline.sol";
import {BaseRecv, BaseTx, BaseAccountView} from "../src/Types.sol";

/// The baseline's version rule (SPEC 13).
contract PriDeFULBaselineTest is Test {
    PriDeFULBaseline internal b;

    function _p(uint256 k) internal view returns (BLS12381.G1Point memory) {
        return BLS12381.g1Mul(BLS12381.g1Generator(), k);
    }

    function setUp() public {
        b = new PriDeFULBaseline();
        for (uint256 i = 1; i <= 5; ++i) {
            b.register(_p(10 + i), _p(20 + i), _p(30 + i));
        }
    }

    function _tx(uint32 sender, uint32[] memory ring, uint256 salt) internal view returns (BaseTx memory t) {
        t.sender = sender;
        t.ring = ring;
        t.versions = new uint64[](ring.length + 1);
        t.versions[0] = b.getAccount(sender).version;
        t.c = _p(100 + salt);
        t.d0 = _p(200 + salt);
        t.recv = new BaseRecv[](ring.length);
        for (uint256 j = 0; j < ring.length; ++j) {
            t.versions[j + 1] = b.getAccount(ring[j]).version;
            t.recv[j] = BaseRecv(_p(300 + salt + j), _p(1), _p(2), _p(400 + j), _p(500 + j), bytes32(salt));
        }
    }

    function _ring(uint32 a, uint32 c) internal pure returns (uint32[] memory r) {
        r = new uint32[](2);
        (r[0], r[1]) = (a, c);
    }

    function test_HonestTransactionUpdatesState() public {
        BaseTx memory t = _tx(1, _ring(2, 3), 7);
        BaseAccountView memory before = b.getAccount(2);
        b.transact(t);
        BaseAccountView memory a = b.getAccount(2);
        assertEq(a.version, before.version + 1);
        assertEq(abi.encode(a.c), abi.encode(BLS12381.g1Add(before.c, t.c)));
        assertEq(abi.encode(a.d), abi.encode(BLS12381.g1Add(BLS12381.g1Add(before.d, t.recv[0].d), t.recv[0].e)));
        assertEq(abi.encode(a.pk), abi.encode(BLS12381.g1Add(before.pk, t.recv[0].dpk)), "key moves now");
        assertEq(b.getAccount(1).version, 1, "the sender's version moves too");
    }

    function test_SecondTransactionOnSameStateIsRefused() public {
        BaseTx memory first = _tx(1, _ring(2, 3), 1);
        BaseTx memory overlap = _tx(4, _ring(3, 5), 2);
        BaseTx memory disjoint = _tx(5, _ring(4, 2), 3);
        b.transact(first);
        vm.expectRevert(PriDeFULBaseline.StaleVersion.selector);
        b.transact(overlap);
        vm.expectRevert(PriDeFULBaseline.StaleVersion.selector);
        b.transact(disjoint);
        b.transact(_tx(4, _ring(3, 5), 2));
    }

    function test_SenderConflictAlsoCounts() public {
        BaseTx memory credit1 = _tx(2, _ring(1, 3), 1);
        BaseTx memory spend1 = _tx(1, _ring(4, 5), 2);
        b.transact(credit1);
        vm.expectRevert(PriDeFULBaseline.StaleVersion.selector);
        b.transact(spend1);
    }

    function test_MintBumpsVersion() public {
        BaseTx memory t = _tx(1, _ring(2, 3), 1);
        b.mint(2, 5);
        vm.expectRevert(PriDeFULBaseline.StaleVersion.selector);
        b.transact(t);
    }

    function test_BadRing() public {
        BaseTx memory t = _tx(1, _ring(2, 2), 1);
        vm.expectRevert(PriDeFULBaseline.BadRing.selector);
        b.transact(t);
        t = _tx(1, _ring(1, 2), 1);
        vm.expectRevert(PriDeFULBaseline.BadRing.selector);
        b.transact(t);
        t = _tx(1, _ring(2, 9), 1);
        t.versions[2] = 0;
        vm.expectRevert(PriDeFULBaseline.BadRing.selector);
        b.transact(t);
    }
}
