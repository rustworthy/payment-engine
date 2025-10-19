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

    /// Client's _unique_ identifier.
    client: u16,

    /// Transaction's _unique_ identifier.
    tx: u32,

    /// Transaction ammount.
    #[serde(deserialize_with = "utils::deser_amount")]
    amount: f64,
}

/// Process the records contained in the `reader` in CSV format.
///
/// Note how there is no timestamp field in the `TxnRecord` for us to be able
/// to establish the order. Instead, we expect the transactions to have been
/// written to whatever we are now reading from (e.g. a file) respecting
/// the chronological order.
///
/// Whitespaces and decimal precisions (up to four places past the decimal)
/// are accepted. Internally, whitespaces get trimmed both in headers and in fields.
// TODO: once our trace-bullet implementation is ready, consider intoducing
// our own enumerated error using `thiserror` and `anyhow`
pub fn process<B>(reader: B) -> Result<(), Box<dyn Error>>
where
    B: Read,
{
    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(reader);
    for result in rdr.deserialize() {
        let record: TxnRecord = result?;
        println!("{:?}", record);
    }
    Ok(())
}

mod utils {
    use serde::Deserialize;
    use serde::Deserializer;

    // Deserialize transaction amount.
    //
    // Internally, will deserialize the `value` as `f64` and adjust it to
    // four places past the decimal, so that 1.53349999 turns into 1.5334.
    pub(crate) fn deser_amount<'de, D>(value: D) -> Result<f64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let amount = f64::deserialize(value)?;
        let amount = (amount * 10u32.pow(4) as f64).floor() / 10u32.pow(4) as f64;
        Ok(amount)
    }
}
