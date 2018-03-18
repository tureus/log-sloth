extern crate log_sloth;

use log_sloth::CSVLookupTable;

fn main() {
    let map = CSVLookupTable::load("ipam_lookup_v2.csv");
    let entry = map.get(&"123".into());
    println!("entry: {:?}", entry);
}