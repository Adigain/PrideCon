//! The Rust interface declarations have the selectors of the compiled contracts.

use alloy_sol_types::SolCall;
use btp_protocol::abi::sol::{IPriDeFULBaseline as Bc, IPrideConcurrent as C};
use std::path::Path;

fn method_ids(name: &str) -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../contracts/out/{name}.sol/{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "{} missing: run `forge build` in contracts/",
            path.display()
        )
    });
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    json["methodIdentifiers"].clone()
}

fn check<T: SolCall>(ids: &serde_json::Value) {
    let found = ids[T::SIGNATURE]
        .as_str()
        .unwrap_or_else(|| panic!("{} not in the contract ABI", T::SIGNATURE));
    assert_eq!(found, hex::encode(T::SELECTOR), "{}", T::SIGNATURE);
}

#[test]
fn pride_concurrent_selectors() {
    let ids = method_ids("PrideConcurrent");
    check::<C::registerCall>(&ids);
    check::<C::mintCall>(&ids);
    check::<C::transactCall>(&ids);
    check::<C::compactCall>(&ids);
    check::<C::getAccountCall>(&ids);
    check::<C::currentEpochCall>(&ids);
    check::<C::numAccountsCall>(&ids);
    check::<C::epochLenCall>(&ids);
    check::<C::kMaxCall>(&ids);
    check::<C::usedTagCall>(&ids);
}

#[test]
fn baseline_selectors() {
    let ids = method_ids("PriDeFULBaseline");
    check::<Bc::registerCall>(&ids);
    check::<Bc::mintCall>(&ids);
    check::<Bc::transactCall>(&ids);
    check::<Bc::getAccountCall>(&ids);
    check::<Bc::numAccountsCall>(&ids);
}
