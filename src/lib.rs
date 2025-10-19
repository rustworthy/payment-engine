#[macro_use]
extern crate serde;

use std::{
    collections::HashMap,
    error::Error,
    io::{Read, Write},
};

mod domain;

use domain::{Account, ClientID, TxnKind, TxnRecord};

/// Process the records contained in the `reader` in CSV format.
///
/// Note how there are no timestamps on the precessed records for us to be
/// able to establish the order. Instead, we expect the transactions to have been
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
