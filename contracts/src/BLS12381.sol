// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

/// @title BLS12-381 helpers over the EIP-2537 precompiles.
library BLS12381 {
    struct G1Point {
        bytes32[4] w;
    }

    struct G2Point {
        bytes32[8] w;
    }

    address internal constant G1ADD = address(0x0b);
    address internal constant G1MSM = address(0x0c);
    address internal constant G2ADD = address(0x0d);
    address internal constant G2MSM = address(0x0e);
    address internal constant PAIRING_CHECK = address(0x0f);

    uint256 internal constant R = 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001;
    uint256 internal constant P_HI = 0x000000000000000000000000000000001a0111ea397fe69a4b1ba7b6434bacd7;
    uint256 internal constant P_LO = 0x64774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab;

    function g1Generator() internal pure returns (G1Point memory p) {
        p.w[0] = bytes32(uint256(0x17f1d3a73197d7942695638c4fa9ac0f));
        p.w[1] = bytes32(uint256(0xc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb));
        p.w[2] = bytes32(uint256(0x08b3f481e3aaa0f1a09e30ed741d8ae4));
        p.w[3] = bytes32(uint256(0xfcf5e095d5d00af600db18cb2c04b3edd03cc744a2888ae40caa232946c5e7e1));
    }

    function g2Generator() internal pure returns (G2Point memory q) {
        q.w[0] = bytes32(uint256(0x024aa2b2f08f0a91260805272dc51051));
        q.w[1] = bytes32(uint256(0xc6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8));
        q.w[2] = bytes32(uint256(0x13e02b6052719f607dacd3a088274f65));
        q.w[3] = bytes32(uint256(0x596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e));
        q.w[4] = bytes32(uint256(0x0ce5d527727d6e118cc9cdc6da2e351a));
        q.w[5] = bytes32(uint256(0xadfd9baa8cbdd3a76d429a695160d12c923ac9cc3baca289e193548608b82801));
        q.w[6] = bytes32(uint256(0x0606c4a02ea734cc32acd2b02bc28b99));
        q.w[7] = bytes32(uint256(0xcb3e287e85a763af267492ab572e99ab3f370d275cec1da1aaa9075ff05f79be));
    }

    function isZero(G1Point memory a) internal pure returns (bool) {
        return a.w[0] == 0 && a.w[1] == 0 && a.w[2] == 0 && a.w[3] == 0;
    }

    function g1Add(G1Point memory a, G1Point memory b) internal view returns (G1Point memory c) {
        (bool ok, bytes memory out) = G1ADD.staticcall(abi.encodePacked(a.w, b.w));
        require(ok && out.length == 128, "BLS: G1ADD failed");
        c.w = abi.decode(out, (bytes32[4]));
    }

    function g1Neg(G1Point memory a) internal pure returns (G1Point memory c) {
        c.w[0] = a.w[0];
        c.w[1] = a.w[1];
        uint256 yHi = uint256(a.w[2]);
        uint256 yLo = uint256(a.w[3]);
        if (yHi == 0 && yLo == 0) {
            return c;
        }
        unchecked {
            uint256 borrow = yLo > P_LO ? 1 : 0;
            c.w[2] = bytes32(P_HI - yHi - borrow);
            c.w[3] = bytes32(P_LO - yLo);
        }
    }

    function g1Sub(G1Point memory a, G1Point memory b) internal view returns (G1Point memory) {
        return g1Add(a, g1Neg(b));
    }

    function g1Msm(G1Point[] memory pts, uint256[] memory scalars) internal view returns (G1Point memory c) {
        uint256 k = pts.length;
        require(k == scalars.length && k > 0, "BLS: MSM length");
        bytes memory input = new bytes(160 * k);
        for (uint256 i = 0; i < k; ++i) {
            uint256 off = 160 * i;
            for (uint256 j = 0; j < 4; ++j) {
                bytes32 v = pts[i].w[j];
                assembly ("memory-safe") {
                    mstore(add(add(input, 32), add(off, mul(j, 32))), v)
                }
            }
            uint256 s = scalars[i];
            assembly ("memory-safe") {
                mstore(add(add(input, 32), add(off, 128)), s)
            }
        }
        (bool ok, bytes memory out) = G1MSM.staticcall(input);
        require(ok && out.length == 128, "BLS: G1MSM failed");
        c.w = abi.decode(out, (bytes32[4]));
    }

    function g1Mul(G1Point memory p, uint256 s) internal view returns (G1Point memory) {
        G1Point[] memory pts = new G1Point[](1);
        uint256[] memory sc = new uint256[](1);
        pts[0] = p;
        sc[0] = s;
        return g1Msm(pts, sc);
    }

    function g2Mul(G2Point memory q, uint256 s) internal view returns (G2Point memory c) {
        (bool ok, bytes memory out) = G2MSM.staticcall(abi.encodePacked(q.w, s));
        require(ok && out.length == 256, "BLS: G2MSM failed");
        c.w = abi.decode(out, (bytes32[8]));
    }

    function pairingCheck(G1Point[] memory a, G2Point[] memory b) internal view returns (bool) {
        uint256 k = a.length;
        require(k == b.length && k > 0, "BLS: pairing length");
        bytes memory input = new bytes(384 * k);
        for (uint256 i = 0; i < k; ++i) {
            uint256 off = 384 * i;
            for (uint256 j = 0; j < 4; ++j) {
                bytes32 v = a[i].w[j];
                assembly ("memory-safe") {
                    mstore(add(add(input, 32), add(off, mul(j, 32))), v)
                }
            }
            for (uint256 j = 0; j < 8; ++j) {
                bytes32 v = b[i].w[j];
                assembly ("memory-safe") {
                    mstore(add(add(input, 32), add(add(off, 128), mul(j, 32))), v)
                }
            }
        }
        (bool ok, bytes memory out) = PAIRING_CHECK.staticcall(input);
        require(ok && out.length == 32, "BLS: PAIRING_CHECK failed");
        // forge-lint: disable-next-line(unsafe-typecast)
        return uint256(bytes32(out)) == 1;
    }
}
