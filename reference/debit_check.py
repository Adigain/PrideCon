# Exponent-level check of the debit rule (N0, V5) mixed with concurrent credits, over all orders of an epoch.
import random, itertools
P = 2**61-1; random.seed(5); rnd = lambda: random.randrange(2, P); inv = lambda a: pow(a % P, P-2, P)
be, eta, ka = rnd(), rnd(), rnd()
phi = lambda t, e: (be*t + eta + ka*e) % P
def credit(s, xbase, e, pl):
    r, d, tau, t = rnd(), rnd(), rnd(), rnd()
    return dict(kind='c', t=t, r=r, d=d, pl=pl, C=r, D=(r*s+pl) % P, E=(xbase+r)*d % P,
                R1=be*r % P, R2=be*phi(t, e)*r % P, H0=(d+be*be*tau) % P, H1=phi(t, e)*tau % P, H2=tau)
def debit(s, base, e, pl):
    r, t = rnd(), rnd()
    D0 = (r*s - pl) % P
    N0 = ((base['C'] + r)*s + base['bal'] - pl) % P
    assert N0 == (base['D'] + D0 + base['W']) % P
    return dict(kind='d', t=t, r=r, d=0, pl=-pl, C=r, D0=D0, N0=N0, R1=be*r % P, R2=be*phi(t, e)*r % P)
def X(i, m):
    if m['kind'] == 'd': return 0
    return (i['C']*m['H0'] - (i['R1']*m['H1'] - i['R2']*m['H2'])*inv(m['t']-i['t'])) % P
def run_epoch(base, s, txs, order):
    C, D, W, dsum, seen = base['C'], base['D'], base['W'], 0, []
    for k in order:
        tx = txs[k]
        for o in seen: W = (W + X(tx, o) + X(o, tx)) % P
        C = (C + tx['C']) % P; dsum = (dsum + tx['d']) % P
        if tx['kind'] == 'c': D = (D + tx['D'] + tx['E']) % P
        else: D = (D + tx['N0'] - base['D']) % P; W = (W - base['W']) % P
        seen.append(tx)
    return C, D, W, dsum
s, x, b = rnd(), rnd(), 5000
base = dict(C=x, D=(x*s+b) % P, W=0, bal=b)
ep1 = [credit(s, x, 1, 10*(i+1)) for i in range(4)]
C, D, W, d = run_epoch(base, s, ep1, range(4)); s2 = (s+d) % P; bal = b + 100
assert (D + W - C*s2) % P == bal and W != 0
base2 = dict(C=C, D=D, W=W, bal=bal)
ep2 = [credit(s2, C, 2, 7), credit(s2, C, 2, 11), debit(s2, base2, 2, 300), credit(s2, C, 2, 13)]
res = {run_epoch(base2, s2, ep2, o) for o in itertools.permutations(range(4))}
C3, D3, W3, d3 = res.pop()
print('order-independent with a debit (24 orders):', not res)
print('valid under s+Delta with balance', bal - 300 + 31, ':', (D3 + W3 - C3*(s2+d3)) % P == bal - 300 + 31)
