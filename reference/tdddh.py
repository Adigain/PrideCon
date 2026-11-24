# Rank test of the TD-DDH game with negative controls, and an end-to-end exponent-level simulation.
import random, itertools
P = 2**61-1
random.seed(11)
rnd = lambda: random.randrange(2, P)
NAMES = 'be eta ka x th r d tau q U'.split(); N = len(NAMES)
class Pol:
    def __init__(s, d=None): s.d = {k: v % P for k, v in (d or {}).items() if v % P}
    @staticmethod
    def c(v): return Pol({(0,)*N: v})
    @staticmethod
    def var(i): return Pol({tuple(1 if j == i else 0 for j in range(N)): 1})
    def __add__(s, o):
        o = o if isinstance(o, Pol) else Pol.c(o); d = dict(s.d)
        for k, v in o.d.items(): d[k] = (d.get(k, 0) + v) % P
        return Pol(d)
    __radd__ = __add__
    def __neg__(s): return Pol({k: -v for k, v in s.d.items()})
    def __sub__(s, o): return s + (-(o if isinstance(o, Pol) else Pol.c(o)))
    def __rsub__(s, o): return (-s) + o
    def __mul__(s, o):
        o = o if isinstance(o, Pol) else Pol.c(o); d = {}
        for k1, v1 in s.d.items():
            for k2, v2 in o.d.items():
                k = tuple(a+b for a, b in zip(k1, k2)); d[k] = (d.get(k, 0) + v1*v2) % P
        return Pol(d)
    __rmul__ = __mul__
V = {n: Pol.var(i) for i, n in enumerate(NAMES)}; ONE = Pol.c(1)
def rank(polys):
    idx, piv, r = {}, {}, 0
    for q in polys:
        v = {idx.setdefault(m, len(idx)): c for m, c in q.d.items()}
        while v:
            lead = min(v)
            if lead in piv:
                f = v[lead]
                for k, c in piv[lead].items():
                    nv = (v.get(k, 0) - f*c) % P
                    if nv: v[k] = nv
                    else: v.pop(k, None)
            else:
                inv = pow(v[lead], P-2, P); piv[lead] = {k: c*inv % P for k, c in v.items()}; r += 1; break
    return r

def game(variant='hidden', same_epoch=False, raw_delta=False, split_E=False, x_handles=True, x_known=False):
    be, eta, ka, th, r, d, tau, q, U = (V[n] for n in 'be eta ka th r d tau q U'.split())
    x = Pol.c(rnd()) if x_known else V['x']
    lam = rnd(); s = lam - d
    t, e, t0 = rnd(), 5, rnd(); e0 = e if same_epoch else 4
    phi = be*t + eta + ka*e; phi0 = be*t0 + eta + ka*e0
    T = rnd(); m = rnd()
    out = {}
    for world, pay in (('real', Pol.c(m)), ('rand', U)):
        oD = x*s + (th if variant == 'hidden' else (T - pay))
        G1 = [ONE, be, be*be, eta, eta*be, ka, ka*be, s, x, oD, r, r*s + pay, q, q*s + d, d, be*r, be*phi*r]
        G1 += [x*d, r*d] if split_E else [(x + r)*d]
        if x_handles: G1 += [be*x, be*phi0*x]
        G2 = [ONE, be, be*be, eta, ka]
        G2 += [d] if raw_delta else [d + be*be*tau, phi*tau, tau]
        out[world] = (rank(G1), rank(G2), rank([a*c for a in G1 for c in G2]))
    return out
tests = [
 ('TD-DDH, hidden uniform balance (variant i)      ', dict()),
 ('TD-DDH, known total, equal sums (variant ii)    ', dict(variant='total')),
 ('TD-DDH without prior-epoch x-handles            ', dict(x_handles=False)),
 ('control: x-handles in SAME epoch (must leak)    ', dict(same_epoch=True)),
 ('control: raw g2^delta (must leak)               ', dict(raw_delta=True)),
 ('control: E split into x*d and r*d (must leak)   ', dict(split_E=True)),
 ('control: epoch-start randomness x known (leaks) ', dict(x_known=True, x_handles=False)),
]
for name, kw in tests:
    o = game(**kw); leak = any(o['real'][i] < o['rand'][i] for i in range(3))
    print(name, o['real'], o['rand'], 'LEAK' if leak else 'no leak')

inv = lambda a: pow(a % P, P-2, P)
be, eta, ka = rnd(), rnd(), rnd()
def phi(t, e): return (be*t + eta + ka*e) % P
def make_tx(pk_base_s, C0x, e, pl):
    r, d, tau, q = rnd(), rnd(), rnd(), rnd(); t = rnd()
    return dict(t=t, e=e, r=r, d=d, pl=pl, C=r, D=(r*pk_base_s + pl) % P, E=((C0x + r)*d) % P,
                up=(q, (q*pk_base_s + d) % P), dpk=d,
                R1=be*r % P, R2=be*phi(t, e)*r % P,
                H0=(d + be*be*tau) % P, H1=phi(t, e)*tau % P, H2=tau)
def checks(tx):
    c1 = tx['R1'] == tx['C']*be % P
    c2 = tx['R2'] == tx['R1']*phi(tx['t'], tx['e']) % P
    c3 = tx['H0'] == (tx['dpk'] + be*be*tx['H2']) % P
    c4 = tx['H1'] == phi(tx['t'], tx['e'])*tx['H2'] % P
    return c1 and c2 and c3 and c4
def cross(i, m):
    return (i['C']*m['H0'] - (i['R1']*m['H1'] - i['R2']*m['H2'])*inv(m['t'] - i['t'])) % P
s, x, b = rnd(), rnd(), rnd()
txs = [make_tx(s, x, 7, rnd() % 1000) for _ in range(5)]
assert all(checks(tx) for tx in txs)
finals = set()
for perm in itertools.permutations(range(5)):
    C, D, W, dsum, seen = x, (x*s + b) % P, 0, 0, []
    for k in perm:
        tx = txs[k]
        C = (C + tx['C']) % P; D = (D + tx['D'] + tx['E']) % P; dsum = (dsum + tx['dpk']) % P
        for o in seen: W = (W + cross(tx, o) + cross(o, tx)) % P
        seen.append(tx)
    finals.add((C, D, W, dsum))
C, D, W, dsum = finals.pop()
print('order-independent over all 120 orders:', len(finals) == 0)
print('decrypts under s+Delta to b+sum(pl):', (D + W - C*(s + dsum)) % P == (b + sum(t['pl'] for t in txs)) % P)
print('cross-term exact:', all(cross(i, m) == i['r']*m['d'] % P for i in txs for m in txs if i is not m))
