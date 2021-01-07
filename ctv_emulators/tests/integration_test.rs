use bitcoin::hashes::sha256d;
use bitcoin::hashes::Hash;
use bitcoin::secp256k1::Secp256k1;
use bitcoin::util::amount::Amount;
use bitcoin::util::bip32::*;
use bitcoin::Script;
use bitcoin::TxOut;
use bitcoin::Txid;
use emulator_connect::HDOracleEmulatorConnection;
use emulator_connect::*;
use sapio::contract::object::BadTxIndex;
use sapio::contract::object::TxIndex;
use sapio::contract::*;
use sapio::*;
use sapio_base::timelocks::RelTime;
use std::collections::HashMap;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;

pub struct TestEmulation<T> {
    pub to_contract: T,
    pub amount: Amount,
    pub timeout: u16,
}

impl<T> TestEmulation<T>
where
    T: Compilable,
{
    then!(
        complete | s,
        ctx | {
            ctx.template()
                .add_output(s.amount, &s.to_contract, None)?
                .set_sequence(0, RelTime::from(s.timeout).into())?
                .into()
        }
    );
}

impl<T: Compilable + 'static> Contract for TestEmulation<T> {
    declare! {then, Self::complete}
    declare! {non updatable}
}

#[test]
fn test_connect() {
    let root =
        ExtendedPrivKey::new_master(bitcoin::network::constants::Network::Regtest, &[44u8; 32])
            .unwrap();
    let pk_root = ExtendedPubKey::from_private(&Secp256k1::new(), &root);
    let rt1 = Arc::new(tokio::runtime::Runtime::new().unwrap());
    let (shutdown, quit) = tokio::sync::oneshot::channel();
    {
        let rt = rt1.clone();
        std::thread::spawn(move || {
            let oracle = HDOracleEmulator::new(root);
            rt.block_on(async {
                let server = tokio::spawn(oracle.bind("127.0.0.1:8080"));
                quit.await.unwrap();
                server.abort();
            });
        });
    };

    let contract_1 = TestEmulation {
        to_contract: Compiled::from_address(
            bitcoin::Address::from_str("bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh").unwrap(),
            None,
        ),
        amount: Amount::from_btc(1.0).unwrap(),
        timeout: 6,
    };
    let contract = TestEmulation {
        to_contract: contract_1,
        amount: Amount::from_btc(1.0).unwrap(),
        timeout: 4,
    };
    let rt2 = Arc::new(tokio::runtime::Runtime::new().unwrap());
    let connecter = rt2.block_on(async {
        HDOracleEmulatorConnection::new(
            "127.0.0.1:8080",
            pk_root,
            rt2.clone(),
            Arc::new(Secp256k1::new()),
        )
        .await
        .unwrap()
    });
    let rc_conn: Rc<dyn CTVEmulator> = Rc::new(connecter);
    let compiled = contract
        .compile(&Context::new(
            Amount::from_btc(1.0).unwrap(),
            Some(rc_conn.clone()),
        ))
        .unwrap();
    let txindex: Rc<dyn TxIndex> = Rc::new(BadTxIndex::new());
    let fake_txid = Txid::from_hash(sha256d::Hash::from_slice(&[0u8; 32]).unwrap());
    let tx = bitcoin::Transaction {
        version: 2,
        lock_time: 0,
        input: vec![],
        output: vec![TxOut {
            value: Amount::from_btc(1.0).unwrap().as_sat(),
            script_pubkey: Script::new(),
        }],
    };
    txindex.add_output(fake_txid, tx);
    let _psbts = compiled.bind_psbt(
        bitcoin::OutPoint::new(fake_txid, 0),
        HashMap::new(),
        txindex,
        rc_conn,
    );
    shutdown.send(()).unwrap();
    // TODO: Test PSBT result
}