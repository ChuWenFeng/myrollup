use pairing::bn256::Bn256;
// use sapling_crypto::jubjub::{FixedGenerators};
// use sapling_crypto::alt_babyjubjub::{AltJubjubBn256};

use models::plasma::account::Account;
use models::plasma::block::{Block, BlockData};
use models::plasma::tx::{DepositTx, ExitTx, TransferTx};
use models::plasma::{AccountId, AccountMap, BatchNumber};
use plasma::state::PlasmaState;
use rayon::prelude::*;
use sapling_crypto::eddsa::PrivateKey;
use std::collections::VecDeque;
use web3::types::H256;

use models::config;

use models::{
    CommitRequest, NetworkStatus, ProtoBlock, StateKeeperRequest, TransferTxConfirmation,
    TransferTxResult,
};

use storage::ConnectionPool;

use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive, Zero};
use fnv::FnvHashMap;
use std::sync::mpsc::{Receiver, Sender};

use std::io::BufReader;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct PlasmaStateKeeper {
    /// Current plasma state
    pub state: PlasmaState,

    /// Queue for blocks to be processed next
    /// 排队等待接下来要处理的块
    block_queue: VecDeque<ProtoBlock>,

    /// Queue for transfer transactions
    /// 传输事务队列
    transfer_tx_queue: Vec<TransferTx>,

    /// Promised latest UNIX timestamp of the next block
    /// 承诺的下一个块的最新UNIX时间戳
    next_block_at_max: Option<SystemTime>,
}

#[allow(dead_code)]
type RootHash = H256;
#[allow(dead_code)]
type UpdatedAccounts = AccountMap;

impl PlasmaStateKeeper {
    pub fn new(pool: ConnectionPool) -> Self {
        info!("constructing state keeper instance");
        // here we should insert default accounts into the tree
        let storage = pool
            .access_storage()
            .expect("db connection failed for statekeeper");

        //读取数据库中的账户信息并初始化AccountTree.
        let (last_committed, accounts) = storage.load_committed_state().expect("db failed");

        
        
        let last_verified = storage.get_last_verified_block().expect("db failed");
        let state = PlasmaState::new(accounts, last_committed + 1);
        //let outstanding_txs = storage.count_outstanding_proofs(last_verified).expect("db failed");

        info!(
            "last_committed = {}, last_verified = {}",
            last_committed, last_verified
        );

        // Keeper starts with the NEXT block
        let keeper = PlasmaStateKeeper {
            state,
            block_queue: VecDeque::default(),
            transfer_tx_queue: Vec::default(),
            next_block_at_max: None,
        };

        let root = keeper.state.root_hash();
        info!("created state keeper, root hash = {}", root);

        keeper
    }

    fn run(
        &mut self,
        rx_for_blocks: Receiver<StateKeeperRequest>,
        tx_for_commitments: Sender<CommitRequest>,
    ) {
        for req in rx_for_blocks {
            match req {
                StateKeeperRequest::GetNetworkStatus(sender) => {
                    let r = sender.send(NetworkStatus {
                        next_block_at_max: self
                            .next_block_at_max
                            .map(|t| t.duration_since(UNIX_EPOCH).unwrap().as_secs()),
                        last_committed: 0,
                        last_verified: 0,
                        outstanding_txs: 0,
                        total_transactions: 0,
                    });
                    if r.is_err() {
                        error!(
                            "StateKeeperRequest::GetNetworkStatus: channel closed, sending failed"
                        );
                    }
                }
                StateKeeperRequest::GetAccount(account_id, sender) => {
                    let account = self.state.get_account(account_id);
                    let r = sender.send(account);
                    if r.is_err() {
                        error!("StateKeeperRequest::GetAccount: channel closed, sending failed");
                    }
                }
                StateKeeperRequest::AddTransferTx(tx, sender) => {
                    let result = self.apply_transfer_tx(*tx);
                    if result.is_ok() && self.next_block_at_max.is_none() {
                        self.next_block_at_max =
                            Some(SystemTime::now() + Duration::from_secs(config::PADDING_INTERVAL));
                    }
                    let r = sender.send(result);
                    if r.is_err() {
                        error!("StateKeeperRequest::AddTransferTx: channel closed, sending failed");
                    }

                    if self.transfer_tx_queue.len() == config::RUNTIME_CONFIG.transfer_batch_size {
                        self.finalize_current_batch(&tx_for_commitments);
                    }
                }
                StateKeeperRequest::AddBlock(block) => {
                    self.block_queue.push_back(block);
                    //debug!("new protoblock, transfer_tx_queue.len() = {}", self.transfer_tx_queue.len());
                    if self.transfer_tx_queue.is_empty() {
                        self.process_block_queue(&tx_for_commitments);
                    }
                }
                StateKeeperRequest::TimerTick => {
                    if let Some(next_block_at) = self.next_block_at_max {
                        if next_block_at <= SystemTime::now() {
                            self.finalize_current_batch(&tx_for_commitments);
                        }
                    }
                }
            }
        }
    }

