//! Writes the Foundry fixtures (SPEC 15); expected outcomes and states come from a mirror-ledger replay.

use alloy_sol_types::SolValue;
use ark_ff::UniformRand;
use btp_algebra::bls::Bls;
use btp_algebra::codec::{fr_to_bytes32, g1_to_eip2537, g2_to_eip2537, to_hex};
use btp_algebra::{Backend, GroupElem};
use btp_protocol::abi;
use btp_protocol::apply::{credit_record, debit_record};
use btp_protocol::encode::{batch_weight, chain_of, chain_step, record_hash, tx_tag, txhash};
use btp_protocol::ledger::{Ledger, TraceOp};
use btp_protocol::read::BsgsTable;
use btp_protocol::workload::{generate, run, Order, RunConfig, WorkloadParams};
use btp_protocol::{Crs, Params, ProtocolError, Record};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

type B = Bls;
type Fr = <Bls as Backend>::Fr;

const CODEC_SEED: u64 = 2537;
const EPOCH_LEN: u64 = 16;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn write_json(dir: &Path, name: &str, doc: &Value) -> std::io::Result<()> {
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(doc)? + "\n")?;
    eprintln!("wrote {}", path.display());
    Ok(())
}

fn codec_scalars() -> Vec<Fr> {
    let mut ks: Vec<Fr> = vec![
        Fr::from(0u64),
        Fr::from(1u64),
        Fr::from(2u64),
        Fr::from(3u64),
        Fr::from(u64::MAX),
        -Fr::from(1u64),
    ];
    let mut rng = ChaCha20Rng::seed_from_u64(CODEC_SEED);
    ks.extend((0..4).map(|_| Fr::rand(&mut rng)));
    ks
}

fn codec_fixture() -> Value {
    let cases: Vec<_> = codec_scalars()
        .iter()
        .map(|k| {
            json!({
                "k": to_hex(&fr_to_bytes32(k)),
                "g1": to_hex(&g1_to_eip2537(&Bls::g1().smul(k))),
                "g2": to_hex(&g2_to_eip2537(&Bls::g2().smul(k))),
            })
        })
        .collect();
    json!({
        "description": "k, k*g1 and k*g2 in EIP-2537 encoding (SPEC Section 3), from btp-algebra::codec",
        "seed": CODEC_SEED,
        "count": cases.len(),
        "cases": cases,
    })
}

type Driver = btp_protocol::driver::Driver<B>;

fn scenario_credits() -> Driver {
    let mut d = Driver::new(b"fixtures", 4, 8, 101);
    d.mint_all(100);
    d.pay(1, &[2, 3], &[10, 0]);
    d.pay(2, &[3, 4], &[5, 0]);
    d.next_epoch();
    d.pay(3, &[1, 2], &[7, 0]);
    d.pay(4, &[1], &[1]);
    d
}

fn scenario_spend() -> Driver {
    let mut d = Driver::new(b"fixtures", 5, 8, 102);
    d.mint_all(1000);
    d.pay(2, &[1, 5], &[11, 0]);
    d.pay(3, &[1, 5], &[22, 0]);
    d.pay(4, &[1], &[33]);
    d.pay(1, &[5], &[50]);
    d.next_epoch();
    d.pay(1, &[2, 3], &[60, 0]);
    d.pay(2, &[1], &[4]);
    d.next_epoch();
    d.pay(1, &[4], &[1]);
    d
}

fn scenario_compaction() -> Driver {
    let mut d = Driver::new(b"fixtures", 4, 8, 103);
    d.mint_all(500);
    d.pay(2, &[1], &[10]);
    d.pay(3, &[1], &[20]);
    d.next_epoch();
    let c = d.build_compact(1);
    d.op(TraceOp::Compact(c)).expect("compaction");
    d.pay(2, &[1], &[5]);
    d.pay(3, &[1, 4], &[6, 0]);
    d.next_epoch();
    let c = d.build_compact(1);
    let (tx, pls) = (d.build_pay(1, &[4], &[30]), [30]);
    d.op(TraceOp::Compact(c)).expect("compaction");
    d.submit(tx, &pls);
    d.pay(2, &[1], &[7]);
    d.next_epoch();
    let c = d.build_compact(1);
    let (tx, pls) = (d.build_pay(1, &[3], &[40]), [40]);
    d.submit(tx, &pls);
    d.op(TraceOp::Compact(c)).expect("compaction after debit");
    d
}

