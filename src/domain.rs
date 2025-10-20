use std::{
    error::Error,
    ops::{Add, AddAssign, Sub, SubAssign},
};

// this could be something provided by a command line arg if such a feature
// is requested, but we in practice this is oftentimes system-wide or well-known
// parameter and so we hard-code it, which implies that re-build will be needed
// if we want to adjust it
const DECIMALS_PRECISION: u32 = 4;

pub type ClientID = u16;
pub type TxnID = u32;

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct Amount {
    inner: i64,
}

impl Amount {
    /// Create new [`Amount`] from an f64 `value`.
    ///
    /// Internally, will store the `value` as i64 (counting in up to four
    /// places past the decimal in the given float), so that 1.53349999 turns
    /// into 15334.
    ///
    /// This conversion is fallible, since we are not allowing to create an
    /// [`Amount`] holding a NaN.
    pub fn try_from_f64(value: f64) -> Result<Self, Box<dyn Error>> {
        let amount = (value * 10u32.pow(DECIMALS_PRECISION) as f64).trunc();
        Ok(Self {
            inner: amount as i64,
        })
    }

    pub fn as_f64(&self) -> f64 {
        self.inner as f64 / 10u32.pow(DECIMALS_PRECISION) as f64
    }
}

impl Add for Amount {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            inner: self.inner + rhs.inner,
        }
    }
}
impl AddAssign for Amount {
    fn add_assign(&mut self, rhs: Self) {
        self.inner = self.inner + rhs.inner;
    }
}
impl Sub for Amount {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            inner: self.inner - rhs.inner,
        }
    }
}
impl SubAssign for Amount {
    fn sub_assign(&mut self, rhs: Self) {
        self.inner = self.inner - rhs.inner;
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TxnRecordKind {
    Deposit,
    Withdrawal,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum TxnState {
    #[default]
    Undisputed,
    Disputed,
    Reversed,
}

#[derive(Debug, Deserialize)]
pub struct TxnRecord {
    #[serde(rename = "type")]
    pub kind: TxnRecordKind,

    /// Client's _unique_ identifier.
    pub client: ClientID,

    /// Transaction's _unique_ identifier.
    #[allow(unused)]
    pub tx: TxnID,

    /// Transaction ammount.
    pub amount: Amount,

    /// Wether this transaction is under dispute.
    #[serde(skip)]
    pub state: TxnState,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisputeRecordKind {
    Dispute,
    Resolve,
    ChargeBack,
}

#[derive(Debug, Deserialize)]
pub struct DisputeRecord {
    #[serde(rename = "type")]
    pub kind: DisputeRecordKind,

    /// Client's _unique_ identifier.
    pub client: ClientID,

    /// Transaction's _unique_ identifier.
    pub tx: TxnID,
}

/// Operation record.
///
/// An operation can ether be a transaction one (debit or credit), which is
/// described as [`TxnRecord`], or a dispute resolution one ([`DisputeRecord`]).
/// The latter does not contain `amount`, it is rather referencing a transaction,
/// which - in its turn - always holds the amount in question.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RecordInner {
    TxnRecord(TxnRecord),
    DisputeRecord(DisputeRecord),
}

// an alternative approach would be to keep things flat: make the amount
// field optional and then just `.expect` the value to be there for deposits
// and withdrawals; it works but is not idiomatic and also semantically
// incorrect, and so instead we bifurcate the records into operations that
// create a transaction and hold the amount in question vs operations that
// reference such transactions (dispute resolution operations);
//
// we need a hack here to make serde crate play nicely with the csv crate, see:
// https://github.com/BurntSushi/rust-csv/issues/357
#[derive(Debug, Deserialize)]
pub struct Record {
    #[serde(flatten)]
    pub inner: RecordInner,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
pub struct Account {
    /// Client's _unique_ identifier.
    pub client: ClientID,

    /// Available funds.
    ///
    /// Total funds available for trading, staking, withdrawal, etc.
    pub available: Amount,

    /// Total funds held for dispute.
    pub held: Amount,

    /// Total funds.
    ///
    /// Calcualted as [`Account::available`] plus [`Account::held`]
    pub total: Amount,

    /// Whether this account is locked.
    ///
    /// An account gets locked when a charge back is taking place.
    pub locked: bool,
}

impl Account {
    pub fn new(client: ClientID) -> Self {
        Account {
            client,
            available: Amount::default(),
            held: Amount::default(),
            total: Amount::default(),
            locked: false,
        }
    }

    /// Credit the client's account.
    pub fn deposit(&mut self, amount: Amount) {
        self.available += amount;
        self.total += amount;
    }

    /// Debit the client's account.
    ///
    /// If they do not have sufficient available funds ([`Account::available`]),
    /// the operation will return `false` leaving the account intact, otherwise
    /// their [`Account::available`] and [`Account::total`] will be reduced by
    /// the provided `amount`.
    pub fn withdraw(&mut self, amount: Amount) -> bool {
        if self.available < amount {
            return false;
        }
        self.available -= amount;
        self.total -= amount;
        true
    }

    pub fn hold(&mut self, amount: Amount) {
        self.available -= amount;
        self.held += amount;
    }

    /// Unblock the previously disputed amount.
    pub fn resolve(&mut self, amount: Amount) {
        self.held -= amount;
        self.available += amount;
    }

    /// Unblock the previously disputed amount.
    pub fn charge_back(&mut self, amount: Amount) {
        self.held -= amount;
        self.total -= amount;
    }

    pub fn lock(&mut self) {
        self.locked = true;
    }
}

mod utils {
    use super::Amount;
    use serde::de::Error;
    use serde::{Deserialize, Deserializer};
    use serde::{Serialize, Serializer};

    impl<'de> Deserialize<'de> for Amount {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let value: f64 = Deserialize::deserialize(deserializer)?;
            let amount = Self::try_from_f64(value).map_err(|e| Error::custom(e.to_string()))?;
            Ok(amount)
        }
    }

    impl Serialize for Amount {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_f64(self.as_f64())
        }
    }
}