    fn process_block_queue(&mut self, tx_for_commitments: &Sender<CommitRequest>) {
        let blocks = std::mem::replace(&mut self.block_queue, VecDeque::default());
        for block in blocks.into_iter() {
            let req = match block {
                ProtoBlock::Transfer => self.create_transfer_block(),
                ProtoBlock::Deposit(batch_number, transactions) => {
                    self.create_deposit_block(batch_number, transactions)
                }
                ProtoBlock::Exit(batch_number, transactions) => {
                    self.create_exit_block(batch_number, transactions)
                }
            };
            //debug!("sending request to committer {:?}", req);
            tx_for_commitments
                 .send(req)
                 .expect("must send new operation for commitment");
            println!("-------------------------------------------------------------------------------------------------------------------------------");
            println!(
                "block_number: {}\ntree_root_hash: {}\n",
                self.state.block_number,
                self.state.root_hash()
            );
            println!("Account :{:?}", self.state.balance_tree.items);
            self.state.block_number += 1; // bump current block number as we've made one
        }
    }

    fn apply_transfer_tx(&mut self, tx: TransferTx) -> TransferTxResult {
        let appication_result = self.state.apply_transfer(&tx);
        if appication_result.is_ok() {
            //debug!("accepted transaction for account {}, nonce {}", tx.from, tx.nonce);
            self.transfer_tx_queue.push(tx);
        }

        // TODO: sign confirmation
        appication_result.map(|_| TransferTxConfirmation {
            block_number: self.state.block_number,
            signature: "0x133sig".to_owned(),
        })
    }

    fn finalize_current_batch(&mut self, tx_for_commitments: &Sender<CommitRequest>) {
        self.apply_padding();
        self.block_queue.push_front(ProtoBlock::Transfer);
        self.process_block_queue(&tx_for_commitments);
        self.next_block_at_max = None;
    }

    fn apply_padding(&mut self) {
        let to_pad = config::RUNTIME_CONFIG.transfer_batch_size - self.transfer_tx_queue.len();
        if to_pad > 0 {
            debug!("padding transactions");
            // TODO: move to env vars
            let pk_bytes =
                hex::decode("8ea0225bbf7f3689eb8ba6f8d7bef3d8ae2541573d71711a28d5149807b40805")// 8ea0225bbf7f3689eb8ba6f8d7bef3d8ae2541573d71711a28d5149807b40805
                    .unwrap();
            let private_key: PrivateKey<Bn256> =
                PrivateKey::read(BufReader::new(pk_bytes.as_slice())).unwrap();
 
            // use sapling_crypto::alt_babyjubjub::fs::Fs;
            // use sapling_crypto::jubjub::ToUniform;
            // let pk_bytes =
            //     hex::decode("058452a6b4f8fc2090c55708bae4ee01dc2195294a9dcea2e81030501d66ce2f")// 8ea0225bbf7f3689eb8ba6f8d7bef3d8ae2541573d71711a28d5149807b40805
            //         .unwrap();
            // let rr = Fs::to_uniform_32(&pk_bytes);
            // let mypk:PrivateKey<Bn256> = PrivateKey::<Bn256>(rr);
            
            let padding_account_id = 2; // TODO: 1
            let base_nonce = self.account(padding_account_id).nonce;

            let prepared_transactions: Vec<TransferTx> = (0..(to_pad as u32))
                .into_par_iter()
                .map(|i| {
                    let nonce = base_nonce + i;
                    let tx = TransferTx::create_signed_tx(
                        padding_account_id, // from
                        0,                  // to
                        BigDecimal::zero(), // amount
                        BigDecimal::zero(), // fee
                        nonce,              // nonce
                        2_147_483_647,      // good until max_block
                        &private_key,
                    );
                    //assert!(tx.verify_sig(&pub_key));

                    tx
                })
                .collect();

                

            for tx in prepared_transactions.into_iter() {

                let pub_key = self.state
                .get_account(tx.from)
                .and_then(|a| a.get_pub_key())
                .ok_or("err");

                // 验证签名
                // use models::plasma::PublicKey;
                // let mut pub_key2: Option<PublicKey> = None;
                // if let Some(cache_pub_key) = tx.cached_pub_key.clone(){
                //     pub_key2 = Some(cache_pub_key);
                // }
                // let (x,y) = pub_key2.unwrap().0.into_xy();
                // println!("{:?}", x);
                // println!("{:?}", y);
                if !pub_key.is_err(){
                    let pk = pub_key.unwrap();
                    let verified = tx.verify_sig(&pk);
                    if !verified {
                        let (x, y) = pk.0.into_xy();
                        warn!("Got public key: {:?}, {:?}", x, y);
                        warn!(
                            "Signature is invalid: (x,y,s) = ({:?},{:?},{:?})",
                            &tx.signature.r_x, &tx.signature.r_y, &tx.signature.s
                        );
                        //return;
                        panic!("apply padding transfer sign verify err");
                    }
                }
                
                self.state
                    .apply_transfer(&tx)
                    .expect("padding must always be applied correctly");
                self.transfer_tx_queue.push(tx);
            }

            // for i in 0..to_pad {
            //     let nonce = self.account(padding_account_id).nonce;
            //     let tx = TransferTx::create_signed_tx(
            //         padding_account_id, // from
            //         0,                  // to
            //         BigDecimal::zero(), // amount
            //         BigDecimal::zero(), // fee
            //         nonce,              // nonce
            //         2147483647,         // good until max_block
            //         &private_key
            //     );

            //     let pub_key = self.state.get_account(padding_account_id).and_then(|a| a.get_pub_key()).expect("public key must exist for padding account");
            //     assert!( tx.verify_sig(&pub_key) );

            //     self.state.apply_transfer(&tx).expect("padding must always be applied correctly");
            //     self.transfer_tx_queue.push(tx);
            // }
        }
    }