fn scenario_rejects() -> Driver {
    use ProtocolError::*;
    let mut d = Driver::new(b"fixtures", 6, 2, 104);
    d.mint_all(1000);
    let z = Bls::g1().smul(&Fr::from(0u64));
    let pk = d.ledger.accounts[0].pk_base;
    d.reject(TraceOp::Register { pk, c: z, d: z }, BadRegistration);

    let honest = d.build_pay(1, &[2, 3], &[10, 0]);
    d.submit(honest.clone(), &[10, 0]);
    d.reject(TraceOp::Transact(honest), DuplicateTag);
    let again = d.build_pay(1, &[4], &[1]);
    d.reject(TraceOp::Transact(again), AlreadyDebited);

    let base = d.build_pay(2, &[3, 4], &[1, 0]);
    let mut x = base.clone();
    x.epoch += 1;
    d.reject(TraceOp::Transact(x), WrongEpoch);
    let mut x = base.clone();
    x.ring[1] = 3;
    d.reject(TraceOp::Transact(x), BadRing);
    let mut x = base.clone();
    x.ring[1] = 2;
    d.reject(TraceOp::Transact(x), BadRing);
    let mut x = base.clone();
    x.ring[1] = 99;
    d.reject(TraceOp::Transact(x), BadRing);
    let mut x = base.clone();
    x.recv.pop();
    d.reject(TraceOp::Transact(x), BadRing);
    let mut x = base.clone();
    x.recv[1].h1 += Bls::g2();
    d.reject(TraceOp::Transact(x), HandleCheck);

    d.pay(4, &[6], &[3]);
    d.pay(5, &[6], &[4]);
    let capped = d.build_pay(3, &[6], &[1]);
    d.reject(TraceOp::Transact(capped), CreditCap);
    d.submit(base, &[1, 0]);

    d.next_epoch();
    let bal = d.snap[5];
    let over = d.build_pay_with(6, &[1], &[1], bal + 1);
    d.reject(TraceOp::Transact(over), SpendCheck);
    let base = d.build_pay(6, &[1], &[5]);
    let mut x = base.clone();
    x.spend.epochs[0].records.pop();
    d.reject(TraceOp::Transact(x), RecordChain);
    let mut x = base.clone();
    x.spend.epochs[0].hints[0][0] += Fr::from(1u64);
    d.reject(TraceOp::Transact(x), BadHint);
    let c = d.build_compact(2);
    let mut x = c.clone();
    x.d_prime += Bls::g1();
    d.reject(TraceOp::Compact(x), SpendCheck);
    let mut x = c.clone();
    x.account = 99;
    d.reject(TraceOp::Compact(x), UnknownAccount);
    let mut x = c.clone();
    x.epoch = 1;
    d.reject(TraceOp::Compact(x), WrongEpoch);
    d.submit(base, &[5]);
    d.op(TraceOp::Compact(c)).expect("honest compaction");
    d
}

fn scenario_workload() -> (Crs<B>, Params, Vec<TraceOp<B>>) {
    let p = WorkloadParams {
        accounts: 5,
        epochs: 4,
        pays_per_epoch: 4,
        ring_size: 2,
        max_payload: 30,
        initial_mint: 200,
        compact_prob: 0.3,
        mint_prob: 0.2,
        ..Default::default()
    };
    let plan = generate(&p, &mut ChaCha20Rng::seed_from_u64(105));
    let cfg = RunConfig {
        build_seed: 105,
        order: Order::Seeded(105),
        check_reads: true,
        check_each_op: false,
    };
    let out = run::<B>(&plan, &p, &cfg, &BsgsTable::new(p.max_balance()));
    assert!(out.stats.all_ok(), "{:?}", out.failures);
    (out.mirror.crs.clone(), out.mirror.params.clone(), out.trace)
}

fn hex32(b: &[u8; 32]) -> String {
    to_hex(b)
}

