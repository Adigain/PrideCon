#!/usr/bin/env python3
"""Regenerate every figure and table from results/ into results/figures (PDF, PNG and Markdown)."""
import argparse
import csv
import glob
import json
import math
import os
import statistics
from collections import defaultdict

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt

SURFACE, INK, INK2, MUTED, GRID = "#fcfcfb", "#0b0b0b", "#52514e", "#898781", "#e6e5e1"
SERIES = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"]
MARKERS = ["o", "s", "^", "D", "v", "P", "X", "*"]
BLUE_ORDINAL = ["#86b6ef", "#5598e7", "#2a78d6", "#1c5cab", "#0d366b"]

SCHEMES = ["ours", "PriDeFUL option 2", "PriDeFUL option 1", "PriDe CT", "Construction II"]
SLOT = {name: i for i, name in enumerate(SCHEMES)}
SLOT["baseline"] = SLOT["PriDeFUL option 2"]

plt.rcParams.update({
    "figure.facecolor": SURFACE, "axes.facecolor": SURFACE, "savefig.facecolor": SURFACE,
    "font.size": 9, "axes.titlesize": 10, "axes.titleweight": "bold", "axes.titlecolor": INK,
    "axes.labelcolor": INK2, "axes.edgecolor": MUTED, "axes.linewidth": 0.8,
    "axes.spines.top": False, "axes.spines.right": False,
    "axes.grid": True, "grid.color": GRID, "grid.linestyle": "-", "grid.linewidth": 0.6,
    "xtick.color": MUTED, "ytick.color": MUTED, "xtick.labelcolor": INK2, "ytick.labelcolor": INK2,
    "lines.linewidth": 1.6, "lines.markersize": 5.5, "legend.frameon": False, "legend.fontsize": 8,
    "text.color": INK,
})

T95 = {1: 12.71, 2: 4.30, 3: 3.18, 4: 2.78, 5: 2.57, 6: 2.45, 7: 2.36, 8: 2.31, 9: 2.26, 10: 2.23}


def mean_ci(xs):
    xs = [float(x) for x in xs]
    m = statistics.fmean(xs)
    if len(xs) < 2:
        return m, 0.0
    t = T95.get(len(xs) - 1, 1.96)
    return m, t * statistics.stdev(xs) / math.sqrt(len(xs))


def wilson(k, n, z=1.96):
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return p, p - (c - h), (c + h) - p


