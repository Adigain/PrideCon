# Check the BLS12-381 constants used in contracts/src/BLS12381.sol and the two-word negation.
x = -0xd201000000010000
p = (x - 1)**2 * (x**4 - x**2 + 1) // 3 + x
r = x**4 - x**2 + 1
P_HEX = int("1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab", 16)
R_HEX = int("73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001", 16)
print("p matches:", p == P_HEX, " r matches:", r == R_HEX)
gx = int("17f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb", 16)
gy = int("08b3f481e3aaa0f1a09e30ed741d8ae4fcf5e095d5d00af600db18cb2c04b3edd03cc744a2888ae40caa232946c5e7e1", 16)
print("G1 gen on y^2=x^3+4:", (gy * gy - gx**3 - 4) % p == 0)
def m(a, b): return ((a[0]*b[0] - a[1]*b[1]) % p, (a[0]*b[1] + a[1]*b[0]) % p)
def add(a, b): return ((a[0]+b[0]) % p, (a[1]+b[1]) % p)
X = (int("024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8", 16),
     int("13e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e", 16))
Y = (int("0ce5d527727d6e118cc9cdc6da2e351aadfd9baa8cbdd3a76d429a695160d12c923ac9cc3baca289e193548608b82801", 16),
     int("0606c4a02ea734cc32acd2b02bc28b99cb3e287e85a763af267492ab572e99ab3f370d275cec1da1aaa9075ff05f79be", 16))
lhs = m(Y, Y); rhs = add(m(m(X, X), X), (4, 4))
print("G2 gen on twist:", lhs == rhs)
P_HI, P_LO = P_HEX >> 256, P_HEX & (2**256 - 1)
print("P_HI = 0x%064x" % P_HI); print("P_LO = 0x%064x" % P_LO)
import random
ok = True
for _ in range(10000):
    y = random.randrange(1, P_HEX)
    yhi, ylo = y >> 256, y & (2**256 - 1)
    lo = (P_LO - ylo) % 2**256; borrow = 1 if ylo > P_LO else 0
    hi = (P_HI - yhi - borrow) % 2**256
    ok &= (hi << 256 | lo) == P_HEX - y
print("negation logic correct:", ok)