    fn create_transfer_block(&mut self) -> CommitRequest {
        let transactions = std::mem::replace(&mut self.transfer_tx_queue, Vec::default());
        let total_fees: u128 = transactions
            .iter()
            .map(|tx| tx.fee.to_u128().expect("should not overflow"))
            .sum();

        let total_fees = BigDecimal::from_u128(total_fees).unwrap();

        // collect updated state
        let mut accounts_updated = FnvHashMap::<u32, Account>::default();
        for tx in transactions.iter() {
            accounts_updated.insert(tx.from, self.account(tx.from));
            accounts_updated.insert(tx.to, self.account(tx.to));
        }

        let block = Block {
            block_number: self.state.block_number,
            new_root_hash: self.state.root_hash(),
            block_data: BlockData::Transfer {
                total_fees,
                transactions,
            },
        };

        CommitRequest {
            block,
            accounts_updated,
        }
    }

    fn create_deposit_block(
        &mut self,
        batch_number: BatchNumber,
        transactions: Vec<DepositTx>,
    ) -> CommitRequest {
        let transactions = Self::sort_deposit_block(transactions);
        let mut accounts_updated = FnvHashMap::<u32, Account>::default();
        for tx in transactions.iter() {
            self.state
                .apply_deposit(&tx) //在merkle树上更新tx中的账户余额(self.state.balance_tree)
                .expect("must apply deposit transaction");

            // collect updated state
            accounts_updated.insert(tx.account, self.account(tx.account));
        }

        let block = Block {
            block_number: self.state.block_number,
            new_root_hash: self.state.root_hash(),
            block_data: BlockData::Deposit {
                batch_number,
                transactions,
            },
        };

        CommitRequest {
            block,
            accounts_updated,
        }
    }

    // prover MUST read old balances and mutate the block data
    //验证者必须读取旧的平衡并改变块数据
    fn create_exit_block(
        &mut self,
        batch_number: BatchNumber,
        transactions: Vec<ExitTx>,
    ) -> CommitRequest {
        let transactions = Self::sort_exit_block(transactions);
        let mut accounts_updated = FnvHashMap::<u32, Account>::default();
        let mut augmented_txes = vec![];
        for tx in transactions.iter() {
            let augmented_tx = self
                .state
                .apply_exit(&tx)
                .expect("must augment exit transaction information");
            augmented_txes.push(augmented_tx);
            // collect updated state
            accounts_updated.insert(tx.account, self.account(tx.account));
        }

        let block = Block {
            block_number: self.state.block_number,
            new_root_hash: self.state.root_hash(),
            block_data: BlockData::Exit {
                batch_number,
                transactions: augmented_txes,
            },
        };

        CommitRequest {
            block,
            accounts_updated,
        }
    }

    // sorting is required to ensure that all accounts affected are unique, see the smart contract
    fn sort_deposit_block(mut txes: Vec<DepositTx>) -> Vec<DepositTx> {
        txes.sort_by_key(|l| l.account);
        txes
    }

    // sorting is required to ensure that all accounts affected are unique, see the smart contract
    fn sort_exit_block(mut txes: Vec<ExitTx>) -> Vec<ExitTx> {
        txes.sort_by_key(|l| l.account);
        txes
    }

    //根据account_id获取account
    fn account(&self, account_id: AccountId) -> Account {
        self.state.get_account(account_id).unwrap_or_default()
    }
}

pub fn start_state_keeper(
    mut sk: PlasmaStateKeeper,
    rx_for_blocks: Receiver<StateKeeperRequest>,
    tx_for_commitments: Sender<CommitRequest>,
) {
    std::thread::Builder::new()
        .name("state_keeper".to_string())
        .spawn(move || sk.run(rx_for_blocks, tx_for_commitments))
        .expect("State keeper thread");
}
