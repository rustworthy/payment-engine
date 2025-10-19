#[macro_use]
extern crate serde;

use std::{
    error::Error,
    io::{Read, Write},
};

// this could be something provided by a command line arg if such a feature
// is requested, but we in practice this is oftentimes system-wide or well-known
// parameter and so we hard-code it, which implies that re-build will be needed
// if we want to adjust it
const DECIMALS_PRECISION: u32 = 4;

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

#[allow(unused)]
#[derive(Debug, Serialize)]
pub struct AccountState {
    /// Client's _unique_ identifier.
    client: u16,

    /// Available funds.
    ///
    /// Total funds available for trading, staking, withdrawal, etc.
    available: f64,

    /// Total funds held for dispute.
    held: f64,

    /// Total funds.
    ///
    /// Calcualted as [`AccountState::available`] plus [`AccountState::held`]
    total: f64,

    /// Whether this account is locked.
    ///
    /// An account gets locked when a charge back is taking place.
    locked: bool,
}

/// Process the records contained in the `reader` in CSV format.
///
/// Note how there is no timestamp field in the `TxnRecord` for us to be able
/// to establish the order. Instead, we expect the transactions to have been
/// written to whatever we are now reading from (e.g. a file) respecting
/// the chronological order.
///
/// Whitespaces and decimal precisions are accepted. Internally, Whitespaces
/// get trimmed both in headers and in fields.
// TODO: once our trace-bullet implementation is ready, consider intoducing
// our own enumerated error using `thiserror` and `anyhow`
pub fn process<R, W>(reader: R, writer: W) -> Result<(), Box<dyn Error>>
where
    R: Read,
    W: Write,
{
    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(reader);
    for result in rdr.deserialize() {
        let record: TxnRecord = result?;
        println!("{:?}", record);
    }
    let mut wrt = csv::Writer::from_writer(writer);
    wrt.serialize(AccountState {
        client: 1,
        available: utils::to_precision(80.299911, DECIMALS_PRECISION),
        held: utils::to_precision(20.00199, DECIMALS_PRECISION),
        total: 100.50,
        locked: false,
    })?;
    wrt.flush()?;
    Ok(())
}

mod utils {
    use crate::DECIMALS_PRECISION;
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
        Ok(to_precision(amount, DECIMALS_PRECISION))
    }

    // TODO: consider adding a trait and  auto-imlement it for floats:
    // will probably be more idiomatic, but on the other hand less explicit
    pub(crate) fn to_precision(value: f64, precesion: u32) -> f64 {
        (value * 10u32.pow(precesion) as f64).floor() / 10u32.pow(precesion) as f64
    }
}
