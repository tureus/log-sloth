use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

use serde::de;
use serde::de::{Deserialize, Deserializer, Visitor};
use serde_json;

#[derive(Deserialize, Debug, PartialEq)]
struct Fortigate {
    name: String,
    kv: FortigateKV,
}

#[derive(Debug, PartialEq)]
struct FortigateKV(HashMap<String, String>);

struct FortigateKVVisitor<K, V> {
    marker: PhantomData<fn() -> HashMap<K, V>>,
}

impl<K, V> FortigateKVVisitor<K, V> {
    fn new() -> Self {
        FortigateKVVisitor {
            marker: PhantomData,
        }
    }
}

impl<'de> Visitor<'de> for FortigateKVVisitor<String, String> where {
    type Value = HashMap<String, String>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a fortigate k=v map")
    }

    fn visit_str<E>(self, kvs: &str) -> Result<HashMap<String, String>, E>
    where
        E: de::Error,
    {
        let hash_map: HashMap<String, String> = kvs.split_whitespace()
            .map(|kv_part| kv_part.split("=").collect::<Vec<&str>>())
            .filter(|x| x.len() == 2)
            .map(|split_list| (split_list[0].to_owned(), split_list[1].to_owned()))
            .collect();

        Ok(hash_map)
    }
}

impl<'a> Deserialize<'a> for FortigateKV {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        Ok(FortigateKV(deserializer
            .deserialize_string(FortigateKVVisitor::new())?))
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
