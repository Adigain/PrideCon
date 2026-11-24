# Generic-bilinear-group leak test: a design leaks iff the real world has lower rank than the random world.
import random
P = 2**61-1
random.seed(7)
rnd = lambda: random.randrange(2, P)
NAMES = 'x0 b r0 r1 d1 tau1 sg1 pl1 q1 rs ds taus sgs qs be eta uu vv U'.split()
N = len(NAMES)

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

V = {n: Pol.var(i) for i, n in enumerate(NAMES)}
ONE = Pol.c(1)

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
                inv = pow(v[lead], P-2, P)
                piv[lead] = {k: c*inv % P for k, c in v.items()}; r += 1; break
    return r

def run(scheme, b_known=False, x0_zero=False, epoch_layer=True, honest_other=True):
    g = V
    Sp = rnd()
    s = Sp - g['ds'] - (g['d1'] if honest_other else 0)
    p = rnd(); t0, t1, ts = rnd(), rnd(), rnd(); e0, e = 3, 4
    bb = Pol.c(rnd()) if b_known else g['b']
    x = (Pol() if x0_zero else g['x0']) + g['r0']
    be, eta, uu, vv = g['be'], g['eta'], g['uu'], g['vv']
    out = {}
    for world, pay in (('real', Pol.c(p)), ('rand', g['U'])):
        G1 = [ONE, s, x, x*s + bb]
        G2 = [ONE]
        txs = [(g['rs'], g['ds'], g['taus'], g['sgs'], g['qs'], pay, ts)]
        if honest_other: txs.append((g['r1'], g['d1'], g['tau1'], g['sg1'], g['q1'], g['pl1'], t1))
        if scheme == 'original_single':
            r, d, tau, sg, q, pl, t = txs[0]
            G1 += [r, r*s + pl, q, q*s + d, d, (x + r)*d]
        elif scheme == 'methodA':
            for r, d, tau, sg, q, pl, t in txs:
                G1 += [r, r*s + pl, q, q*s + d, d, x*d]; G2 += [d]
        elif scheme == 'tagged':
            G1 += [be, be*be, eta, eta*be]; G2 += [be, be*be, eta, eta*be]
            if epoch_layer: G1 += [uu, vv]; G2 += [uu, vv]
            r0 = g['r0']
            G1 += [r0, be*r0, (be*be*t0 + eta*be)*r0]
            if epoch_layer: G1 += [(uu*e0 + vv)*r0]
            for r, d, tau, sg, q, pl, t in txs:
                G1 += [r, r*s + pl, q, q*s + d, d, (x + r)*d, be*r, (be*be*t + eta*be)*r]
                h0 = d + be*be*tau
                if epoch_layer:
                    G1 += [(uu*e + vv)*r]; h0 = h0 + (uu*e + vv)*sg; G2 += [sg]
                G2 += [h0, (be*t + eta)*tau, tau]
        out[world] = rank([a*c for a in G1 for c in G2])
    return out

cases = [
 ('original PriDeFUL (single tx)          ', dict(scheme='original_single', honest_other=False)),
 ('original, prior balance b known        ', dict(scheme='original_single', honest_other=False, b_known=True)),
 ('plan Method A (raw g2^delta)           ', dict(scheme='methodA')),
 ('tagged handles, no epoch binding, x0=0 ', dict(scheme='tagged', epoch_layer=False, x0_zero=True)),
 ('tagged handles, no epoch binding       ', dict(scheme='tagged', epoch_layer=False)),
 ('tagged handles + epoch binding         ', dict(scheme='tagged')),
 ('tagged + epoch binding, x0=0           ', dict(scheme='tagged', x0_zero=True)),
 ('tagged + epoch binding, tx* alone      ', dict(scheme='tagged', honest_other=False)),
 ('tagged + epoch binding, b known        ', dict(scheme='tagged', b_known=True)),
]
for name, kw in cases:
    o = run(**kw); print(name, o, 'LEAK' if o['real'] < o['rand'] else 'no leak')

g = V; be, eta, uu, vv = g['be'], g['eta'], g['uu'], g['vv']; ti, tm, e = rnd(), rnd(), 4
r, d, tau, sg = g['r1'], g['ds'], g['taus'], g['sgs']
C, R1, R2, R3 = r, be*r, (be*be*ti + eta*be)*r, (uu*e + vv)*r
H0, H1, H2, H3 = d + be*be*tau + (uu*e + vv)*sg, (be*tm + eta)*tau, -tau, sg
inv = pow((tm - ti) % P, P-2, P)
W = C*H0 - (R1*H1 + R2*H2)*inv - R3*H3
print('cross-term formula correct:', (W - r*d).d == {})
