use crate::deposit::deposit_request::DepositRequest;
use crate::leaf::{make_leaf_content, LeafWitness};
use bellman::{Circuit, ConstraintSystem, SynthesisError};
use ff::{Field, PrimeField};
use models::plasma::circuit::utils::{allocate_audit_path, append_packed_public_key};
use models::plasma::params as plasma_constants;
use sapling_crypto::circuit::num::{AllocatedNum, Num};
use sapling_crypto::circuit::{boolean, ecc, num, pedersen_hash, sha256, Assignment};
use sapling_crypto::jubjub::JubjubEngine;

#[derive(Clone)]
pub struct DepositWitness<E: JubjubEngine> {
    pub leaf: LeafWitness<E>,
    /// The authentication path of the leaf to deposit into the tree
    /// Path is not used as it's determined by "into" field in deposit request itself
    pub auth_path: Vec<Option<E::Fr>>,
    // We inflate a witness to avoid calculation inside the SNARK.
    // First we use boolean to show that leaf is empty or not (for deposits into existing account)
    pub leaf_is_empty: Option<bool>,
    // We also witness new public keys, and internally constraint whether we swap those or not
    pub new_pub_x: Option<E::Fr>,
    pub new_pub_y: Option<E::Fr>,
}

/// This is an instance of the `Spend` circuit.
pub struct Deposit<'a, E: JubjubEngine> {
    pub params: &'a E::Params,

    // number of deposits per block
    pub number_of_deposits: usize,

    /// The old root of the tree
    pub old_root: Option<E::Fr>,

    /// The new root of the tree
    pub new_root: Option<E::Fr>,

    /// Final truncated rolling SHA256
    pub public_data_commitment: Option<E::Fr>,

    /// Block number
    pub block_number: Option<E::Fr>,

    /// Requests for this block
    pub requests: Vec<(DepositRequest<E>, DepositWitness<E>)>,
}

