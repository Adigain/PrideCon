//! The 61-bit Mersenne prime field used by the fast exponent backend.

use ark_ff::fields::{Fp64, MontBackend, MontConfig};

#[derive(MontConfig)]
#[modulus = "2305843009213693951"]
#[generator = "37"]
pub struct F61Config;

pub type F61 = Fp64<MontBackend<F61Config, 1>>;
