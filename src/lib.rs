extern crate hostname;
#[macro_use]
extern crate log;

extern crate futures;
extern crate hyper;
extern crate tokio_core;

pub mod stats;

#[allow(dead_code)]
fn extract_kv(input: &str) -> Vec<Vec<String>> {
    input
        .split_whitespace()
        .map(|x| x.split('=').map(|y| y.to_string()).collect())
        .collect()
}

#[test]
fn test_extract_kv() {
    let res = extract_kv("a=b c=d e=f g=h");
    assert_eq!(
        res,
        vec![
            vec!["a".into(), "b".into()],
            vec!["c".into(), "d".into()],
            vec!["e".into(), "f".into()],
            vec!["g".into(), "h".into()],
        ]
    )
}
//
//#[test]
//fn fortigate_parses_bad_kv() {
//    let f = Fortigate {};
//    let res = { f.process("a=b I AM BAD c=d") };
//    assert_eq!(
//        res.unwrap(),
//        Log {
//            app: "fortigate".to_owned(),
//            sender_ip: None,
//            kv: Some(vec![
//                vec!["a".into(), "b".into()],
//                vec!["I".into()],
//                vec!["AM".into()],
//                vec!["BAD".into()],
//                vec!["c".into(), "d".into()],
//            ]),
//            message: None,
//        }
//    )
//}
