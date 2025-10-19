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
    TxnState,
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
    // TODO: in case we decide tp use this logic on the server, we will
    // want to use a concurrent hash map and also make it available either
    // via the app's state, or globally
    let mut txns: HashMap<TxnID, TxnRecord> = HashMap::new();
    let mut accounts: HashMap<ClientID, Account> = HashMap::new();

    for result in csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(true)
        .from_reader(reader)
        .deserialize()
    {
        let record: Record = result?;
        match record.inner {
            RecordInner::TxnRecord(record) => {
                match record.kind {
                    TxnRecordKind::Deposit => {
                        if let Some(account) = accounts.get_mut(&record.client) {
                            if account.locked {
                                // we assume they cannot credit a locked account
                                continue;
                            }
                            account.deposit(record.amount);
                        } else {
                            let mut account = Account::new(record.client);
                            account.deposit(record.amount);
                            accounts.insert(record.client, account);
                        }
                    }
                    TxnRecordKind::Withdrawal => {
                        if let Some(account) = accounts.get_mut(&record.client) {
                            if account.locked {
                                // we assume they cannot debit a locked account
                                // (similar to the credit operation above)
                                continue;
                            }
                            // this operation is "fallible", but we are currently
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
                    // it (we can consider emitting a warning), so we just move on;
                    //
                    // further down this branch, we know by this time that we actually
                    // processed and stored the referenced transaction, hence we
                    // can `.expect` it as our invariant
                    continue;
                };
                match record.kind {
                    DisputeRecordKind::Dispute => {
                        if txn.state != TxnState::Undisputed {
                            // this transaction has already been disputed or even
                            // reversed, and so to guarantee idempotency, we simply
                            // move on to the next record
                            continue;
                        }
                        let account = accounts
                            .get_mut(&record.client)
                            .expect("account to have been created earlier for this client");
                        account.hold(txn.amount);
                        txn.state = TxnState::Disputed;
                    }
                    DisputeRecordKind::Resolve => {
                        if txn.state != TxnState::Disputed {
                            // this transaction has never been disputed in the
                            // first place or has already been reversed, and so
                            // we are moving on to the next record
                            continue;
                        }
                        let account = accounts
                            .get_mut(&record.client)
                            .expect("account to have been created earlier for this client");
                        account.resolve(txn.amount);
                        txn.state = TxnState::Undisputed;
                    }
                    DisputeRecordKind::ChargeBack => {
                        if txn.state != TxnState::Disputed {
                            // similar to `DisputeRecordKind::Resolve`, we can
                            // only act here if the transaction is under dipute
                            continue;
                        }
                        let account = accounts
                            .get_mut(&record.client)
                            .expect("account to have been created earlier for this client");
                        account.charge_back(txn.amount);
                        account.lock();
                        txn.state = TxnState::Reversed;
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

#[cfg(test)]
mod tests {
    use super::process;

    #[test]
    fn handles_malformed_input() {
        // let's check we are not panicking on some malformed inputs
        let cases = [
            (
                "\
                    wrong,      column,  names, provided\n\
                    deposit,    1,       1,     5.9999\n\
                ",
                "wrong column names",
            ),
            (
                "\
                    type,      client,  tx,     amount\n\
                    blocking,  1,       1,      5.9999\n\
                ",
                "wrong variant for record type",
            ),
            (
                "\
                    type,      client,  tx,     amount\n\
                    deposit,   1.0,     1,      5.9999\n\
                ",
                "wrong data type for client",
            ),
            (
                "\
                    type,      client,  tx,     amount\n\
                    deposit,   1,       1.0,    5.9999\n\
                ",
                "wrong data type for tx",
            ),
        ];
        for (case, msg) in cases {
            let writer = Vec::new();
            let result = process(case.as_bytes(), writer);
            assert!(result.is_err(), "{msg}");
        }
    }

    #[test]
    fn handles_emptiness() {
        let cases = [
            ("", "empty reader"),
            (
                "type, client, tx, amount\n",
                "header is valid, but no other rows",
            ),
        ];
        for (case, msg) in cases {
            let mut writer = Vec::new();
            let result = process(case.as_bytes(), &mut writer);
            assert!(result.is_ok(), "{msg}");
            assert!(
                csv::Reader::from_reader(writer.as_slice())
                    .records()
                    .collect::<Vec<_>>()
                    .is_empty()
            );
        }
    }

    #[test]
    fn handles_decimals_precision() {
        let cases = [
            (
                "\
                    type,      client,  tx,     amount\n\
                    deposit,   1,       1,      5\n\
                ",
                "amount w/o decimal point",
            ),
            (
                "\
                    type,      client,  tx,     amount\n\
                    deposit,   1,       1,      5.0\n\
                ",
                "amount with one place after point",
            ),
            (
                "\
                    type,      client,  tx,     amount\n\
                    deposit,   1,       1,      5.0000999\n\
                ",
                "amount with more than four places after point",
            ),
        ];
        for (case, msg) in cases {
            let mut writer = Vec::new();
            let result = process(case.as_bytes(), &mut writer);
            assert!(result.is_ok());
            assert_eq!(
                String::from_utf8(writer).unwrap(),
                "client,available,held,total,locked\n1,5.0,0.0,5.0,false\n",
                "{msg}"
            )
        }
    }

    #[test]
    fn handler_empty_amount() {
        let cases = [
            (
                "\
                    type,      client,  tx,     amount\n\
                    dispute,   1,       1,\n\
                ",
                "amount field is empty but preceeded by comma",
            ),
            // this case is also considered fine, since we are just ingoring
            // the amount field for dipute resolution operations
            (
                "\
                    type,         client,  tx,     amount\n\
                    chargeback,   1,       1,      whatever\n\
                ",
                "amount valid is not valid type for amount",
            ),
        ];
        for (case, msg) in cases {
            let mut writer = Vec::new();
            let result = process(case.as_bytes(), &mut writer);
            assert!(result.is_ok(), "{msg}");
            // those records alone do not much: we actually need some
            // debit and credit transactions to happen before, i.e. to have
            // something to dispute about
            assert!(writer.is_empty(), "{msg}");
        }

        let cases = [
            // amount is not required for dispute resolution operations, but formatting
            // should still be valid
            (
                "\
                    type,      client,  tx,     amount\n\
                    resolve,   1,       1\n\
                ",
                "amount field is empty and _not_ preceeded by comma",
            ),
            // amout is a required field for deposits ...
            (
                "\
                    type,      client,  tx,     amount\n\
                    deposit,   1,       1,\n\
                ",
                "amount not provided for deposit",
            ),
            // ... as well as for withdrawals
            (
                "\
                    type,      client,  tx,     amount\n\
                    withdrawal,   1,       1,\n\
                ",
                "amount not provided for withdrawal",
            ),
        ];
        for (case, msg) in cases {
            let writer = Vec::new();
            let result = process(case.as_bytes(), writer);
            assert!(result.is_err(), "{msg}");
        }
    }
}