impl<'a, E: JubjubEngine> Circuit<E> for Deposit<'a, E> {
    fn synthesize<CS: ConstraintSystem<E>>(self, cs: &mut CS) -> Result<(), SynthesisError> {
        // Check that transactions are in a right quantity
        assert!(self.number_of_deposits == self.requests.len());

        let old_root_value = self.old_root;
        // Expose inputs and do the bits decomposition of hash
        let mut old_root =
            AllocatedNum::alloc(cs.namespace(|| "old root"), || Ok(*old_root_value.get()?))?;
        old_root.inputize(cs.namespace(|| "old root input"))?;

        let new_root_value = self.new_root;
        let new_root =
            AllocatedNum::alloc(cs.namespace(|| "new root"), || Ok(*new_root_value.get()?))?;
        new_root.inputize(cs.namespace(|| "new root input"))?;

        let rolling_hash_value = self.public_data_commitment;
        let rolling_hash = AllocatedNum::alloc(cs.namespace(|| "rolling hash"), || {
            Ok(*rolling_hash_value.get()?)
        })?;
        rolling_hash.inputize(cs.namespace(|| "rolling hash input"))?;

        let mut public_data_vector: Vec<boolean::Boolean> = vec![];

        // Ok, now we need to update the old root by applying requests in sequence
        let requests = self.requests.clone();

        for (i, tx) in requests.into_iter().enumerate() {
            let (request, witness) = tx;
            let (intermediate_root, public_data) = apply_request(
                cs.namespace(|| format!("applying transaction {}", i)),
                old_root,
                request,
                witness,
                self.params,
            )?;
            old_root = intermediate_root;
            // flatten the public transaction data
            public_data_vector.extend(public_data.into_iter());
        }

        // constraint the new hash to be equal to updated hash

        cs.enforce(
            || "enforce new root equal to recalculated one",
            |lc| lc + new_root.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + old_root.get_variable(),
        );

        // Inside the circuit with work with LE bit order,
        // so an account number "1" that would have a natural representation of e.g. 0x000001
        // will have a bit decomposition [1, 0, 0, 0, ......]

        // Don't deal with it here, but rather do on application layer when parsing the data!
        // The only requirement is to properly seed initial hash value with block number and fees,
        // as those are going to be naturally represented as Ethereum units

        // Now it's time to pack the initial SHA256 hash due to Ethereum BE encoding
        // and start rolling the hash

        let mut initial_hash_data: Vec<boolean::Boolean> = vec![];

        let block_number_allocated =
            AllocatedNum::alloc(cs.namespace(|| "allocate block number"), || {
                Ok(*self.block_number.get()?)
            })?;

        // make initial hash as sha256(uint256(block_number))
        let mut block_number_bits = block_number_allocated
            .into_bits_le(cs.namespace(|| "unpack block number for hashing"))?;

        block_number_bits.resize(
            plasma_constants::FR_BIT_WIDTH,
            boolean::Boolean::Constant(false),
        );
        block_number_bits.reverse();
        initial_hash_data.extend(block_number_bits.into_iter());

        assert_eq!(initial_hash_data.len(), 256);

        let mut hash_block = sha256::sha256(
            cs.namespace(|| "initial rolling sha256"),
            &initial_hash_data,
        )?;

        // now pack the public data and do the final hash

        let mut pack_bits = vec![];
        pack_bits.extend(hash_block);
        pack_bits.extend(public_data_vector.into_iter());

        hash_block = sha256::sha256(cs.namespace(|| "hash public data"), &pack_bits)?;

        // // now pack and enforce equality to the input

        hash_block.reverse();
        hash_block.truncate(E::Fr::CAPACITY as usize);

        let mut packed_hash_lc = Num::<E>::zero();
        let mut coeff = E::Fr::one();
        for bit in hash_block {
            packed_hash_lc = packed_hash_lc.add_bool_with_coeff(CS::one(), &bit, coeff);
            coeff.double();
        }

        cs.enforce(
            || "enforce external data hash equality",
            |lc| lc + rolling_hash.get_variable(),
            |lc| lc + CS::one(),
            |_| packed_hash_lc.lc(E::Fr::one()),
        );

        Ok(())
    }
}