def read(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def pick(results, exp, pattern):
    hits = sorted(glob.glob(os.path.join(results, exp, pattern)))
    if len(hits) > 1:
        print(f"  note: several {exp} files match {pattern}; using {os.path.basename(hits[0])}")
    return hits[0] if hits else None


class Out:
    def __init__(self, d):
        self.d = d
        os.makedirs(d, exist_ok=True)
        self.written = []

    def fig(self, fig, name):
        for ext in ("pdf", "png"):
            fig.savefig(os.path.join(self.d, f"{name}.{ext}"), dpi=200, bbox_inches="tight")
        plt.close(fig)
        self.written.append(name)

    def table(self, name, header, rows, note=""):
        lines = ["| " + " | ".join(header) + " |", "|" + "---|" * len(header)]
        lines += ["| " + " | ".join(str(c) for c in r) + " |" for r in rows]
        with open(os.path.join(self.d, f"{name}.md"), "w") as f:
            f.write((note + "\n\n" if note else "") + "\n".join(lines) + "\n")
        self.written.append(name + ".md")


def series_style(slot, hollow=False):
    c = SERIES[slot]
    return dict(color=c, marker=MARKERS[slot], markerfacecolor=SURFACE if hollow else c,
                markeredgecolor=c, markeredgewidth=1.2)


def reference_line(ax, y, label, horizontal=True, left=False):
    if horizontal:
        ax.axhline(y, color=MUTED, linewidth=1.0, zorder=1)
        ax.annotate(label, xy=(0 if left else 1, y), xycoords=("axes fraction", "data"),
                    xytext=(2 if left else -2, 3), textcoords="offset points",
                    ha="left" if left else "right", va="bottom", color=INK2, fontsize=7.5)
    else:
        ax.axvline(y, color=MUTED, linewidth=1.0, zorder=1)
        ax.annotate(label, xy=(y, 1), xycoords=("data", "axes fraction"), xytext=(3, -2),
                    textcoords="offset points", ha="left", va="top", color=INK2, fontsize=7.5)


def log2_axis(ax, ticks):
    ax.set_xscale("log", base=2)
    ax.set_xticks(ticks)
    ax.set_xticklabels([str(t) for t in ticks])
    ax.minorticks_off()


def e1(results, out):
    path = pick(results, "e1", "*.meta.json")
    if not path:
        return print("  E1: no results, skipped")
    m = json.load(open(path))
    t = m["totals"]
    rows = [
        ["random workloads", f"{t['cases']:,}"],
        ["runs (workload x order)", f"{t['runs']:,}"],
        ["payments / credits / compactions / mints", f"{t['pays']:,} / {t['credits']:,} / {t['compactions']:,} / {t['mints']:,}"],
        ["orders with a different final state", t["divergent_orders"]],
        ["honest operations rejected", t["rejected"]],
        ["native vs mirror mismatches", t["native_mirror_mismatches"]],
        ["balance reads != plaintext", t["read_failures"]],
        ["native W != W from records", t["w_mismatches"]],
        ["key failures", t["key_failures"]],
    ]
    out.table("e1_summary", ["E1 order-independence", "value"], rows,
              note=f"Source: {os.path.basename(path)} (git {str(m['git']['commit'])[:7]}). "
                   f"All checks passed: {m['all_checks_passed']}.")


def e2(results, out):
    path = pick(results, "e2", "seed*-epochs*[0-9].csv")
    if not path:
        return print("  E2: no results, skipped")
    rows = read(path)
    unl = [r for r in rows if r["variant"] == "unlimited"]
    default = [r for r in unl if r["sweep"] == "default"]

    def curve(sweep, key, metric, scheme, cast=float):
        pts = defaultdict(list)
        for r in unl:
            if r["scheme"] == scheme and r["sweep"] in (sweep, "default"):
                pts[cast(r[key])].append(r[metric])
        xs = sorted(pts)
        return xs, [mean_ci(pts[x]) for x in xs]

    def sweep_fig(sweep, key, xlabel, name, title, cast=float, ticks=None, logx=None):
        fig, ax = plt.subplots(figsize=(5.2, 3.2))
        for s in SCHEMES:
            xs, mc = curve(sweep, key, "first_try_rate", s, cast)
            ax.errorbar(xs, [m for m, _ in mc], yerr=[c for _, c in mc], label=s, capsize=0,
                        elinewidth=1.0, **series_style(SLOT[s]))
        if ticks:
            log2_axis(ax, ticks)
        elif logx:
            ax.set_xscale("log")
        ax.set_ylim(0, 1.08)
        ax.annotate("option 1 and PriDe CT coincide at 1.0 (they always land)", xy=(0.5, 1.0),
                    xycoords=("axes fraction", "data"), xytext=(0, 5), textcoords="offset points",
                    ha="center", va="bottom", fontsize=7.5, color=INK2)
        ax.set_xlabel(xlabel)
        ax.set_ylabel("share landing at the first\neligible block")
        ax.set_title(title, loc="left")
        ax.legend(loc="lower left", ncol=2)
        out.fig(fig, name)

    sweep_fig("ring", "ring", "ring size t", "e2_first_try_vs_ring",
              "Conflicts grow with the ring size", cast=int, ticks=[2, 4, 8, 16, 32, 64])
    sweep_fig("rate", "rate", "arrival rate (payments per block)", "e2_first_try_vs_rate",
              "Conflicts grow with the load", logx=True)
    sweep_fig("accounts", "accounts", "accounts N", "e2_first_try_vs_accounts",
              "Fewer accounts, more overlap", cast=int, logx=True)

    fig, (a1, a2) = plt.subplots(1, 2, figsize=(7.2, 2.8), gridspec_kw={"wspace": 0.55})
    for i, s in enumerate(SCHEMES):
        rs = [r for r in default if r["scheme"] == s]
        share, sh_ci = mean_ci([r["fs_fraction"] for r in rs])
        a1.barh(i, share, height=0.55, color=SERIES[SLOT[s]], xerr=sh_ci, ecolor=INK2)
        a1.annotate(f"{share:.1%}", (share, i), xytext=(3, 0), textcoords="offset points",
                    va="center", fontsize=7.5, color=INK2)
        if share > 0:
            tm, tm_ci = mean_ci([r["fs_time_mean"] for r in rs])
            a2.barh(i, tm, height=0.55, color=SERIES[SLOT[s]], xerr=tm_ci, ecolor=INK2)
            a2.annotate(f"{tm:.1f}", (tm, i), xytext=(3, 0), textcoords="offset points",
                        va="center", fontsize=7.5, color=INK2)
        else:
            a2.annotate("never", (0, i), xytext=(3, 0), textcoords="offset points",
                        va="center", fontsize=7.5, color=INK2)
    for ax in (a1, a2):
        ax.set_yticks(range(len(SCHEMES)))
        ax.invert_yaxis()
        ax.grid(axis="y", visible=False)
    a1.set_yticklabels(SCHEMES)
    a2.set_yticklabels([])
    a1.set_xlim(0, 1.18)
    a1.set_xticks([0, 0.25, 0.5, 0.75, 1.0])
    a2.set_xlim(0, a2.get_xlim()[1] * 1.15)
    a1.set_xlabel("share of payments forward-secure")
    a2.set_xlabel("mean blocks until forward-secure")
    a1.set_title("Forward secrecy (default point)", loc="left")
    out.fig(fig, "e2_forward_secrecy")

    cols = ["first_try_rate", "attempts_mean", "latency_mean", "latency_p95", "fs_fraction",
            "fs_time_mean", "fs_time_p95", "messages_per_payment"]
    trows = []
    for s in SCHEMES:
        rs = [r for r in default if r["scheme"] == s]
        trows.append([s] + [f"{mean_ci([r[c] for r in rs])[0]:.3f}" for c in cols])
    d0 = default[0]
    out.table("e2_default", ["scheme"] + cols, trows,
              note=f"E2 default point: N={d0['accounts']}, t={d0['ring']}, rate={d0['rate']}/block, "
                   f"L={d0['epoch_len']}, K_MAX={d0['k_max']}, uniform popularity; mean of "
                   f"{len(default) // len(SCHEMES)} seeds. Source: {os.path.basename(path)}.")

    cap = [r for r in rows if r["variant"] == "gas-capacity"]
    if cap:
        fig, ax = plt.subplots(figsize=(5.2, 3.2))
        for s in ("ours", "PriDeFUL option 2"):
            pts = defaultdict(list)
            for r in cap:
                if r["scheme"] == s:
                    pts[float(r["rate"])].append(r["latency_mean"])
            xs = sorted(pts)
            per_block = next(r["capacity"] for r in cap if r["scheme"] == s)
            ax.plot(xs, [mean_ci(pts[x])[0] for x in xs], label=f"{s} ({per_block} tx/block)",
                    **series_style(SLOT[s]))
        ax.set_xscale("log")
        ax.set_yscale("log")
        ax.set_xlabel("arrival rate (payments per block)")
        ax.set_ylabel("mean blocks from arrival to landing")
        ax.set_title("With block space from E4's gas (100M gas/block)", loc="left")
        ax.legend(loc="lower right")
        out.fig(fig, "e2_gas_capacity")


def e3(results, out):
    path = pick(results, "e3", "*-hist.csv")
    if not path:
        return print("  E3: no results, skipped")
    hist = defaultdict(lambda: defaultdict(int))
    for r in read(path):
        key = (r["sweep"], int(r["epoch_len"]), float(r["zipf_s"]), int(r["ring"]), int(r["accounts"]),
               float(r["rate"]))
        hist[key][int(r["k"])] += int(r["count"])

    def ccdf(h):
        total = sum(h.values())
        ks = sorted(h)
        xs, ys, above = [], [], total
        for k in ks:
            above -= h[k]
            if above > 0:
                xs.append(k)
                ys.append(above / total)
        return xs, ys

    fig, (a1, a2) = plt.subplots(1, 2, figsize=(7.4, 3.0), gridspec_kw={"wspace": 0.3})
    ls = [4, 8, 16, 32, 64]
    for i, L in enumerate(ls):
        h = next(v for k, v in hist.items() if k[0] == "epoch_len x zipf" and k[1] == L and k[2] == 0.0)
        xs, ys = ccdf(h)
        a1.plot(xs, ys, color=BLUE_ORDINAL[i], marker=MARKERS[i], label=f"L = {L}")
    for i, s in enumerate([0.0, 0.8, 1.2]):
        h = next(v for k, v in hist.items() if k[0] == "epoch_len x zipf" and k[1] == 16 and k[2] == s)
        xs, ys = ccdf(h)
        a2.plot(xs, ys, color=BLUE_ORDINAL[[0, 2, 4][i]], marker=MARKERS[i], markevery=0.1,
                label="uniform" if s == 0 else f"Zipf s = {s}")
    for ax in (a1, a2):
        ax.set_yscale("log")
        reference_line(ax, 8, " K_MAX = 8", horizontal=False)
        ax.set_xlabel("k (credits per account per epoch)")
    a1.legend(loc="upper right", bbox_to_anchor=(1.0, 0.9))
    a2.legend(loc="lower left")
    a2.set_xscale("log")
    a1.set_ylabel("share of account-epochs with more than k")
    a1.set_title("Uniform popularity, epoch length L", loc="left")
    a2.set_title("L = 16, receiver popularity", loc="left")
    out.fig(fig, "e3_k_ccdf")

    spath = pick(results, "e3", "*-summary.csv")
    if spath:
        srows = read(spath)
        agg = defaultdict(list)
        for r in srows:
            agg[(r["sweep"], r["accounts"], r["ring"], r["rate"], r["zipf_s"], r["epoch_len"])].append(r)
        trows = []
        for k, rs in agg.items():
            trows.append([k[0], k[1], k[2], k[3], k[4], k[5]] +
                         [f"{mean_ci([r[c] for r in rs])[0]:.4g}" for c in
                          ("k_mean", "k_p99", "k_max_seen", "share_above_4", "share_above_8", "share_above_16")])
        out.table("e3_summary", ["sweep", "N", "t", "rate", "zipf s", "L", "mean k", "p99 k", "max k",
                                 "share k>4", "share k>8", "share k>16"], trows,
                  note=f"E3: credits per account per epoch with no cap; mean over seeds. Source: {os.path.basename(spath)}.")


def e4(results, out):
    path = pick(results, "e4", "*.csv")
    if not path:
        return print("  E4: no results, skipped")
    rows = read(path)
    cap = 1 << 24

    def pts(contract, op, key):
        p = sorted((int(r[key]), int(r["gas_used"]) / 1e6, r["model_precompile_gas"]) for r in rows
                   if r["contract"] == contract and r["operation"] == op)
        return p

    fig, ax = plt.subplots(figsize=(5.4, 3.3))
    fresh, warm = pts("PrideConcurrent", "transact", "n"), pts("PrideConcurrent", "transact_warm", "n")
    base = pts("PriDeFULBaseline", "transact", "n")
    ax.plot([p[0] for p in fresh], [p[1] for p in fresh], label="ours, receivers new in the epoch",
            **series_style(SLOT["ours"]))
    ax.plot([p[0] for p in warm], [p[1] for p in warm], label="ours, receivers already credited",
            **series_style(SLOT["ours"], hollow=True))
    ax.plot([p[0] for p in base], [p[1] for p in base], label="baseline (option 2)",
            **series_style(SLOT["baseline"]))
    ax.plot([p[0] for p in fresh], [int(p[2]) / 1e6 for p in fresh], color=MUTED, linewidth=1.0,
            marker="", label="cost model, precompiles only")
    reference_line(ax, cap / 1e6, "2^24 per-transaction cap (Osaka)", left=True)
    log2_axis(ax, [1, 2, 4, 8, 16, 32])
    ax.set_xlabel("ring size n")
    ax.set_ylabel("gas per transaction (millions)")
    ax.set_title("transact: measured on anvil vs model", loc="left")
    ax.legend(loc="upper left", bbox_to_anchor=(0.0, 0.72))
    out.fig(fig, "e4_gas_vs_ring")

    fig, ax = plt.subplots(figsize=(5.4, 3.3))
    for op, label, slot, hollow in (("spend", "spend (V5 over k credits)", SLOT["ours"], False),
                                    ("compact", "compaction", SLOT["ours"], True)):
        p = pts("PrideConcurrent", op, "k")
        ax.plot([q[0] for q in p], [q[1] for q in p], label=label, **series_style(slot, hollow))
        ax.plot([q[0] for q in p], [int(q[2]) / 1e6 for q in p], color=MUTED, linewidth=1.0,
                marker=MARKERS[slot], markerfacecolor=SURFACE if hollow else MUTED,
                markeredgecolor=MUTED, markersize=3.5)
    ax.plot([], [], color=MUTED, linewidth=1.0, label="cost model, precompiles only (grey)")
    reference_line(ax, cap / 1e6, "2^24 per-transaction cap (Osaka)", left=True)
    reference_line(ax, 8, " K_MAX = 8", horizontal=False)
    ax.set_xlabel("credits k in the open epoch")
    ax.set_ylabel("gas per transaction (millions)")
    ax.set_title("Spend and compaction vs k", loc="left")
    ax.legend(loc="upper left", bbox_to_anchor=(0.3, 0.98))
    out.fig(fig, "e4_gas_vs_k")

    trows = [[r["contract"], r["operation"], r["n"], r["k"], f"{int(r['gas_used']):,}",
              f"{int(r['model_precompile_gas']):,}" if r["model_precompile_gas"] else "",
              "yes" if r["over_2_24"] == "true" else ""]
             for r in rows if r["operation"] not in ("register", "mint")]
    for op in ("register", "mint"):
        for c in ("PrideConcurrent", "PriDeFULBaseline"):
            g = [int(r["gas_used"]) for r in rows if r["operation"] == op and r["contract"] == c]
            if g:
                trows.append([c, op + " (median)", "", "", f"{int(statistics.median(g)):,}", "", ""])
    out.table("e4_gas", ["contract", "operation", "n", "k", "gas used", "model (precompiles)", "> 2^24"],
              trows, note=f"E4: receipts' gasUsed on anvil (Prague). Source: {os.path.basename(path)}.")


def e5(results, out):
    path = pick(results, "e5", "*.csv")
    if not path:
        return print("  E5: no results, skipped")
    rows = read(path)
    mpath = pick(results, "e2", "*-e5model.csv")
    model = read(mpath) if mpath else []

    def measured(contract, keep, key):
        tot = defaultdict(lambda: [0, 0])
        for r in rows:
            if r["contract"] == contract and keep(r):
                tot[int(r[key])][0] += int(r["succeeded"])
                tot[int(r[key])][1] += int(r["submitted"])
        xs = sorted(tot)
        w = [wilson(*tot[x]) for x in xs]
        return xs, [p for p, _, _ in w], [[lo for _, lo, _ in w], [hi for _, _, hi in w]]

    fig, (a1, a2) = plt.subplots(1, 2, figsize=(7.6, 3.1), gridspec_kw={"wspace": 0.25, "width_ratios": [2, 1]})
    for contract, name, mcol in (("PrideConcurrent", "ours", "ours_success_rate"),
                                 ("PriDeFULBaseline", "baseline", "option2_success_rate")):
        xs, ys, err = measured(contract, lambda r: int(r["ring_size"]) == 4, "senders")
        a1.errorbar(xs, ys, yerr=err, linestyle="none", capsize=2,
                    label=f"{name}, measured on anvil", **series_style(SLOT[name]))
        mx = sorted((int(r["senders"]), float(r[mcol])) for r in model if int(r["ring_size"]) == 4)
        a1.plot([p[0] for p in mx], [p[1] for p in mx], color=SERIES[SLOT[name]], linewidth=1.0,
                marker="", label=f"{name}, conflict model")
        xs2, ys2, err2 = measured(contract, lambda r: int(r["senders"]) == 16, "ring_size")
        a2.errorbar(xs2, ys2, yerr=err2, linestyle="none", capsize=2, **series_style(SLOT[name]))
        mx2 = sorted((int(r["ring_size"]), float(r[mcol])) for r in model if int(r["senders"]) == 16)
        a2.plot([p[0] for p in mx2], [p[1] for p in mx2], color=SERIES[SLOT[name]], linewidth=1.0, marker="")
    log2_axis(a1, [1, 2, 4, 8, 16, 32])
    log2_axis(a2, [2, 4, 8])
    for ax in (a1, a2):
        ax.set_ylim(0, 1.05)
    a1.set_xlabel("concurrent senders in one block (n = 4)")
    a2.set_xlabel("ring size n (16 senders)")
    a1.set_ylabel("share of transactions that land\n(95% Wilson interval)")
    a1.set_title("Concurrent payments in one block", loc="left")
    a1.legend(loc="lower left")
    out.fig(fig, "e5_concurrency")

    agg = defaultdict(list)
    for r in rows:
        agg[(r["contract"], int(r["senders"]), int(r["ring_size"]))].append(r)
    trows = []
    for (c, s, n), rs in sorted(agg.items(), key=lambda kv: (kv[0][2], kv[0][1], kv[0][0])):
        sub, ok = sum(int(r["submitted"]) for r in rs), sum(int(r["succeeded"]) for r in rs)
        reasons = sorted({x.split(":")[0] for r in rs for x in r["revert_reasons"].split(";") if x})
        trows.append([c, s, n, len(rs), f"{ok}/{sub}", f"{ok / sub:.3f}", ", ".join(reasons),
                      max(int(r["blocks_spanned"]) for r in rs)])
    out.table("e5_load", ["contract", "senders", "ring", "rounds", "landed", "share", "revert reasons",
                          "max blocks"], trows,
              note=f"E5 on anvil (Prague, 100M gas/block). Source: {os.path.basename(path)}.")


def e6(results, out):
    path = pick(results, "e6", "*-attacks.csv")
    if not path:
        return print("  E6: no results, skipped")
    rows = read(path)
    designs = list(dict.fromkeys(r["design"] for r in rows))
    labels = list(dict.fromkeys((r["design"], r["attack"]) for r in rows))
    fig, ax = plt.subplots(figsize=(5.6, 0.22 * len(labels) + 1.0))
    for slot, backend in enumerate(("exponent", "bls12-381")):
        ys, xs, lo, hi = [], [], [], []
        for i, (d, a) in enumerate(labels):
            r = next((r for r in rows if r["design"] == d and r["attack"] == a and r["backend"] == backend), None)
            if r:
                ys.append(i + (0.18 if slot else -0.18))
                xs.append(float(r["rate"]))
                lo.append(float(r["rate"]) - float(r["ci95_low"]))
                hi.append(float(r["ci95_high"]) - float(r["rate"]))
        trials = next(r["trials"] for r in rows if r["backend"] == backend)
        ax.errorbar(xs, ys, xerr=[lo, hi], linestyle="none", capsize=0, elinewidth=1.0,
                    label=f"{backend} ({int(trials):,} trials per row)", **series_style(slot))
    reference_line(ax, 0.5, "guessing", horizontal=False)
    ax.set_yticks(range(len(labels)))
    ax.set_yticklabels([f"{d}  {a}" for d, a in labels], fontsize=7.5)
    ax.invert_yaxis()
    ax.set_xlim(0.3, 1.04)
    ax.set_xlabel("adversary success rate (95% Wilson interval)")
    ax.set_title("Attacks: flawed designs break, the final design holds", loc="left")
    ax.grid(axis="y", visible=False)
    for i in range(1, len(labels)):
        if labels[i][0] != labels[i - 1][0]:
            ax.axhline(i - 0.5, color=GRID, linewidth=0.8)
    ax.legend(loc="upper center", bbox_to_anchor=(0.4, -0.1), ncol=2)
    out.fig(fig, "e6_attacks")
    del designs

    out.table("e6_attacks", ["backend", "design", "attack", "trials", "rate", "95% CI", "expected"],
              [[r["backend"], r["design"], r["attack"], r["trials"], f"{float(r['rate']):.4f}",
                f"[{float(r['ci95_low']):.4f}, {float(r['ci95_high']):.4f}]", r["expected"]] for r in rows],
              note=f"E6 two-candidate game (Def. 2). Source: {os.path.basename(path)}.")
    rpath = pick(results, "e6", "*-rank.csv")
    if rpath:
        rr = [r for r in read(rpath) if r["seed"] == "0"]
        out.table("e6_rank", ["game", "case", "ranks real (G1, G2, GT)", "ranks random", "leak", "expected"],
                  [[r["game"], r["case"], f"({r['g1_real']}, {r['g2_real']}, {r['gt_real']})",
                    f"({r['g1_rand']}, {r['g2_rand']}, {r['gt_rand']})", r["leak"], r["expected_leak"]]
                   for r in rr],
                  note=f"Generic-group rank test (ggm.py rows report GT only). Source: {os.path.basename(rpath)}.")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--results", default="results")
    ap.add_argument("--out", default=os.path.join("results", "figures"))
    a = ap.parse_args()
    out = Out(a.out)
    for name, f in (("E1", e1), ("E2", e2), ("E3", e3), ("E4", e4), ("E5", e5), ("E6", e6)):
        print(f"{name} ...")
        f(a.results, out)
    print(f"wrote {len(out.written)} files to {a.out}: {', '.join(out.written)}")


if __name__ == "__main__":
    main()
