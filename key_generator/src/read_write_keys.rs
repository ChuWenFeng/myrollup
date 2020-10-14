use time::PreciseTime;

use circuit::leaf::LeafWitness;
use circuit::transfer::circuit::{TransactionWitness, Transfer};
use circuit::transfer::transaction::Transaction;
use circuit::CircuitAccountTree;
use crypto::digest::Digest;
use crypto::sha2::Sha256;
use ff::{BitIterator, Field, PrimeField, PrimeFieldRepr};
use hex::encode;
use pairing::bn256::*;
use pairing::Engine;
use rand::{Rng, SeedableRng, XorShiftRng};
use sapling_crypto::alt_babyjubjub::AltJubjubBn256;
use sapling_crypto::circuit::test::*;
use std::collections::HashMap;

use crate::vk_contract_generator::hardcode_vk;
use bellman::groth16::{
    create_random_proof, generate_random_parameters, prepare_verifying_key, verify_proof,
    VerifyingKey,
};
use bellman::Circuit;

use sapling_crypto::circuit::float_point::convert_to_float;
use sapling_crypto::eddsa::{PrivateKey, PublicKey};
use sapling_crypto::jubjub::FixedGenerators;

use models::plasma::circuit::account::CircuitAccount;
use models::plasma::circuit::utils::{be_bit_vector_into_bytes, le_bit_vector_into_field_element};
use models::plasma::params as plasma_constants;

const TXES_TO_TEST: usize = 128;