/// Applies one request to the tree,
/// outputs a new root
fn apply_request<E, CS>(
    mut cs: CS,
    old_root: AllocatedNum<E>,
    request: DepositRequest<E>,
    witness: DepositWitness<E>,
    params: &E::Params,
) -> Result<(AllocatedNum<E>, Vec<boolean::Boolean>), SynthesisError>
where
    E: JubjubEngine,
    CS: ConstraintSystem<E>,
{
    // Calculate leaf value commitment

    let leaf = make_leaf_content(cs.namespace(|| "create leaf"), witness.clone().leaf)?;

    // Compute the hash of the from leaf
    let mut leaf_hash = pedersen_hash::pedersen_hash(
        cs.namespace(|| "leaf content hash"),
        pedersen_hash::Personalization::NoteCommitment,
        &leaf.leaf_bits,
        params,
    )?;

    // Constraint that "into" field in transaction is
    // equal to the merkle proof path

    let address_allocated = AllocatedNum::alloc(cs.namespace(|| "deposit into address"), || {
        Ok(*request.into.get()?)
    })?;

    let mut path_bits =
        address_allocated.into_bits_le(cs.namespace(|| "into address bit decomposition"))?;

    path_bits.truncate(plasma_constants::BALANCE_TREE_DEPTH);

    let audit_path = allocate_audit_path(
        cs.namespace(|| "allocate audit path"),
        witness.clone().auth_path,
    )?;

    {
        // This is an injective encoding, as cur is a
        // point in the prime order subgroup.
        let mut cur = leaf_hash.get_x().clone();

        // Ascend the merkle tree authentication path
        for (i, direction_bit) in path_bits.clone().into_iter().enumerate() {
            let cs = &mut cs.namespace(|| format!("merkle tree hash {}", i));

            // "direction_bit" determines if the current subtree
            // is the "right" leaf at this depth of the tree.

            // Witness the authentication path element adjacent
            // at this depth.
            let path_element = &audit_path[i];

            // Swap the two if the current subtree is on the right
            let (xl, xr) = num::AllocatedNum::conditionally_reverse(
                cs.namespace(|| "conditional reversal of preimage"),
                &cur,
                path_element,
                &direction_bit,
            )?;

            // We don't need to be strict, because the function is
            // collision-resistant. If the prover witnesses a congruency,
            // they will be unable to find an authentication path in the
            // tree with high probability.
            let mut preimage = vec![];
            preimage.extend(xl.into_bits_le(cs.namespace(|| "xl into bits"))?);
            preimage.extend(xr.into_bits_le(cs.namespace(|| "xr into bits"))?);

            // Compute the new subtree value
            cur = pedersen_hash::pedersen_hash(
                cs.namespace(|| "computation of pedersen hash"),
                pedersen_hash::Personalization::MerkleTree(i),
                &preimage,
                params,
            )?
            .get_x()
            .clone(); // Injective encoding
        }

        // enforce old root before update
        cs.enforce(
            || "enforce correct old root for leaf",
            |lc| lc + cur.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + old_root.get_variable(),
        );
    }

    // Initial leaf values are allocated, so we modify a leaf

    // Leaf can be empty if and only if the nonce == 0 && balance == 0
    // but we also check that pub_x and pub_y are zeroes.
    // External witness is used whether leaf is empty or not

    let leaf_is_empty = boolean::Boolean::from(boolean::AllocatedBit::alloc(
        cs.namespace(|| "Allocate leaf is empty"),
        witness.leaf_is_empty,
    )?);

    // constraint it
    // balance * leaf_is_empty == 0 -> balance == 0 || leaf_is_empty != 1
    cs.enforce(
        || "boolean constraint for balance is zero for empty leaf",
        |lc| lc + leaf.value.get_variable(),
        |_| leaf_is_empty.lc(CS::one(), E::Fr::one()),
        |lc| lc,
    );

    cs.enforce(
        || "boolean constraint for nonce is zero for empty leaf",
        |lc| lc + leaf.nonce.get_variable(),
        |_| leaf_is_empty.lc(CS::one(), E::Fr::one()),
        |lc| lc,
    );

    cs.enforce(
        || "boolean constraint for pub_x is zero for empty leaf",
        |lc| lc + leaf.pub_x.get_variable(),
        |_| leaf_is_empty.lc(CS::one(), E::Fr::one()),
        |lc| lc,
    );

    cs.enforce(
        || "boolean constraint for pub_y is zero for empty leaf",
        |lc| lc + leaf.pub_y.get_variable(),
        |_| leaf_is_empty.lc(CS::one(), E::Fr::one()),
        |lc| lc,
    );

    // reconstruct a new leaf structure
    // first decompress the input public key using the y point
    // and conditionally select either existing value or
    // a value from witness
    // In any case this value should be a valid public key in the correct group!

    // witness new public key
    let new_pk_x = num::AllocatedNum::alloc(cs.namespace(|| "updated public key x"), || {
        Ok(*witness.new_pub_x.get()?)
    })?;

    let new_pk_y = num::AllocatedNum::alloc(cs.namespace(|| "updated public key y"), || {
        Ok(*witness.new_pub_y.get()?)
    })?;

    // if leaf is empty - use witness, other wise use existing
    let leaf_pk_x = num::AllocatedNum::conditionally_select(
        cs.namespace(|| "conditional select public key x"),
        &new_pk_x,
        &leaf.pub_x,
        &leaf_is_empty,
    )?;

    // if leaf is empty - use witness, other wise use existing
    let leaf_pk_y = num::AllocatedNum::conditionally_select(
        cs.namespace(|| "conditional select public key y"),
        &new_pk_y,
        &leaf.pub_y,
        &leaf_is_empty,
    )?;

    // interpret as point, check on curve
    let updated_pk = ecc::EdwardsPoint::interpret(
        cs.namespace(|| "witness updated leaf public key"),
        &leaf_pk_x,
        &leaf_pk_y,
        params,
    )?;

    // order check here is not implemented by design

    // // and check order
    // updated_pk.assert_not_small_order(
    //     cs.namespace(|| "assert update public key is in correct group"),
    //     params
    // )?;

    // repack balances as we have truncated bit decompositions already
    let mut old_balance_lc = Num::<E>::zero();
    let mut coeff = E::Fr::one();
    for bit in &leaf.value_bits {
        old_balance_lc = old_balance_lc.add_bool_with_coeff(CS::one(), &bit, coeff);
        coeff.double();
    }

    let old_balance = AllocatedNum::alloc(cs.namespace(|| "allocate old leaf balance"), || {
        Ok(*old_balance_lc.get_value().get()?)
    })?;

    cs.enforce(
        || "pack old leaf balance",
        |lc| lc + old_balance.get_variable(),
        |lc| lc + CS::one(),
        |_| old_balance_lc.lc(E::Fr::one()),
    );

    // witness the deposit amount
    let amount = AllocatedNum::alloc(cs.namespace(|| "allocate deposit amount"), || {
        Ok(*request.amount.get()?)
    })?;

    let mut amount_bits = amount.into_bits_le(cs.namespace(|| "decompose deposit amount bits"))?;
    amount_bits.truncate(plasma_constants::BALANCE_BIT_WIDTH);

    let new_balance = AllocatedNum::alloc(cs.namespace(|| "new balance from"), || {
        let old_balance_value = *old_balance.get_value().get()?;
        let deposit_value = *amount.clone().get_value().get()?;
        let mut new_balance_value = old_balance_value;
        new_balance_value.add_assign(&deposit_value);

        Ok(new_balance_value)
    })?;

    // constraint no overflow
    new_balance.limit_number_of_bits(
        cs.namespace(|| "limit number of bits for new balance from"),
        plasma_constants::BALANCE_BIT_WIDTH,
    )?;

    // enforce increase of balance
    cs.enforce(
        || "enforce new balance",
        |lc| lc + new_balance.get_variable(),
        |lc| lc + CS::one(),
        |lc| lc + old_balance.get_variable() + amount.get_variable(),
    );

    // Now we should assemble a new root by wrapping a tree backwards

    let mut pub_x_content_new = updated_pk
        .get_x()
        .into_bits_le(cs.namespace(|| "updated pub_x bits"))?;
    pub_x_content_new.truncate(1);

    let mut pub_y_content_new = updated_pk
        .get_y()
        .into_bits_le(cs.namespace(|| "updated pub_y bits"))?;
    pub_y_content_new.resize(
        plasma_constants::FR_BIT_WIDTH - 1,
        boolean::Boolean::Constant(false),
    );

    // make leaf
    {
        let mut leaf_content = vec![];

        // change balance and nonce

        let mut value_content =
            new_balance.into_bits_le(cs.namespace(|| "from leaf updated amount bits"))?;

        value_content.truncate(plasma_constants::BALANCE_BIT_WIDTH);
        leaf_content.extend(value_content.clone());

        leaf_content.extend(leaf.nonce_bits.clone());

        // update public keys

        append_packed_public_key(
            &mut leaf_content,
            pub_x_content_new.clone(),
            pub_y_content_new.clone(),
        );

        assert_eq!(
            leaf_content.len(),
            plasma_constants::BALANCE_BIT_WIDTH
                + plasma_constants::NONCE_BIT_WIDTH
                + plasma_constants::FR_BIT_WIDTH
        );

        // Compute the hash of the from leaf
        leaf_hash = pedersen_hash::pedersen_hash(
            cs.namespace(|| "leaf content hash updated"),
            pedersen_hash::Personalization::NoteCommitment,
            &leaf_content,
            params,
        )?;
    }

    let mut cur = leaf_hash.get_x().clone();

    // Ascend the merkle tree authentication path
    for (i, direction_bit) in path_bits.clone().into_iter().enumerate() {
        let cs = &mut cs.namespace(|| format!("update merkle tree hash {}", i));

        // "direction_bit" determines if the current subtree
        // is the "right" leaf at this depth of the tree.

        // Witness the authentication path element adjacent
        // at this depth.
        let path_element = &audit_path[i];

        // Swap the two if the current subtree is on the right
        let (xl, xr) = num::AllocatedNum::conditionally_reverse(
            cs.namespace(|| "conditional reversal of preimage"),
            &cur,
            path_element,
            &direction_bit,
        )?;

        // We don't need to be strict, because the function is
        // collision-resistant. If the prover witnesses a congruency,
        // they will be unable to find an authentication path in the
        // tree with high probability.
        let mut preimage = vec![];
        preimage.extend(xl.into_bits_le(cs.namespace(|| "xl into bits"))?);
        preimage.extend(xr.into_bits_le(cs.namespace(|| "xr into bits"))?);

        // Compute the new subtree value
        cur = pedersen_hash::pedersen_hash(
            cs.namespace(|| "computation of pedersen hash"),
            pedersen_hash::Personalization::MerkleTree(i),
            &preimage,
            params,
        )?
        .get_x()
        .clone(); // Injective encoding
    }

    // the last step - we expose public data for later commitment

    // data packing should be BE
    let mut public_data = vec![];
    let mut path_bits = path_bits.clone();
    path_bits.reverse();
    public_data.extend(path_bits);
    let mut amount_bits_be = amount_bits.clone();
    amount_bits_be.reverse();
    public_data.extend(amount_bits_be);

    let mut pub_bits_be = pub_y_content_new.clone();
    assert_eq!(pub_bits_be.len(), plasma_constants::FR_BIT_WIDTH - 1);

    assert_eq!(pub_x_content_new.len(), 1);
    pub_bits_be.extend(pub_x_content_new);

    pub_bits_be.reverse();
    public_data.extend(pub_bits_be);

    assert_eq!(
        public_data.len(),
        plasma_constants::BALANCE_TREE_DEPTH
            + plasma_constants::BALANCE_BIT_WIDTH
            + plasma_constants::FR_BIT_WIDTH
    );

    Ok((cur, public_data))
}

