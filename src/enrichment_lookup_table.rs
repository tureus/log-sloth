use std::fs::File;
use csv;
use indexmap::IndexMap;

pub struct CSVLookupTable {
    map: IndexMap<String,Vec<(String,String)>>
}

impl CSVLookupTable {
    pub fn get(&self, key: &String) -> Option<&Vec<(String,String)>> {
        self.map.get(key)
    }

    pub fn load(path: &str) -> Self {
        let mut reader = {
            let f = File::open(path).expect("wah");
            csv::Reader::from_reader(f)
        };

        let mut map : IndexMap<String,Vec<(String,String)>> = IndexMap::new();

        for line in reader.deserialize() {
            let record: LookupRecord = line.unwrap();

            if map.get(&record.0).is_none() {
                map.insert(record.0.clone(), Vec::with_capacity(2));
            }
            let mut entry = map.get_mut(&record.0).unwrap();
            entry.push((record.1, record.2));
        }

        Self {
            map
        }
    }
}

#[derive(Debug, Deserialize)]
struct LookupRecord(String, String, String);

#[test]
fn parse_it() {
    CSVLookupTable::load("ipam_lookup_v2.csv");
}