pub const TRANSFER_BATCH_SIZE: usize = 8;
pub const DEPOSIT_BATCH_SIZE: usize = 1;
pub const EXIT_BATCH_SIZE: usize = 1;
pub const PADDING_INTERVAL: u64 = 60; // sec
pub const PROVER_TIMEOUT: usize = 60; // sec
pub const PROVER_TIMER_TICK: u64 = 5; // sec
pub const PROVER_CYCLE_WAIT: u64 = 5; // sec

pub const DEFAULT_KEYS_PATH: &str = "keys";

lazy_static! {
    pub static ref RUNTIME_CONFIG: RuntimeConfig = RuntimeConfig::new();
}

use std::env;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub transfer_batch_size: usize,
    pub keys_path: String,
    pub max_outstanding_txs: u32,
    pub contract_addr: String,
    pub mainnet_http_endpoint_string: String,
    pub rinkeby_http_endpoint_string: String,
    pub mainnet_franklin_contract_address: String,
    pub rinkeby_franklin_contract_address: String,
}

impl RuntimeConfig {
    pub fn new() -> Self {
        let transfer_batch_size_env =
            env::var("TRANSFER_BATCH_SIZE").unwrap_or("8".to_string());
        let transfer_size = usize::from_str_radix(&(transfer_batch_size_env), 10)
            .expect("TRANSFER_BATCH_SIZE invalid");
        let keys_path = env::var("KEY_DIR")
            .ok()
            .unwrap_or_else(|| DEFAULT_KEYS_PATH.to_string());

        Self {
            transfer_batch_size: transfer_size,
            keys_path,
            contract_addr: env::var("CONTRACT_ADDR").unwrap_or("5F939954eA54FA9b61Fd59518945D09E8939f2B2".to_string()),
            max_outstanding_txs: 120000,
            mainnet_http_endpoint_string: env::var("TREE_RESTORE_MAINNET_ENDPOINT")
                .unwrap_or("https://mainnet.infura.io/".to_string()),
            rinkeby_http_endpoint_string: env::var("TREE_RESTORE_RINKEBY_ENDPOINT")
                .unwrap_or("https://rinkeby.infura.io/".to_string()),
            mainnet_franklin_contract_address: env::var("TREE_RESTORE_MAINNET_CONTRACT_ADDR")
                .unwrap_or("4a89f998dce2453e96b795d47603c4b5a16144b0".to_string()),
            rinkeby_franklin_contract_address: env::var("TREE_RESTORE_RINKEBY_CONTRACT_ADDR")
                .unwrap_or("4fbf331db438c88a83b1316d072b7d73d8366367".to_string()),
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self::new()
    }
}
