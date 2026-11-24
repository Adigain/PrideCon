# Exponent-level checks of the on-chain design: batched verify, deferred spend check, compaction, gas model.
import random, itertools, math
P = 2**61 - 1
random.seed(2026)
rnd = lambda: random.randrange(2, P)
inv = lambda a: pow(a % P, P - 2, P)
beta, eta, kappa = rnd(), rnd(), rnd()
phi = lambda t, e: (beta * t + eta + kappa * e) % P

def pairing_product(pairs):
    return sum(a * b for a, b in pairs) % P == 0

def make_tx(n, e, honest=True):
    r, t = rnd(), rnd()
    tx = dict(t=t, e=e, C=r, R1=beta * r % P, R2=beta * phi(t, e) * r % P, rec=[])
    for _ in range(n):
        d, tau = rnd(), rnd()
        tx['rec'].append(dict(dpk=d, H0=(d + beta * beta * tau) % P, H1=phi(t, e) * tau % P, H2=tau))
    if not honest:
        j = random.randrange(n); k = random.choice(['H0', 'H1', 'H2'])
        tx['rec'][j][k] = (tx['rec'][j][k] + 1) % P
    return tx

def verify_batched(tx):
    t, e = tx['t'], tx['e']
    phi1 = (beta * t + eta + kappa * e) % P
    eqs = [
        [(tx['R1'], 1), (-tx['C'] % P, beta)],
        [(tx['R2'], 1), (-tx['R1'] * t % P, beta), (-tx['R1'] % P, eta), (-tx['R1'] * e % P, kappa)],
    ]
    for rj in tx['rec']:
        eqs.append([(1, rj['H0']), (-rj['dpk'] % P, 1), (-beta * beta % P, rj['H2'])])
        eqs.append([(1, rj['H1']), (-phi1 % P, rj['H2'])])
    merged = {}
    for eq in eqs:
        w = rnd()
        for a, b in eq: merged[b] = (merged.get(b, 0) + w * a) % P
    return pairing_product(list(merged.items())), len(merged)

ok_h = all(verify_batched(make_tx(n, 3))[0] for n in (1, 2, 5) for _ in range(50))
bad = sum(verify_batched(make_tx(n, 3, honest=False))[0] for n in (1, 2, 5) for _ in range(50))
_, npairs = verify_batched(make_tx(5, 3))
print(f"[1] batched Verify, G1-only arithmetic: honest accepted={ok_h}, malformed accepted={bad}/150, "
      f"pairings for n=5 receivers={npairs} (= 4 + 3n)")

class Account:
    def __init__(s, sk, bal):
        x = rnd(); s.sk = sk; s.bal = bal
        s.C, s.D = x, (x * sk + bal) % P
        s.Wnative = 0
        s.records = {}
        s.compacted_upto = -1
        s.snap = None; s.snap_epoch = None; s.dsum = 0
    def rollover(s, e):
        if s.snap_epoch != e:
            s.sk = (s.sk + s.dsum) % P; s.dsum = 0
            s.snap = dict(C=s.C, D=s.D, W=s.Wnative, bal=s.bal); s.snap_epoch = e

def X(i, m):
    if m.get('H0') is None: return 0
    return (i['C'] * m['H0'] - (i['R1'] * m['H1'] - i['R2'] * m['H2']) * inv(m['t'] - i['t'])) % P

def W_naive(acct, upto):
    return sum(X(i, m) for e, recs in acct.records.items() if acct.compacted_upto < e <= upto
               for i in recs for m in recs if i is not m) % P

def W_grouped(acct, upto, count):
    tot = 0
    for e, recs in acct.records.items():
        if not (acct.compacted_upto < e <= upto): continue
        for m in recs:
            if m.get('H0') is None: continue
            others = [i for i in recs if i is not m]
            if not others: continue
            hints = [inv(m['t'] - i['t']) for i in others]
            assert all(h * (m['t'] - i['t']) % P == 1 for h, i in zip(hints, others))
            Csum = sum(i['C'] for i in others) % P
            A = sum(-h * i['R1'] for h, i in zip(hints, others)) % P
            B = sum(h * i['R2'] for h, i in zip(hints, others)) % P
            tot += Csum * m['H0'] + A * m['H1'] + B * m['H2']
            count['pairings'] += 3; count['msm_terms'] += 2 * len(others); count['adds'] += len(others)
    return tot % P

def credit(acct, e, pl):
    acct.rollover(e)
    r, d, tau, t = rnd(), rnd(), rnd(), rnd()
    rec = dict(t=t, C=r, R1=beta * r % P, R2=beta * phi(t, e) * r % P,
               H0=(d + beta * beta * tau) % P, H1=phi(t, e) * tau % P, H2=tau)
    E = (acct.snap['C'] + r) * d % P
    for o in acct.records.get(e, []):
        acct.Wnative = (acct.Wnative + X(rec, o) + X(o, rec)) % P
    acct.records.setdefault(e, []).append(rec)
    acct.C = (acct.C + r) % P; acct.D = (acct.D + r * acct.sk + pl + E) % P
    acct.dsum = (acct.dsum + d) % P; acct.bal += pl

def spend_check(acct, e, count):
    return W_grouped(acct, e - 1, count)

