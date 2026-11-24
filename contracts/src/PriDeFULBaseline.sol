// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {BLS12381} from "./BLS12381.sol";
import {BaseRecv, BaseTx, BaseAccountView} from "./Types.sol";

/// @title PriDeFUL CT option (2): one update per account per state version (SPEC 13).
contract PriDeFULBaseline {
    using BLS12381 for BLS12381.G1Point;

    uint256 internal constant MAX_RING = 32;

    error BadRing();
    error StaleVersion();
    error UnknownAccount();
    error BadRegistration();

    struct Account {
        BLS12381.G1Point pk;
        BLS12381.G1Point c;
        BLS12381.G1Point d;
        uint64 version;
    }

    event Registered(uint32 indexed id, BLS12381.G1Point pk);
    event Credit(uint32 indexed id, BLS12381.G1Point up1, BLS12381.G1Point up2, BLS12381.G1Point dpk, bytes32 side);

    mapping(uint32 => Account) internal accounts;
    uint32 public numAccounts;

    function register(BLS12381.G1Point memory pk, BLS12381.G1Point memory c, BLS12381.G1Point memory d)
        external
        returns (uint32 id)
    {
        if (pk.isZero() || c.isZero()) revert BadRegistration();
        id = ++numAccounts;
        Account storage a = accounts[id];
        (a.pk, a.c, a.d) = (pk, c, d);
        emit Registered(id, pk);
    }

    function mint(uint32 id, uint64 amount) external {
        if (id == 0 || id > numAccounts) revert UnknownAccount();
        Account storage a = accounts[id];
        a.d = BLS12381.g1Add(a.d, BLS12381.g1Mul(BLS12381.g1Generator(), amount));
        a.version += 1;
    }

    function transact(BaseTx memory txn) external {
        uint256 n = txn.ring.length;
        if (n == 0 || n > MAX_RING || txn.recv.length != n || txn.versions.length != n + 1) revert BadRing();
        if (txn.sender == 0 || txn.sender > numAccounts) revert BadRing();
        for (uint256 i = 0; i < n; ++i) {
            uint32 id = txn.ring[i];
            if (id == 0 || id > numAccounts || id == txn.sender) revert BadRing();
            for (uint256 k = 0; k < i; ++k) {
                if (txn.ring[k] == id) revert BadRing();
            }
        }
        if (accounts[txn.sender].version != txn.versions[0]) revert StaleVersion();
        for (uint256 i = 0; i < n; ++i) {
            if (accounts[txn.ring[i]].version != txn.versions[i + 1]) revert StaleVersion();
        }

        Account storage s = accounts[txn.sender];
        s.c = BLS12381.g1Add(s.c, txn.c);
        s.d = BLS12381.g1Add(s.d, txn.d0);
        s.version += 1;
        for (uint256 i = 0; i < n; ++i) {
            BaseRecv memory r = txn.recv[i];
            Account storage a = accounts[txn.ring[i]];
            a.c = BLS12381.g1Add(a.c, txn.c);
            a.d = BLS12381.g1Add(BLS12381.g1Add(a.d, r.d), r.e);
            a.pk = BLS12381.g1Add(a.pk, r.dpk);
            a.version += 1;
            emit Credit(txn.ring[i], r.up1, r.up2, r.dpk, r.side);
        }
    }

    function getAccount(uint32 id) external view returns (BaseAccountView memory v) {
        Account storage a = accounts[id];
        (v.pk, v.c, v.d, v.version) = (a.pk, a.c, a.d, a.version);
    }
}