#[allow(clippy::cognitive_complexity)]
pub fn read_write_keys() {
    let p_g = FixedGenerators::SpendingKeyGenerator;
    let params = &AltJubjubBn256::new();
    let rng = &mut XorShiftRng::from_seed([0x3dbe_6258, 0x8d31_3d76, 0x3237_db17, 0xe5bc_0654]);
    let tree_depth = plasma_constants::BALANCE_TREE_DEPTH as u32;

    let capacity: u32 = 1 << tree_depth;

    let mut existing_accounts: Vec<(u32, PrivateKey<Bn256>, PublicKey<Bn256>)> = vec![];

    let mut tree = CircuitAccountTree::new(tree_depth);

    let number_of_accounts = 1000;

    let mut existing_account_hm = HashMap::<u32, bool>::new();

    let default_balance_string = "1000000";
    let transfer_amount: u128 = 1000;
    let fee_amount: u128 = 0;

    for _ in 0..number_of_accounts {
        let mut leaf_number: u32 = rng.gen();
        leaf_number %= capacity;
        if existing_account_hm.get(&leaf_number).is_some() {
            continue;
        } else {
            existing_account_hm.insert(leaf_number, true);
        }

        let sk = PrivateKey::<Bn256>(rng.gen());
        let pk = PublicKey::from_private(&sk, p_g, params);
        let (x, y) = pk.0.into_xy();

        existing_accounts.push((leaf_number, sk, pk));

        let leaf = CircuitAccount {
            balance: Fr::from_str(default_balance_string).unwrap(),
            nonce: Fr::zero(),
            pub_x: x,
            pub_y: y,
        };

        tree.insert(leaf_number, leaf.clone());
    }

    let num_accounts = existing_accounts.len();

    debug!("Inserted {} accounts", num_accounts);

    let initial_root = tree.root_hash();

    let mut witnesses: Vec<(Transaction<Bn256>, TransactionWitness<Bn256>)> = vec![];
    let mut public_data_vector: Vec<bool> = vec![];

    let transfer_amount_as_field_element = Fr::from_str(&transfer_amount.to_string()).unwrap();

    let transfer_amount_bits = convert_to_float(
        transfer_amount,
        plasma_constants::AMOUNT_EXPONENT_BIT_WIDTH,
        plasma_constants::AMOUNT_MANTISSA_BIT_WIDTH,
        10,
    )
    .unwrap();

    let transfer_amount_encoded: Fr = le_bit_vector_into_field_element(&transfer_amount_bits);

    let fee_as_field_element = Fr::from_str(&fee_amount.to_string()).unwrap();

    let fee_bits = convert_to_float(
        fee_amount,
        plasma_constants::FEE_EXPONENT_BIT_WIDTH,
        plasma_constants::FEE_MANTISSA_BIT_WIDTH,
        10,
    )
    .unwrap();

    let fee_encoded: Fr = le_bit_vector_into_field_element(&fee_bits);

    let mut total_fees = Fr::zero();

    for _ in 0..TXES_TO_TEST {
        let mut sender_account_number: usize = rng.gen();
        sender_account_number %= num_accounts;
        let sender_account_info: &(u32, PrivateKey<Bn256>, PublicKey<Bn256>) =
            existing_accounts.get(sender_account_number).unwrap();

        let mut recipient_account_number: usize = rng.gen();
        recipient_account_number %= num_accounts;
        if recipient_account_number == sender_account_number {
            recipient_account_number += 1 % num_accounts;
        }
        let recipient_account_info: &(u32, PrivateKey<Bn256>, PublicKey<Bn256>) =
            existing_accounts.get(recipient_account_number).unwrap();

        let sender_leaf_number = sender_account_info.0;
        let recipient_leaf_number = recipient_account_info.0;

        let items = tree.items.clone();

        let sender_leaf = items.get(&sender_leaf_number).unwrap().clone();
        let recipient_leaf = items.get(&recipient_leaf_number).unwrap().clone();

        let path_from: Vec<Option<Fr>> = tree
            .merkle_path(sender_leaf_number)
            .into_iter()
            .map(|e| Some(e.0))
            .collect();
        let path_to: Vec<Option<Fr>> = tree
            .merkle_path(recipient_leaf_number)
            .into_iter()
            .map(|e| Some(e.0))
            .collect();

        // debug!("Making a transfer from {} to {}", sender_leaf_number, recipient_leaf_number);

        let from = Fr::from_str(&sender_leaf_number.to_string());
        let to = Fr::from_str(&recipient_leaf_number.to_string());

        let mut transaction: Transaction<Bn256> = Transaction {
            from,
            to,
            amount: Some(transfer_amount_encoded),
            fee: Some(fee_encoded),
            nonce: Some(sender_leaf.nonce),
            good_until_block: Some(Fr::one()),
            signature: None,
        };

        let sender_sk = &sender_account_info.1;

        transaction.sign(&sender_sk, p_g, params, rng);

        assert!(transaction.signature.is_some());

        //assert!(tree.verify_proof(sender_leaf_number, sender_leaf.clone(), tree.merkle_path(sender_leaf_number)));
        //assert!(tree.verify_proof(recipient_leaf_number, recipient_leaf.clone(), tree.merkle_path(recipient_leaf_number)));

        // debug!("Sender: balance: {}, nonce: {}, pub_x: {}, pub_y: {}", sender_leaf.balance, sender_leaf.nonce, sender_leaf.pub_x, sender_leaf.pub_y);
        // debug!("Recipient: balance: {}, nonce: {}, pub_x: {}, pub_y: {}", recipient_leaf.balance, recipient_leaf.nonce, recipient_leaf.pub_x, recipient_leaf.pub_y);

        let mut updated_sender_leaf = sender_leaf.clone();
        let mut updated_recipient_leaf = recipient_leaf.clone();

        updated_sender_leaf
            .balance
            .sub_assign(&transfer_amount_as_field_element);
        updated_sender_leaf
            .balance
            .sub_assign(&fee_as_field_element);

        updated_sender_leaf.nonce.add_assign(&Fr::one());

        updated_recipient_leaf
            .balance
            .add_assign(&transfer_amount_as_field_element);

        total_fees.add_assign(&fee_as_field_element);

        // debug!("Updated sender: balance: {}, nonce: {}, pub_x: {}, pub_y: {}", updated_sender_leaf.balance, updated_sender_leaf.nonce, updated_sender_leaf.pub_x, updated_sender_leaf.pub_y);
        // debug!("Updated recipient: balance: {}, nonce: {}, pub_x: {}, pub_y: {}", updated_recipient_leaf.balance, updated_recipient_leaf.nonce, updated_recipient_leaf.pub_x, updated_recipient_leaf.pub_y);

        tree.insert(sender_leaf_number, updated_sender_leaf.clone());
        tree.insert(recipient_leaf_number, updated_recipient_leaf.clone());

        //assert!(tree.verify_proof(sender_leaf_number, updated_sender_leaf.clone(), tree.merkle_path(sender_leaf_number)));
        //assert!(tree.verify_proof(recipient_leaf_number, updated_recipient_leaf.clone(), tree.merkle_path(recipient_leaf_number)));

        let public_data = transaction.public_data_into_bits();
        public_data_vector.extend(public_data.into_iter());

        let leaf_witness_from = LeafWitness {
            balance: Some(sender_leaf.balance),
            nonce: Some(sender_leaf.nonce),
            pub_x: Some(sender_leaf.pub_x),
            pub_y: Some(sender_leaf.pub_y),
        };

        let leaf_witness_to = LeafWitness {
            balance: Some(recipient_leaf.balance),
            nonce: Some(recipient_leaf.nonce),
            pub_x: Some(recipient_leaf.pub_x),
            pub_y: Some(recipient_leaf.pub_y),
        };

        let transaction_witness = TransactionWitness {
            leaf_from: leaf_witness_from,
            auth_path_from: path_from,
            leaf_to: leaf_witness_to,
            auth_path_to: path_to,
        };

        let witness = (transaction.clone(), transaction_witness);

        witnesses.push(witness);
    }

    let block_number = Fr::one();

    debug!("Block number = {}", block_number.into_repr());

    let final_root = tree.root_hash();

    let final_root_string = format!(
        "{}",
        CircuitAccountTree::new(tree_depth).root_hash().into_repr()
    );

    debug!("Final root = {}", final_root_string);

    let mut public_data_initial_bits = vec![];

    // these two are BE encodings because an iterator is BE. This is also an Ethereum standard behavior

    let block_number_bits: Vec<bool> = BitIterator::new(block_number.into_repr()).collect();
    for _ in 0..256 - block_number_bits.len() {
        public_data_initial_bits.push(false);
    }
    public_data_initial_bits.extend(block_number_bits.into_iter());

    let total_fee_bits: Vec<bool> = BitIterator::new(total_fees.into_repr()).collect();
    for _ in 0..256 - total_fee_bits.len() {
        public_data_initial_bits.push(false);
    }
    public_data_initial_bits.extend(total_fee_bits.into_iter());

    assert_eq!(public_data_initial_bits.len(), 512);

    let mut h = Sha256::new();

    let bytes_to_hash = be_bit_vector_into_bytes(&public_data_initial_bits);

    h.input(&bytes_to_hash);

    let mut hash_result = [0u8; 32];
    h.result(&mut hash_result[..]);

    {
        let packed_transaction_data_bytes = be_bit_vector_into_bytes(&public_data_vector);

        let mut next_round_hash_bytes = vec![];
        next_round_hash_bytes.extend(hash_result.iter());
        next_round_hash_bytes.extend(packed_transaction_data_bytes.clone());

        debug!("Public data = {}", encode(packed_transaction_data_bytes));

        let mut h = Sha256::new();

        h.input(&next_round_hash_bytes);

        // let mut hash_result = [0u8; 32];
        h.result(&mut hash_result[..]);
    }

    // clip to fit into field element

    hash_result[0] &= 0x1f; // temporary solution

    let mut repr = Fr::zero().into_repr();
    repr.read_be(&hash_result[..])
        .expect("pack hash as field element");

    let public_data_commitment = Fr::from_repr(repr).unwrap();

    debug!("Total fees = {}", total_fees.into_repr());

    debug!(
        "Final data commitment as field element = {}",
        public_data_commitment
    );

    let instance_for_test_cs = Transfer {
        params,
        number_of_transactions: TXES_TO_TEST,
        old_root: Some(initial_root),
        new_root: Some(final_root),
        public_data_commitment: Some(public_data_commitment),
        block_number: Some(Fr::one()),
        total_fee: Some(total_fees),
        transactions: witnesses.clone(),
    };

    {
        let mut cs = TestConstraintSystem::new();

        instance_for_test_cs.synthesize(&mut cs).unwrap();

        debug!("Total of {} constraints", cs.num_constraints());
        debug!(
            "{} constraints per TX for {} transactions",
            cs.num_constraints() / TXES_TO_TEST,
            TXES_TO_TEST
        );

        assert_eq!(cs.num_inputs(), 4);

        assert_eq!(cs.get_input(0, "ONE"), Fr::one());
        assert_eq!(
            cs.get_input(1, "old root input/input variable"),
            initial_root
        );
        assert_eq!(cs.get_input(2, "new root input/input variable"), final_root);
        assert_eq!(
            cs.get_input(3, "rolling hash input/input variable"),
            public_data_commitment
        );

        let err = cs.which_is_unsatisfied();
        if err.is_some() {
            panic!("ERROR satisfying in {}\n", err.unwrap());
        } else {
            debug!("Test constraint system is satisfied");
        }
    }

    let empty_transaction = Transaction {
        from: None,
        to: None,
        amount: None,
        fee: None,
        nonce: None,
        good_until_block: None,
        signature: None,
    };

    let empty_leaf_witness = LeafWitness {
        balance: None,
        nonce: None,
        pub_x: None,
        pub_y: None,
    };

    let empty_witness = TransactionWitness {
        leaf_from: empty_leaf_witness.clone(),
        auth_path_from: vec![None; plasma_constants::BALANCE_TREE_DEPTH],
        leaf_to: empty_leaf_witness,
        auth_path_to: vec![None; plasma_constants::BALANCE_TREE_DEPTH],
    };

    let instance_for_generation: Transfer<'_, Bn256> = Transfer {
        params,
        number_of_transactions: TXES_TO_TEST,
        old_root: None,
        new_root: None,
        public_data_commitment: None,
        block_number: None,
        total_fee: None,
        transactions: vec![(empty_transaction, empty_witness); TXES_TO_TEST],
    };

    debug!("generating setup...");
    let start = PreciseTime::now();
    let tmp_cirtuit_params = generate_random_parameters(instance_for_generation, rng).unwrap();
    debug!(
        "setup generated in {} s",
        start.to(PreciseTime::now()).num_milliseconds() as f64 / 1000.0
    );

    use std::fs::File;
    use std::io::{BufWriter, Write};
    {
        let f = File::create("pk.key").expect("Unable to create file");
        let mut f = BufWriter::new(f);
        tmp_cirtuit_params
            .write(&mut f)
            .expect("Unable to write proving key");
    }

    use std::io::BufReader;

    let f_r = File::open("pk.key").expect("Unable to open file");
    let mut r = BufReader::new(f_r);
    let circuit_params =
        bellman::groth16::Parameters::read(&mut r, true).expect("Unable to read proving key");

    let initial_root_string = format!(
        "{}",
        CircuitAccountTree::new(tree_depth).root_hash().into_repr()
    );
    let contract_content =
        generate_vk_contract(&circuit_params.vk, initial_root_string.as_ref(), tree_depth);

    let f_cont = File::create("VerificationKeys.sol").expect("Unable to create file");
    let mut f_cont = BufWriter::new(f_cont);
    f_cont
        .write_all(contract_content.as_bytes())
        .expect("Unable to write contract");

    let pvk = prepare_verifying_key(&circuit_params.vk);

    let instance_for_proof = Transfer {
        params,
        number_of_transactions: TXES_TO_TEST,
        old_root: Some(initial_root),
        new_root: Some(final_root),
        public_data_commitment: Some(public_data_commitment),
        block_number: Some(Fr::one()),
        total_fee: Some(total_fees),
        transactions: witnesses,
    };

    debug!("creating proof...");
    let start = PreciseTime::now();
    let proof = create_random_proof(instance_for_proof, &circuit_params, rng).unwrap();
    debug!(
        "proof created in {} s",
        start.to(PreciseTime::now()).num_milliseconds() as f64 / 1000.0
    );

    let success = verify_proof(
        &pvk,
        &proof,
        &[initial_root, final_root, public_data_commitment],
    )
    .unwrap();
    assert!(success);
}

fn generate_vk_contract<E: Engine>(
    vk: &VerifyingKey<E>,
    initial_root: &str,
    tree_depth: u32,
) -> String {
    format!(
        r#"
// This contract is generated programmatically

pragma solidity ^0.4.24;


// Hardcoded constants to avoid accessing store
contract VerificationKeys {{

    // For tree depth {tree_depth}
    bytes32 constant EMPTY_TREE_ROOT = {initial_root};

    function getVkUpdateCircuit() internal pure returns (uint256[14] memory vk, uint256[] memory gammaABC) {{

        {vk}

    }}

}}
"#,
        vk = hardcode_vk(&vk),
        initial_root = initial_root,
        tree_depth = tree_depth,
    )
}
