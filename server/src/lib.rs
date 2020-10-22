#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate log;

pub mod api_server;
pub mod committer;
//pub mod eth_sender;
//pub mod eth_watch;
pub mod nonce_futures;
pub mod state_keeper;


use web3::types::{U256,H256};
use bigdecimal::BigDecimal;
use ff::{Field, PrimeField, PrimeFieldRepr};
use models::plasma::params;
use models::plasma::tx::{DepositTx};
use models::plasma::{Engine, Fr};
use sapling_crypto::jubjub::{edwards, Unknown};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DepositReq{
    pub address: String,
    pub public_key:[String;2],
    pub deposit_amount:BigDecimal,
    
}

impl DepositReq{
    pub fn get_DepositTx(&self) -> Result<DepositTx,String>{
        println!("account {:?}",&self);

            let address_str = upper_to_lower(&self.address);

            let address_vec = hex256_to_u8vec(&address_str);
            let account_id = U256::from(H256::from(address_vec.as_slice())).low_u32() >> 8;
            
            

            let pubx_vec = hex256_to_u8vec(&self.public_key[0]);
            let mut puby_vec = hex256_to_u8vec(&self.public_key[1]);

            puby_vec[0] = puby_vec[0] + ((pubx_vec[31] & 1) << 7 );

            let mut public_key_bytes = puby_vec;
            let x_sign = public_key_bytes[0] & 0x80 > 0;
            public_key_bytes[0] &= 0x7f;
            let mut fe_repr = Fr::zero().into_repr();
            fe_repr
                .read_be(public_key_bytes.as_slice())
                .expect("read public key point");

            let y = Fr::from_repr(fe_repr);
            if y.is_err() {
                return Err("could not parse y".to_owned());
            }
            let public_key_point = edwards::Point::<Engine, Unknown>::get_for_y(
                y.unwrap(),
                x_sign,
                &params::JUBJUB_PARAMS,
            );
            if public_key_point.is_none() {
                return Err("public_key_point conversion error".to_owned());
            }
            let (pub_x, pub_y) = public_key_point.unwrap().into_xy();

            let deposit_tx = DepositTx{
                account: account_id.clone(),
                amount: self.deposit_amount.clone(),
                pub_x: pub_x,
                pub_y: pub_y,
            };

            Ok(deposit_tx)

    }
}

// #[derive(Clone, Debug, Serialize, Deserialize)]
// pub struct ExitReq{
//     pub address:String,
// }


pub fn upper_to_lower(str:&String) -> String{

    let mut lower_string = String::new();

    let c = 65_u8 as char;

    for s in str.as_bytes(){
        if s >= &65_u8 && s <= &90_u8 {
            let c = s.clone() + 32;
            lower_string.push(c as char);
        }else{
            lower_string.push(s.clone() as char);
        }
    }

    lower_string
}

/// 将16进制的字符串转化为10进制长32的u8数组
pub fn hex256_to_u8vec(hex_string: &String) -> Vec<u8>{

    let hex:&str;
    if(hex_string.len() > 2){
        if '0' == hex_string.chars().nth(0).unwrap() && 'x' == hex_string.chars().nth(1).unwrap(){
            hex = &hex_string[2..];
        }else{
            hex = &hex_string[..];
        }
    }else{
        hex = "";
    }
    
    let mut hex_64_u8 = vec!('0' as u8; 64 - hex.len());

    for i in hex.as_bytes(){
        hex_64_u8.push(i.clone());
    }
    assert!(hex_64_u8.len() == 64);

    let mut iter = hex_64_u8.into_iter();

    let mut u8_vec:Vec<u8> = Vec::new();

    //let mut hex_iter = hex.chars();
    for _ in 0..32{
        let per_c = iter.next().unwrap();
        let last_c = iter.next().unwrap();
        let per_u8 = u8char_to_u8(per_c);
        let last_c = u8char_to_u8(last_c);
        u8_vec.push(per_u8*16 + last_c); 
    }
    assert!(u8_vec.len()==32);
    return u8_vec;
}

fn u8char_to_u8(c:u8) -> u8{
    //let au8 = c as u8;
    return match c  {
        48_u8 => 0,
        49_u8 => 1,
        50_u8 => 2,
        51_u8 => 3,
        52_u8=> 4,
        53_u8=> 5,
        54_u8=> 6,
        55_u8=> 7,
        56_u8=> 8,
        57_u8=> 9,
        97_u8=> 10,
        98_u8=> 11,
        99_u8=> 12,
        100_u8=> 13,
        101_u8=> 14,
        102_u8=> 15,
        _  => panic!("hex parse err"),
    }
}