fn protocol_fixture(
    name: &str,
    description: &str,
    crs: &Crs<B>,
    params: &Params,
    trace: &[TraceOp<B>],
) -> Value {
    let mut l = Ledger::new_mirror(crs.clone(), params.clone());
    let mut steps = vec![];
    for op in trace {
        let res = l.apply_op(op);
        let (kind, data) = match op {
            TraceOp::Register { pk, c, d } => (
                "register",
                (abi::g1(pk), abi::g1(c), abi::g1(d)).abi_encode_params(),
            ),
            TraceOp::Mint { id, amount } => ("mint", (*id, *amount).abi_encode_params()),
            TraceOp::NextEpoch => ("nextEpoch", vec![]),
            TraceOp::Transact(tx) => ("transact", abi::tx(tx).abi_encode()),
            TraceOp::Compact(c) => ("compact", abi::compaction(c).abi_encode()),
        };
        let tag = match (op, &res) {
            (TraceOp::Transact(tx), Ok(())) => hex32(&fr_to_bytes32(&tx_tag(tx))),
            _ => hex32(&[0u8; 32]),
        };
        let state: Vec<String> = l
            .accounts
            .iter()
            .map(|a| to_hex(&abi::account(a).abi_encode()))
            .collect();
        steps.push(json!({
            "kind": kind,
            "epoch": l.epoch,
            "data": to_hex(&data),
            "expect": match res { Ok(()) => "ok".to_string(), Err(e) => format!("{e:?}") },
            "tag": tag,
            "state": state,
        }));
    }
    json!({
        "name": name,
        "description": description,
        "epochLen": EPOCH_LEN,
        "kMax": params.k_max,
        "crs": to_hex(&abi::crs(crs).abi_encode()),
        "count": steps.len(),
        "steps": steps,
    })
}

fn encoding_fixture(trace: &[TraceOp<B>]) -> Value {
    let mut txs = vec![];
    let mut records: Vec<Record<B>> = vec![];
    for op in trace {
        if let TraceOp::Transact(tx) = op {
            let (t, h) = (tx_tag(tx), txhash(tx));
            let n = tx.recv.len() as u32;
            let weights: Vec<String> = (0..2 * n + 2)
                .map(|i| hex32(&fr_to_bytes32(&batch_weight::<B>(&h, i))))
                .collect();
            txs.push(json!({
                "data": to_hex(&abi::tx(tx).abi_encode()),
                "t": hex32(&fr_to_bytes32(&t)),
                "txhash": hex32(&h),
                "weights": weights,
            }));
            records.push(debit_record(tx, t));
            records.extend((0..tx.recv.len()).map(|j| credit_record(tx, j, t)));
        }
    }
    let recs: Vec<Value> = records
        .iter()
        .map(|r| {
            json!({
                "data": to_hex(&abi::record(r).abi_encode()),
                "hash": hex32(&record_hash(r)),
                "chainFromZero": hex32(&chain_step(&[0u8; 32], r)),
            })
        })
        .collect();
    json!({
        "description": "SPEC 4, 7.3, 8: tag t, txhash, batch weights, record hashes and chains, from btp-protocol::encode",
        "txCount": txs.len(),
        "txs": txs,
        "recordCount": recs.len(),
        "records": recs,
        "chainOfAll": hex32(&chain_of(&records)),
    })
}

fn main() -> std::io::Result<()> {
    let dir = fixtures_dir();
    std::fs::create_dir_all(&dir)?;
    write_json(&dir, "codec.json", &codec_fixture())?;

    type Scenario = (&'static str, &'static str, fn() -> Driver);
    let scenarios: [Scenario; 4] = [
        (
            "credits",
            "registrations, mints, concurrent payments over two epochs",
            scenario_credits,
        ),
        (
            "spend",
            "spends with a history of credits: records, hints and V5",
            scenario_spend,
        ),
        (
            "compaction",
            "compaction alone, then debit + compaction in both orders",
            scenario_compaction,
        ),
        (
            "rejects",
            "one malformed operation per error, mixed with honest ones (k_max = 2)",
            scenario_rejects,
        ),
    ];
    for (name, desc, build) in scenarios {
        let d = build();
        let doc = protocol_fixture(name, desc, &d.ledger.crs, &d.ledger.params, &d.trace);
        write_json(&dir, &format!("protocol_{name}.json"), &doc)?;
        if name == "spend" {
            write_json(&dir, "encoding.json", &encoding_fixture(&d.trace))?;
        }
    }
    let (crs, params, trace) = scenario_workload();
    let doc = protocol_fixture(
        "workload",
        "seeded random workload from the E1 generator",
        &crs,
        &params,
        &trace,
    );
    write_json(&dir, "protocol_workload.json", &doc)
}
