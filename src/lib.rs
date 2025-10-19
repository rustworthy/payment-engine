#[macro_use]
extern crate serde;

use std::{error::Error, io::Read};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TxnKind {
    Deposit,
    Withdrawal,
}

#[allow(unused)]
#[derive(Debug, Deserialize)]
struct TxnRecord {
    #[serde(rename = "type")]
    kind: TxnKind,
    client: u16,
    tx: u32,
    amount: f32,
}

pub fn process<B>(reader: B) -> Result<(), Box<dyn Error>>
where
    B: Read,
{
    let mut rdr = csv::Reader::from_reader(reader);
    for result in rdr.deserialize() {
        let record: TxnRecord = result?;
        println!("{:?}", record);
    }
    Ok(())
}
