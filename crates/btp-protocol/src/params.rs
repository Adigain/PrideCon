//! Protocol parameters (SPEC 2).

#[derive(Clone, Debug)]
pub struct Params {
    pub max_balance: u64,
    pub k_max: u32,
    pub epoch_len_blocks: u64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            max_balance: (1u64 << 32) - 1,
            k_max: 8,
            epoch_len_blocks: 16,
        }
    }
}