#[cfg(test)]
mod test {
    use super::*;

    use log::debug;

    use ff::PrimeFieldRepr;
    use sapling_crypto::jubjub::FixedGenerators;

    use sapling_crypto::eddsa::{PrivateKey, PublicKey};

    #[test]
    fn test_deposit_in_empty_leaf() {
        use crate::CircuitAccountTree;
        use ff::{BitIterator, Field};
        use models::plasma::circuit::account::CircuitAccount;
        use pairing::bn256::*;
        use rand::{Rng, SeedableRng, XorShiftRng};
        use sapling_crypto::alt_babyjubjub::AltJubjubBn256;
        use sapling_crypto::circuit::test::*;
        // use super::super::account_tree::{AccountTree, Account};
        use models::plasma::circuit::utils::be_bit_vector_into_bytes;

        use crypto::digest::Digest;
        use crypto::sha2::Sha256;
        use hex;

        let params = &AltJubjubBn256::new();
        let p_g = FixedGenerators::SpendingKeyGenerator;

        let rng = &mut XorShiftRng::from_seed([0x3dbe_6258, 0x8d31_3d76, 0x3237_db17, 0xe5bc_0654]);

        let tree_depth = plasma_constants::BALANCE_TREE_DEPTH as u32;
        let mut tree = CircuitAccountTree::new(tree_depth);
        let initial_root = tree.root_hash();
        debug!("Initial root = {}", initial_root);

        let capacity = tree.capacity();
        assert_eq!(capacity, 1 << plasma_constants::BALANCE_TREE_DEPTH);

        let sender_sk = PrivateKey::<Bn256>(rng.gen());
        let sender_pk = PublicKey::from_private(&sender_sk, p_g, params);
        let (sender_x, sender_y) = sender_pk.0.into_xy();
        debug!("x = {}, y = {}", sender_x, sender_y);

        // give some funds to sender and make zero balance for recipient

        // let sender_leaf_number = 1;

        let mut sender_leaf_number: u32 = rng.gen();
        sender_leaf_number %= capacity;

        let transfer_amount: u128 = 1_234_567_890;

        let transfer_amount_as_field_element = Fr::from_str(&transfer_amount.to_string()).unwrap();

        let sender_leaf = CircuitAccount {
            balance: transfer_amount_as_field_element,
            nonce: Fr::zero(),
            pub_x: sender_x,
            pub_y: sender_y,
        };

        tree.insert(sender_leaf_number, sender_leaf.clone());

        debug!(
            "Sender leaf hash is {}",
            tree.get_hash((tree_depth, sender_leaf_number))
        );

        //assert!(tree.verify_proof(sender_leaf_number, sender_leaf.clone(), tree.merkle_path(sender_leaf_number)));

        let path_from: Vec<Option<Fr>> = tree
            .merkle_path(sender_leaf_number)
            .into_iter()
            .map(|e| Some(e.0))
            .collect();

        let from = Fr::from_str(&sender_leaf_number.to_string());

        let request: DepositRequest<Bn256> = DepositRequest {
            into: from,
            amount: Some(transfer_amount_as_field_element),
            public_key: Some(sender_pk.0),
        };

        let leaf_witness = LeafWitness {
            balance: Some(Fr::zero()),
            nonce: Some(Fr::zero()),
            pub_x: Some(Fr::zero()),
            pub_y: Some(Fr::zero()),
        };

        let witness = DepositWitness {
            leaf: leaf_witness,
            auth_path: path_from,
            leaf_is_empty: Some(true),
            new_pub_x: Some(sender_x),
            new_pub_y: Some(sender_y),
        };

        let new_root = tree.root_hash();

        debug!("New root = {}", new_root);

        assert_ne!(initial_root, new_root);

        {
            let mut cs = TestConstraintSystem::<Bn256>::new();

            let mut public_data_initial_bits = Vec::new();

            // these two are BE encodings because an iterator is BE. This is also an Ethereum standard behavior

            let block_number_bits: Vec<bool> = BitIterator::new(Fr::one().into_repr()).collect();
            for _ in 0..256 - block_number_bits.len() {
                public_data_initial_bits.push(false);
            }
            public_data_initial_bits.extend(block_number_bits.into_iter());

            assert_eq!(public_data_initial_bits.len(), 256);

            let mut h = Sha256::new();

            let bytes_to_hash = be_bit_vector_into_bytes(&public_data_initial_bits);

            h.input(&bytes_to_hash);

            let mut hash_result = [0u8; 32];
            h.result(&mut hash_result[..]);

            debug!("Initial hash hex {}", hex::encode(hash_result));

            let mut packed_transaction_data = vec![];
            let transaction_data = request.public_data_into_bits();
            packed_transaction_data.extend(transaction_data.clone().into_iter());

            let _leaf_bits = packed_transaction_data.clone();

            let packed_transaction_data_bytes = be_bit_vector_into_bytes(&packed_transaction_data);

            debug!(
                "Packed transaction data hex {}",
                hex::encode(packed_transaction_data_bytes.clone())
            );

            let mut next_round_hash_bytes = vec![];
            next_round_hash_bytes.extend(hash_result.iter());
            next_round_hash_bytes.extend(packed_transaction_data_bytes);

            h = Sha256::new();
            h.input(&next_round_hash_bytes);
            hash_result = [0u8; 32];
            h.result(&mut hash_result[..]);

            debug!("Final hash as hex {}", hex::encode(hash_result));

            hash_result[0] &= 0x1f; // temporary solution

            let mut repr = Fr::zero().into_repr();
            repr.read_be(&hash_result[..])
                .expect("pack hash as field element");

            let public_data_commitment = Fr::from_repr(repr).unwrap();

            debug!(
                "Final data commitment as field element = {}",
                public_data_commitment
            );

            let instance = Deposit {
                params,
                number_of_deposits: 1,
                old_root: Some(initial_root),
                new_root: Some(new_root),
                public_data_commitment: Some(public_data_commitment),
                block_number: Some(Fr::one()),
                requests: vec![(request, witness)],
            };

            instance.synthesize(&mut cs).unwrap();

            debug!("{}", cs.find_unconstrained());

            debug!("{}", cs.num_constraints());

            assert_eq!(cs.num_inputs(), 4);

            let err = cs.which_is_unsatisfied();
            if err.is_some() {
                panic!("ERROR satisfying in {}", err.unwrap());
            }
        }
    }

