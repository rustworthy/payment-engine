
### Payment Engine

This is a simplistic payment processing tool that can read in a series of
operations (currently via CLI), process them and spit out the accounts info.

The tool expects a file in CSV format with the header row containing
`type` (for operation type), `client` (ID of the account, globally unique),
`tx` (ID of the transaction, globally unique), and `amount` (with up to 4 decimal
places precision). Extra white-spaces are allowed and will be trimmed.

Example of the input file content:

```csv
type,       client,  tx,  amount
deposit,    1,       1,   5.9999
deposit,    2,       2,   200.0
deposit,    1,       3,   2.9999
withdrawal, 2,       4,   150.0
deposit,    3,       5,   100.0
dispute,    3,       5,
resolve,    3,       5,
deposit,    4,       6,   100
dispute,    4,       6,
chargeback, 4,       6,
```

Example output (written to stdout):

```csv
client,available,held,total,locked
4,0.0,0.0,0.0,true
3,100.0,0.0,100.0,false
1,8.9997,0.0,8.9997,false
2,50.0,0.0,50.0,false

```

Provided your input file is called `transactions.csv` and is located in projects
root directory, hit:

```bash
cargo run --release -- transactions.csv > accounts.csv
```

Please find further details and assumption we are making in the docs and comments
to the `process` procedure, that the [library](./src/lib.rs) crate of the projects
is exposing as well as in the co-located test suite.
