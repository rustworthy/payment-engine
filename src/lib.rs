#[macro_use]
extern crate serde;

use std::{
    collections::HashMap,
    error::Error,
    io::{Read, Write},
};

mod domain;

use domain::{
    Account, ClientID, DisputeRecordKind, Record, RecordInner, TxnID, TxnRecord, TxnRecordKind,
};

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
    let mut txns: HashMap<TxnID, TxnRecord> = HashMap::new();
    let mut accounts: HashMap<ClientID, Account> = HashMap::new();

    for result in csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(reader)
        .deserialize()
    {
        let record: Record = result?;
        match record.inner {
            RecordInner::TxnRecord(record) => {
                match record.kind {
                    TxnRecordKind::Deposit => {
                        if let Some(account) = accounts.get_mut(&record.client) {
                            account.deposit(record.amount);
                        } else {
                            let mut account = Account::new(record.client);
                            account.deposit(record.amount);
                            accounts.insert(record.client, account);
                        }
                    }
                    TxnRecordKind::Withdrawal => {
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
                            let account = Account::new(record.client);
                            accounts.insert(record.client, account);
                        }
                    }
                }
                // this record may be referenced by one of the further dispute
                // resolution records (if any) so let's store it
                txns.insert(record.tx, record);
            }
            RecordInner::DisputeRecord(record) => {
                let Some(txn) = txns.get_mut(&record.tx) else {
                    // the `DisputeRecord` record is referencing a transaction which we
                    // never encountered before; there is not much we can do about
                    // it (we can consider emitting a warning), so we just move on
                    continue;
                };
                match record.kind {
                    DisputeRecordKind::Dispute => {
                        if txn.disputed {
                            // this transaction has already been disputed, and so
                            // to guarantee idempotency, we simply move on to the
                            // next record
                            continue;
                        }
                        if let Some(account) = accounts.get_mut(&record.client) {
                            account.hold(txn.amount);
                        } else {
                            // the `client` referenced in the `dispute` transaction is
                            // not in our records, so let's create it and move on;
                            let account = Account::new(record.client);
                            // TODO: consider if we should call `dispute` on the newly
                            // created account, and set txn to disputed
                            accounts.insert(record.client, account);
                        };
                        txn.disputed = true;
                    }
                    DisputeRecordKind::Resolve => {
                        if !txn.disputed {
                            continue;
                        }
                        if let Some(account) = accounts.get_mut(&record.client) {
                            account.resolve(txn.amount);
                        } else {
                            let account = Account::new(record.client);
                            accounts.insert(record.client, account);
                        };
                        txn.disputed = false;
                    }
                    DisputeRecordKind::ChargeBack => {
                        if !txn.disputed {
                            continue;
                        }
                        if let Some(account) = accounts.get_mut(&record.client) {
                            account.charge_back(txn.amount);
                        } else {
                            let account = Account::new(record.client);
                            accounts.insert(record.client, account);
                        };
                        txn.disputed = false;
                    }
                }
            }
        }
    }
    let mut wrt = csv::Writer::from_writer(writer);
    for account in accounts.values() {
        wrt.serialize(account)?;
    }
    wrt.flush()?;
    Ok(())
}
