const USAGE_HINT: &str = r#"
    Usage:

    $cargo run -- transactions.csv > accounts.csv
"#;

fn main() {
    // TODO: consider using `clap` if we are going to support
    // extra arguments/flags (e.g. configurable custom separator in the csv file,
    // or "invalid" transactions handling mode, i.e. whether to silently skip vs fail
    let mut args = std::env::args();
    let _binname = args.next();
    let Some(filename) = args.next() else {
        eprintln!("CSV filename expected.\n{USAGE_HINT}",);
        std::process::exit(1);
    };
    let Ok(file) = std::fs::File::open(&filename) else {
        eprintln!("Please make sure file \"{filename}\" exists.\n{USAGE_HINT}");
        std::process::exit(1);
    };

    let reader = std::io::BufReader::new(file);
    let writer = std::io::BufWriter::new(std::io::stdout());
    if let Err(err) = payment_engine::process(reader, writer) {
        eprintln!("Processing error: {}", err);
        std::process::exit(1);
    }
}
