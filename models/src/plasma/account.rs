use crate::circuit;
use crate::plasma::params;
use crate::primitives::GetBits;
use crate::{Engine, Fr, PublicKey};
use bigdecimal::BigDecimal;
use sapling_crypto::jubjub::{edwards, Unknown};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Account {
    pub balance: BigDecimal,
    pub nonce: u32,
    pub public_key_x: Fr,
    pub public_key_y: Fr,
}

impl GetBits for Account {
    fn get_bits_le(&self) -> Vec<bool> {
        circuit::account::CircuitAccount::<Engine>::from(self.clone()).get_bits_le()

        // TODO: make more efficient:

        // let mut leaf_content = Vec::new();
        // leaf_content.extend(self.balance.get_bits_le_fixed(params::BALANCE_BIT_WIDTH));
        // leaf_content.extend(self.nonce.get_bits_le_fixed(params::NONCE_BIT_WIDTH));
        // leaf_content.extend(self.pub_x.get_bits_le_fixed(params::FR_BIT_WIDTH));
        // leaf_content.extend(self.pub_y.get_bits_le_fixed(params::FR_BIT_WIDTH));
        // leaf_content
    }
}

impl Account {
    pub fn get_pub_key(&self) -> Option<PublicKey> {
        let point = edwards::Point::<Engine, Unknown>::from_xy(
            self.public_key_x,
            self.public_key_y,
            &params::JUBJUB_PARAMS,
        );
        point.map(sapling_crypto::eddsa::PublicKey::<Engine>)
    }
}

#[test]
fn test_default_account() {
    let a = Account::default();
    a.get_bits_le();
}
