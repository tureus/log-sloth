use std::collections::HashMap;

use serde::de::{Deserialize, Deserializer};
use serde_json;

#[derive(Deserialize, Debug, PartialEq)]
struct Fortigate {
    name: String,
    kv: FortigateKV,
}

#[derive(Debug, PartialEq)]
struct FortigateKV(HashMap<String, String>);

impl<'a> Deserialize<'a> for FortigateKV {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(FortigateKV(raw.split_whitespace()
                           .map(|kv_part| kv_part.split("=").collect::<Vec<&str>>())
                           .filter(|x| x.len() == 2)
                           .map(|split_list| (split_list[0].to_owned(), split_list[1].to_owned()))
                           .collect()))
    }
}

#[test]
fn fortigate_parses() {
    let made_up_data = r##"{"name": "fortigate", "kv":"I=am a=map"}"##;
    let v: Fortigate = serde_json::from_str(made_up_data).unwrap();
    let mut hash_map: HashMap<String, String> = HashMap::new();
    hash_map.insert("I".into(), "am".into());
    hash_map.insert("a".into(), "map".into());

    assert_eq!(
        v,
        Fortigate {
            name: "fortigate".into(),
            kv: FortigateKV(hash_map),
        }
    )
}

#[test]
fn fortigate_parses_bad_kv() {
    let made_up_data = r##"{"name": "fortigate", "kv":"I=am bad data a=map and skipped"}"##;
    let v: Fortigate = serde_json::from_str(made_up_data).unwrap();
    let mut hash_map: HashMap<String, String> = HashMap::new();
    hash_map.insert("I".into(), "am".into());
    hash_map.insert("a".into(), "map".into());

    assert_eq!(
        v,
        Fortigate {
            name: "fortigate".into(),
            kv: FortigateKV(hash_map),
        }
    )
}
