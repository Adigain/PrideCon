//! Local chain: spawn or connect to anvil, deploy from forge artifacts, mine batches as blocks.

use alloy::network::{ReceiptResponse, TransactionBuilder};
use alloy::primitives::{Address, Bytes, TxHash};
use alloy::providers::ext::AnvilApi;
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::rpc::types::{TransactionInput, TransactionReceipt, TransactionRequest};
use alloy_sol_types::SolCall;
use eyre::{bail, eyre, Result, WrapErr};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

pub fn artifact(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../contracts/out/{name}.sol/{name}.json"))
}

pub struct Chain {
    pub provider: DynProvider,
    pub eoas: Vec<Address>,
    anvil: Option<Child>,
}

impl Drop for Chain {
    fn drop(&mut self) {
        if let Some(c) = &mut self.anvil {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

pub fn anvil_version() -> Option<String> {
    let out = Command::new("anvil").arg("--version").output().ok()?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()?
            .trim()
            .to_string(),
    )
}

impl Chain {
    pub async fn spawn(accounts: usize, block_gas_limit: u64) -> Result<Chain> {
        let port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let child = Command::new("anvil")
            .args(["--port", &port.to_string()])
            .args(["--hardfork", "prague"])
            .args(["--gas-limit", &block_gas_limit.to_string()])
            .args(["--code-size-limit", "100000"])
            .args(["--accounts", &accounts.to_string()])
            .args(["--balance", "1000000"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .wrap_err("could not start anvil (is Foundry installed?)")?;
        let url = format!("http://127.0.0.1:{port}");
        let mut chain = Self::connect_inner(&url, Some(child)).await?;
        chain.provider.anvil_set_auto_mine(false).await?;
        chain.eoas.truncate(accounts);
        Ok(chain)
    }

    pub async fn connect(url: &str) -> Result<Chain> {
        let chain = Self::connect_inner(url, None).await?;
        chain.provider.anvil_set_auto_mine(false).await?;
        Ok(chain)
    }

    async fn connect_inner(url: &str, anvil: Option<Child>) -> Result<Chain> {
        let provider = ProviderBuilder::new()
            .disable_recommended_fillers()
            .connect_http(url.parse()?)
            .erased();
        let mut last = None;
        for _ in 0..100 {
            match provider.get_accounts().await {
                Ok(eoas) => {
                    return Ok(Chain {
                        provider,
                        eoas,
                        anvil,
                    });
                }
                Err(e) => last = Some(e),
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(eyre!("anvil at {url} did not answer: {last:?}"))
    }

    pub async fn block_number(&self) -> Result<u64> {
        Ok(self.provider.get_block_number().await?)
    }

    pub async fn mine_blocks(&self, n: u64) -> Result<()> {
        if n > 0 {
            self.provider.anvil_mine(Some(n), None).await?;
        }
        Ok(())
    }

    pub async fn deploy(&self, name: &str, ctor_args: Vec<u8>) -> Result<Address> {
        let path = artifact(name);
        let json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .wrap_err_with(|| format!("{} missing: run `forge build`", path.display()))?,
        )?;
        let code = json["bytecode"]["object"]
            .as_str()
            .ok_or_else(|| eyre!("no bytecode in artifact"))?;
        let code = hex::decode(code.trim_start_matches("0x"))?;
        let req = TransactionRequest::default()
            .from(self.eoas[0])
            .with_deploy_code(Bytes::from([code, ctor_args].concat()))
            .gas_limit(30_000_000);
        let r = self.run_block(vec![req]).await?.remove(0);
        if !r.status() {
            bail!("deploying {name} failed");
        }
        r.contract_address
            .ok_or_else(|| eyre!("no contract address"))
    }

    pub fn request(&self, from: Address, to: Address, data: Vec<u8>) -> TransactionRequest {
        TransactionRequest::default()
            .from(from)
            .to(to)
            .input(TransactionInput::new(Bytes::from(data)))
    }

    pub async fn gas_for(&self, req: &TransactionRequest) -> u64 {
        match self.provider.estimate_gas(req.clone()).await {
            Ok(g) => g + g / 4 + 50_000,
            Err(_) => 15_000_000,
        }
    }

    pub async fn run_block(
        &self,
        reqs: Vec<TransactionRequest>,
    ) -> Result<Vec<TransactionReceipt>> {
        let mut hashes: Vec<TxHash> = vec![];
        for req in reqs {
            let pending = self.provider.send_transaction(req).await?;
            hashes.push(*pending.tx_hash());
        }
        let mut receipts: Vec<Option<TransactionReceipt>> = vec![None; hashes.len()];
        for _ in 0..64 {
            self.provider.evm_mine(None).await?;
            for (h, slot) in hashes.iter().zip(receipts.iter_mut()) {
                if slot.is_none() {
                    *slot = self.provider.get_transaction_receipt(*h).await?;
                }
            }
            if receipts.iter().all(Option::is_some) {
                return Ok(receipts.into_iter().map(Option::unwrap).collect());
            }
        }
        bail!("transactions were not mined")
    }

    pub async fn view<C: SolCall>(&self, to: Address, call: C) -> Result<C::Return> {
        let req = TransactionRequest::default()
            .to(to)
            .input(TransactionInput::new(Bytes::from(call.abi_encode())));
        let out = self.provider.call(req).await?;
        Ok(C::abi_decode_returns(&out)?)
    }
}

pub fn position(r: &TransactionReceipt) -> (u64, u64) {
    (
        r.block_number().unwrap_or(0),
        r.transaction_index().unwrap_or(0),
    )
}
