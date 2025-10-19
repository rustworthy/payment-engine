use std::{error::Error, io::Read};

pub fn process<B>(reader: B) -> Result<(), Box<dyn Error>>
where
    B: Read,
{
    let mut rdr = csv::Reader::from_reader(reader);
    for result in rdr.records() {
        let record = result?;
        println!("{:?}", record);
    }
    Ok(())
}