    #[test]
    fn test_deposit_into_existing_leaf() {
        use crate::CircuitAccountTree;
        use ff::{BitIterator, Field};
        use models::plasma::circuit::account::CircuitAccount;
        use pairing::bn256::*;
        use rand::{Rng, SeedableRng, XorShiftRng};
        use sapling_crypto::alt_babyjubjub::AltJubjubBn256;
        use sapling_crypto::circuit::test::*;
        // use super::super::account_tree::{AccountTree, Account};
        use models::plasma::circuit::utils::be_bit_vector_into_bytes;

        use crypto::digest::Digest;
        use crypto::sha2::Sha256;
        use hex;

        let params = &AltJubjubBn256::new();
        let p_g = FixedGenerators::SpendingKeyGenerator;

        let rng = &mut XorShiftRng::from_seed([0x3dbe_6258, 0x8d31_3d76, 0x3237_db17, 0xe5bc_0654]);

        let tree_depth = plasma_constants::BALANCE_TREE_DEPTH as u32;
        let mut tree = CircuitAccountTree::new(tree_depth);

        let capacity = tree.capacity();
        assert_eq!(capacity, 1 << plasma_constants::BALANCE_TREE_DEPTH);

        let sender_sk = PrivateKey::<Bn256>(rng.gen());
        let sender_pk = PublicKey::from_private(&sender_sk, p_g, params);
        let (sender_x, sender_y) = sender_pk.0.into_xy();

        // give some funds to sender and make zero balance for recipient

        // let sender_leaf_number = 1;

        let mut sender_leaf_number: u32 = rng.gen();
        sender_leaf_number %= capacity;

        let transfer_amount: u128 = 1_234_567_890;

        let transfer_amount_as_field_element = Fr::from_str(&transfer_amount.to_string()).unwrap();

        let sender_leaf = CircuitAccount {
            balance: transfer_amount_as_field_element,
            nonce: Fr::zero(),
            pub_x: sender_x,
            pub_y: sender_y,
        };

        tree.insert(sender_leaf_number, sender_leaf.clone());

        debug!(
            "Sender leaf hash is {}",
            tree.get_hash((tree_depth, sender_leaf_number))
        );

        //assert!(tree.verify_proof(sender_leaf_number, sender_leaf.clone(), tree.merkle_path(sender_leaf_number)));

        let initial_root = tree.root_hash();
        debug!("Initial root = {}", initial_root);

        let mut double_the_amount = transfer_amount_as_field_element;
        double_the_amount.double();

        let sender_leaf_redeposited = CircuitAccount {
            balance: double_the_amount,
            nonce: Fr::zero(),
            pub_x: sender_x,
            pub_y: sender_y,
        };

        tree.insert(sender_leaf_number, sender_leaf_redeposited);

        let path_from: Vec<Option<Fr>> = tree
            .merkle_path(sender_leaf_number)
            .into_iter()
            .map(|e| Some(e.0))
            .collect();

        let from = Fr::from_str(&sender_leaf_number.to_string());

        let request: DepositRequest<Bn256> = DepositRequest {
            into: from,
            amount: Some(transfer_amount_as_field_element),
            public_key: Some(sender_pk.0),
        };

        let leaf_witness = LeafWitness {
            balance: Some(transfer_amount_as_field_element),
            nonce: Some(Fr::zero()),
            pub_x: Some(sender_x),
            pub_y: Some(sender_y),
        };

        let witness = DepositWitness {
            leaf: leaf_witness,
            auth_path: path_from,
            leaf_is_empty: Some(false),
            new_pub_x: Some(sender_x),
            new_pub_y: Some(sender_y),
        };

        let new_root = tree.root_hash();

        debug!("New root = {}", new_root);

        assert!(initial_root != new_root);

        {
            let mut cs = TestConstraintSystem::<Bn256>::new();

            let mut public_data_initial_bits = vec![];

            // these two are BE encodings because an iterator is BE. This is also an Ethereum standard behavior

            let block_number_bits: Vec<bool> = BitIterator::new(Fr::one().into_repr()).collect();
            for _ in 0..256 - block_number_bits.len() {
                public_data_initial_bits.push(false);
            }
            public_data_initial_bits.extend(block_number_bits.into_iter());

            assert_eq!(public_data_initial_bits.len(), 256);

            let mut h = Sha256::new();

            let bytes_to_hash = be_bit_vector_into_bytes(&public_data_initial_bits);

            h.input(&bytes_to_hash);

            let mut hash_result = [0u8; 32];
            h.result(&mut hash_result[..]);

            debug!("Initial hash hex {}", hex::encode(hash_result));

            let mut packed_transaction_data = vec![];
            let transaction_data = request.public_data_into_bits();
            packed_transaction_data.extend(transaction_data.clone().into_iter());

            let _leaf_bits = packed_transaction_data.clone();

            let packed_transaction_data_bytes = be_bit_vector_into_bytes(&packed_transaction_data);

            debug!(
                "Packed transaction data hex {}",
                hex::encode(packed_transaction_data_bytes.clone())
            );

            let mut next_round_hash_bytes = vec![];
            next_round_hash_bytes.extend(hash_result.iter());
            next_round_hash_bytes.extend(packed_transaction_data_bytes);

            h = Sha256::new();
            h.input(&next_round_hash_bytes);
            hash_result = [0u8; 32];
            h.result(&mut hash_result[..]);

            debug!("Final hash as hex {}", hex::encode(hash_result));

            hash_result[0] &= 0x1f; // temporary solution

            let mut repr = Fr::zero().into_repr();
            repr.read_be(&hash_result[..])
                .expect("pack hash as field element");

            let public_data_commitment = Fr::from_repr(repr).unwrap();

            debug!(
                "Final data commitment as field element = {}",
                public_data_commitment
            );

            let instance = Deposit {
                params,
                number_of_deposits: 1,
                old_root: Some(initial_root),
                new_root: Some(new_root),
                public_data_commitment: Some(public_data_commitment),
                block_number: Some(Fr::one()),
                requests: vec![(request, witness)],
            };

            instance.synthesize(&mut cs).unwrap();

            debug!("{}", cs.find_unconstrained());

            debug!("{}", cs.num_constraints());

            assert_eq!(cs.num_inputs(), 4);

            let err = cs.which_is_unsatisfied();
            if err.is_some() {
                panic!("ERROR satisfying in {}", err.unwrap());
            }
        }
    }

}
