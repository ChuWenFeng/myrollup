import requests

transactions_url = "http://localhost:3000/api/v0.1/submit_tx"

transactions_parase = {
    'from': 1,
    'to': 2,
    'amount': 1,
    'fee': 0,
    'nonce': 0,
    'good_until_block': 2147483647,

}

Account1 = {
    "address": "0x1e8310840Cb4D5c5Db04cdd621a8694b02E5b353",
    "priavte_key": "0xe3f863b70b543222a3fdc984e57dcde64f5ac0cae8c91d42793f7ad59e686147"
}
Account2 = {
    "address": "0x08aaC43473FB6FaeEcb64ffE5326cf87A1434cB3",
    "private_key": "0xd3476a3b1d59e94791274e02bf9d50f57e6be1928a1fddf52f470c8b148a44b4"
}

test_url = "http://localhost:3000/api/v0.1/mytest"
MyObj = {'name': 'myname'}

status_url = "http://localhost:3000/api/v0.1/status"

hd = {'Content-Type': 'application/json'}

deposit_url = "http://localhost:3000/api/v0.1/deposit"

deposit_1 = {
    'account': 1,
    'amount': 100
}
deposit_2 = {
    'account': 2,
    'amount': 10
}


def main():
    r = requests.request('POST', url=deposit_url,
                         json=deposit_1)
    r.raise_for_status()
    r.encoding = r.apparent_encoding
    print(r.text)
    pass


if __name__ == "__main__":
    main()


#     pub struct TransferTx {
#     pub from: u32,
#     pub to: u32,
#     pub amount: BigDecimal,
#     pub fee: BigDecimal,
#     pub nonce: u32,
#     pub good_until_block: u32,
#     pub signature: TxSignature,

#     /// If present, it means that the signature has been verified against this key
#     #[serde(skip)]
#     pub cached_pub_key: Option<PublicKey>,
#   }

# curl localhost:8080/api/v0.1/submit_tx -X POST -d '{"from": 1,"to":2,"amount":1,"fee":0,"nonce":0,"good_until_block":0}' --header "Content-Type: application/json"

depostsreq = { "account": "0x24772e0a45344f76064db45f0be5924c017adabc", 
    "public_key": [ "172ea33871ed571dc13d51f204ca70c806db79409e960d2e1ed2a985674bc616", "12e54617abd9294072231e20002d1a26063683e9d0bf827593a0c5873bdbf16f" ], 
    "deposit_amount": "1" 
} 

deposit_op = {
    "id": None,
    "block": {
        "block_data": {
            "type": "Deposit",
            "batch_number": 0,
            "transactions": [
                {
                    "amount": "10",
                    "account": 2
                }
            ]
        },
        "block_number": 3,
        "new_root_hash": "0x04e3914382733823258ed45efaaceeedad8e4a897c7a2f842355e72652be6912"
    },
    "action": {
        "type": "Commit"
    },
    "accounts_updated": {
        "2": {
            "nonce": 0,
            "balance": "20",
            "public_key_x": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "public_key_y": "0x0000000000000000000000000000000000000000000000000000000000000000"
        }
    }
}

transaction_op = {
    "id": None,
    "block": {
        "block_data": {
            "type": "Transfer",
            "total_fees": "0",
            "transactions": [
                {
                    "to": 2,
                    "fee": "0",
                    "from": 1,
                    "nonce": 0,
                    "amount": "1",
                    "good_until_block": 2147483647
                },
                {
                    "to": 2,
                    "fee": "0",
                    "from": 1,
                    "nonce": 0,
                    "amount": "1",
                    "good_until_block": 2147483647
                },
                {
                    "to": 2,
                    "fee": "0",
                    "from": 1,
                    "nonce": 0,
                    "amount": "1",
                    "good_until_block": 2147483647
                },
                {
                    "to": 2,
                    "fee": "0",
                    "from": 1,
                    "nonce": 0,
                    "amount": "1",
                    "good_until_block": 2147483647
                },
                {
                    "to": 2,
                    "fee": "0",
                    "from": 1,
                    "nonce": 0,
                    "amount": "1",
                    "good_until_block": 2147483647
                },
                {
                    "to": 2,
                    "fee": "0",
                    "from": 1,
                    "nonce": 0,
                    "amount": "1",
                    "good_until_block": 2147483647
                },
                {
                    "to": 2,
                    "fee": "0",
                    "from": 1,
                    "nonce": 0,
                    "amount": "1",
                    "good_until_block": 2147483647
                },
                {
                    "to": 2,
                    "fee": "0",
                    "from": 1,
                    "nonce": 0,
                    "amount": "1",
                    "good_until_block": 2147483647
                }
            ]
        },
        "block_number": 4,
        "new_root_hash": "0x0c5b40e2384e39f08a07568b316594989d0b85d90afe831ce273338006881aa7"
    },
    "action": {
        "type": "Commit"
    },
    "accounts_updated": {
        "1": {
            "nonce": 0,
            "balance": "92",
            "public_key_x": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "public_key_y": "0x0000000000000000000000000000000000000000000000000000000000000000"
        },
        "2": {
            "nonce": 0,
            "balance": "28",
            "public_key_x": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "public_key_y": "0x0000000000000000000000000000000000000000000000000000000000000000"
        }
    }
}