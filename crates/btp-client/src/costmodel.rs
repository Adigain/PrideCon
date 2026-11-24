//! Precompile-only gas model (EIP-2537 prices), for comparison with measured gas.

const PAIR_BASE: u64 = 37_700;
const PAIR: u64 = 32_600;
const G1_ADD: u64 = 375;
const G1_BYTES: u64 = 128;
const G2_BYTES: u64 = 256;

const DISCOUNT: [(u64, u64); 8] = [
    (1, 1000),
    (2, 949),
    (4, 797),
    (8, 728),
    (16, 677),
    (32, 627),
    (64, 576),
    (128, 519),
];

pub fn msm(k: u64) -> u64 {
    if k == 0 {
        return 0;
    }
    let d = if k <= 128 {
        DISCOUNT.iter().rfind(|(x, _)| *x <= k).unwrap().1
    } else {
        519
    };
    k * 12_000 * d / 1000
}

pub fn verify_gas(n: u64) -> u64 {
    PAIR_BASE + (4 + 3 * n) * PAIR + msm(8 + 5 * n)
}

pub fn spend_gas(k: u64) -> (u64, u64) {
    let (pairs, terms) = if k < 2 {
        (1, 0)
    } else {
        (1 + 3 * k, 2 * (k - 1))
    };
    let mut g = PAIR_BASE + pairs * PAIR;
    if k >= 2 {
        g += k * msm(terms);
    }
    g += k * k.saturating_sub(1) * G1_ADD;
    let calldata = k * (3 * G1_BYTES + 3 * G2_BYTES + 32) + k * k.saturating_sub(1) * 32;
    (g, calldata * 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_python_model() {
        let verify = [
            (1, 379_468),
            (2, 509_932),
            (4, 786_772),
            (8, 1_311_652),
            (16, 2_341_156),
            (32, 4_344_004),
        ];
        for (n, g) in verify {
            assert_eq!(verify_gas(n), g, "verify n = {n}");
        }
        let spend = [
            (1, 70_300, 18_944),
            (2, 312_202, 38_912),
            (4, 695_536, 81_920),
            (8, 1_852_132, 180_224),
            (16, 5_624_620, 425_984),
            (32, 18_499_516, 1_114_112),
        ];
        for (k, g, c) in spend {
            assert_eq!(spend_gas(k), (g, c), "spend k = {k}");
        }
    }
}
