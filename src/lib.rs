#[macro_use]
extern crate serde;

use std::{
    collections::HashMap,
    error::Error,
    io::{Read, Write},
    ops::{Add, AddAssign, Sub, SubAssign},
};

// this could be something provided by a command line arg if such a feature
// is requested, but we in practice this is oftentimes system-wide or well-known
// parameter and so we hard-code it, which implies that re-build will be needed
// if we want to adjust it
const DECIMALS_PRECISION: u32 = 4;

type ClientID = u16;
type TxnID = u32;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TxnKind {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    ChargeBack,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, PartialOrd)]
struct Amount(#[serde(deserialize_with = "utils::deser_amount")] f64);

impl Add for Amount {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}
impl AddAssign for Amount {
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0 + rhs.0;
    }
}
impl Sub for Amount {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}
impl SubAssign for Amount {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0 - rhs.0;
    }
}

#[derive(Debug, Deserialize)]
struct TxnRecord {
    #[serde(rename = "type")]
    kind: TxnKind,

    /// Client's _unique_ identifier.
    client: ClientID,

    /// Transaction's _unique_ identifier.
    #[allow(unused)]
    tx: TxnID,

    /// Transaction ammount.
    amount: Amount,
}

#[derive(Debug, Serialize)]
struct Account {
    /// Client's _unique_ identifier.
    client: ClientID,

    /// Available funds.
    ///
    /// Total funds available for trading, staking, withdrawal, etc.
    available: Amount,

    /// Total funds held for dispute.
    held: Amount,

    /// Total funds.
    ///
    /// Calcualted as [`Account::available`] plus [`Account::held`]
    total: Amount,

    /// Whether this account is locked.
    ///
    /// An account gets locked when a charge back is taking place.
    locked: bool,
}

impl Account {
    fn new(client: ClientID) -> Self {
        Account {
            client,
            available: Amount::default(),
            held: Amount::default(),
            total: Amount::default(),
            locked: false,
        }
    }

    /// Credit the client's account.
    fn deposit(&mut self, amount: Amount) {
        self.available += amount;
        self.total += amount;
    }

    /// Debit the client's account.
    ///
    /// If they do not have sufficient available funds ([`Account::available`]),
    /// the operation will return `false` leaving the account intact, otherwise
    /// their [`Account::available`] and [`Account::total`] will be reduced by
    /// the provided `amount`.
    fn withdraw(&mut self, amount: Amount) -> bool {
        if self.available < amount {
            return false;
        }
        self.available -= amount;
        self.total -= amount;
        true
    }
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
    let mut accounts: HashMap<ClientID, Account> = HashMap::new();
    for result in csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(reader)
        .deserialize()
    {
        let record: TxnRecord = result?;
        match record.kind {
            TxnKind::Deposit => {
                if let Some(account) = accounts.get_mut(&record.client) {
                    account.deposit(record.amount);
                } else {
                    let mut account = Account::new(record.client);
                    account.deposit(record.amount);
                    accounts.insert(record.client, account);
                }
            }
            TxnKind::Withdrawal => {
                if let Some(account) = accounts.get_mut(&record.client) {
                    // this operation is fallible, but we are currently
                    // just moving on; we can consider emitting a warn event
                    // or collect such cases and reporting back to the caller
                    let _ok = account.withdraw(record.amount);
                } else {
                    // the account was not there in the first place, and so we
                    // create one and continue; there is probably no sense in
                    // trying to withdraw from the newly created account (unless
                    // we withdraw `0.0`?)
                    accounts.insert(record.client, Account::new(record.client));
                }
            }
            _ => unimplemented!(),
        }
    }
    let mut wrt = csv::Writer::from_writer(writer);
    for account in accounts.values() {
        wrt.serialize(account)?;
    }
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