def debit(acct, e, pl, count):
    acct.rollover(e)
    s, base = acct.sk, acct.snap
    assert base['bal'] >= pl, "overdraft"
    r, t = rnd(), rnd()
    D0 = (r * s - pl) % P
    N0 = ((base['C'] + r) * s + base['bal'] - pl) % P
    Wb = spend_check(acct, e, count)
    assert N0 == (base['D'] + D0 + Wb) % P, "V5 failed"
    assert Wb == W_naive(acct, e - 1)
    rec = dict(t=t, C=r, R1=beta * r % P, R2=beta * phi(t, e) * r % P, H0=None)
    for o in acct.records.get(e, []):
        acct.Wnative = (acct.Wnative + X(rec, o)) % P
    acct.records.setdefault(e, []).append(rec)
    acct.C = (acct.C + r) % P
    acct.D = (acct.D + N0 - base['D']) % P
    acct.Wnative = (acct.Wnative - base['W']) % P
    acct.compacted_upto = e - 1
    base['D'] = (base['D'] + Wb) % P; base['W'] = 0
    acct.bal -= pl

def compact(acct, e, count):
    acct.rollover(e)
    base, s = acct.snap, acct.sk
    Dp = (base['C'] * s + base['bal']) % P
    Wb = spend_check(acct, e, count)
    assert Dp == (base['D'] + Wb) % P, "compaction check failed"
    acct.D = (acct.D + Dp - base['D']) % P
    acct.Wnative = (acct.Wnative - base['W']) % P
    acct.compacted_upto = e - 1
    base['D'] = Dp; base['W'] = 0

def decrypt_ok(acct):
    s = (acct.sk + acct.dsum) % P
    return (acct.D + acct.Wnative - acct.C * s) % P == acct.bal % P

def run(seed, epochs=4, per_epoch=5, orders=20):
    random.seed(seed)
    finals = set()
    plan = []
    for e in range(epochs):
        ops = [('c', random.randint(1, 50)) for _ in range(per_epoch)]
        if e > 0 and random.random() < 0.6: ops[random.randrange(per_epoch)] = ('d', random.randint(1, 30))
        if e > 0 and random.random() < 0.5: ops.append(('k', 0))
        plan.append(ops)
    for o in range(orders):
        random.seed(seed)
        acct = Account(sk=123456789, bal=1000)
        cnt = dict(pairings=0, msm_terms=0, adds=0)
        for e, ops in enumerate(plan):
            idx = list(range(len(ops)))
            random.seed(seed * 7919 + o * 31 + e); random.shuffle(idx)
            for j in idx:
                kind, amt = ops[j]
                random.seed(hash((seed, e, j)))
                if kind == 'c': credit(acct, e, amt)
                elif kind == 'd': debit(acct, e, amt, cnt)
                else: compact(acct, e, cnt)
            assert decrypt_ok(acct)
        finals.add((acct.C, acct.D, acct.Wnative, acct.bal))
    return len(finals) == 1

res = [run(s) for s in range(30)]
print(f"[2-4] 30 random 4-epoch workloads x 20 orderings: grouped spend check == naive, V5 holds, "
      f"compaction holds, state order-independent and decryptable: {all(res)}")

BN = dict(add=150, mul=6000, pair_base=45000, pair=34000, g1_bytes=64, g2_bytes=128)
BLS = dict(add=375, pair_base=37700, pair=32600, g1_bytes=128, g2_bytes=256)
bls_disc = {1: 1000, 2: 949, 4: 797, 8: 728, 16: 677, 32: 627, 64: 576, 128: 519}
def bls_msm(k):
    if k == 0: return 0
    keys = sorted(bls_disc); d = bls_disc[max(x for x in keys if x <= k)] if k <= 128 else 519
    return k * 12000 * d // 1000
def msm(curve, k): return k * BN['mul'] if curve is BN else bls_msm(k)

def verify_gas(curve, n):
    pairs = 4 + 3 * n
    terms = 8 + 5 * n
    return curve['pair_base'] + pairs * curve['pair'] + msm(curve, terms)

def spend_gas(curve, k):
    if k < 2: pairs, terms = 1, 0
    else: pairs, terms = 1 + 3 * k, 2 * (k - 1)
    g = curve['pair_base'] + pairs * curve['pair']
    g += sum(msm(curve, terms) for _ in range(k)) if k >= 2 else 0
    g += k * (k - 1) * curve['add']
    calldata = k * (3 * curve['g1_bytes'] + 3 * curve['g2_bytes'] + 32) + k * (k - 1) * 32
    return g, calldata * 16

print("\n[5] Estimated precompile gas (no storage, no Sigma-proof verification)")
print("    transact Verify (V1-V4 batched), n receivers:")
for n in (1, 2, 4, 8, 16, 32):
    print(f"      n={n:>2}: BN254 {verify_gas(BN, n):>10,}   BLS12-381 {verify_gas(BLS, n):>10,}")
print("    spend / compaction check for one closed epoch with k credits (compute + calldata):")
for k in (1, 2, 4, 8, 16, 32):
    b, bc = spend_gas(BN, k); l, lc = spend_gas(BLS, k)
    print(f"      k={k:>2}: BN254 {b:>11,} + {bc:>9,} calldata   BLS12-381 {l:>11,} + {lc:>9,} calldata")
