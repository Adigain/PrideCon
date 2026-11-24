//! Byte encodings and hashes of SPEC 4, 7.3 and 8; the contract recomputes them byte for byte.

use crate::hash::{hash_to_fr, keccak, TAG_RECORD, TAG_SIDE, TAG_TX, TAG_WEIGHT};
use crate::types::{Record, Tx};
use btp_algebra::codec::fr_to_bytes32;
use btp_algebra::Backend;

pub fn enc_core<B: Backend>(tx: &Tx<B>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(tx.epoch.to_be_bytes());
    out.extend(tx.sender.to_be_bytes());
    out.extend((tx.ring.len() as u32).to_be_bytes());
    for id in &tx.ring {
        out.extend(id.to_be_bytes());
    }
    out.extend(B::enc_g1(&tx.c));
    out.extend(B::enc_g1(&tx.d0));
    for r in &tx.recv {
        out.extend(B::enc_g1(&r.d));
        out.extend(B::enc_g1(&r.up.0));
        out.extend(B::enc_g1(&r.up.1));
        out.extend(B::enc_g1(&r.dpk));
        out.extend(B::enc_g1(&r.e));
        out.extend(r.side);
    }
    out
}

pub fn enc_handles<B: Backend>(tx: &Tx<B>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(B::enc_g1(&tx.r1));
    out.extend(B::enc_g1(&tx.r2));
    for r in &tx.recv {
        out.extend(B::enc_g2(&r.h0));
        out.extend(B::enc_g2(&r.h1));
        out.extend(B::enc_g2(&r.h2));
    }
    out
}

pub fn tx_tag<B: Backend>(tx: &Tx<B>) -> B::Fr {
    hash_to_fr(TAG_TX, &enc_core(tx))
}

pub fn txhash<B: Backend>(tx: &Tx<B>) -> [u8; 32] {
    let data = [enc_core(tx), enc_handles(tx), B::enc_g1(&tx.n0)].concat();
    keccak(b"", &data)
}

pub fn batch_weight<B: Backend>(txhash: &[u8; 32], i: u32) -> B::Fr {
    hash_to_fr(TAG_WEIGHT, &[&txhash[..], &i.to_be_bytes()].concat())
}

pub fn enc_record<B: Backend>(rec: &Record<B>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(fr_to_bytes32(&rec.t));
    out.extend(B::enc_g1(&rec.c));
    out.extend(B::enc_g1(&rec.r1));
    out.extend(B::enc_g1(&rec.r2));
    out.extend(B::enc_g2(&rec.h0));
    out.extend(B::enc_g2(&rec.h1));
    out.extend(B::enc_g2(&rec.h2));
    out.push(rec.is_update as u8);
    out
}

pub fn chain_step<B: Backend>(prev: &[u8; 32], rec: &Record<B>) -> [u8; 32] {
    keccak(b"", &[&prev[..], &record_hash(rec)[..]].concat())
}

pub fn record_hash<B: Backend>(rec: &Record<B>) -> [u8; 32] {
    keccak(TAG_RECORD, &enc_record(rec))
}

pub fn chain_of<B: Backend>(records: &[Record<B>]) -> [u8; 32] {
    records.iter().fold([0u8; 32], |acc, r| chain_step(&acc, r))
}

pub fn side_mask<B: Backend>(shared: &B::G1) -> [u8; 32] {
    keccak(TAG_SIDE, &B::enc_g1(shared))
}
