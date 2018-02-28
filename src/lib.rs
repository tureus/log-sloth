extern crate hostname;
#[macro_use]
extern crate log;

extern crate futures;
extern crate hyper;
extern crate tokio_core;

extern crate time;

extern crate prctl;

#[macro_use]
extern crate nom;

pub mod stats;
pub mod fortigate_kv;

#[cfg(target_os = "linux")]
pub fn rename_thread(input: &str) {
    prctl::set_name(input).unwrap();
}
#[cfg(not(target_os = "linux"))]
pub fn rename_thread(_: &str) {
}

pub fn extract_kv(input: &str) -> Vec<Vec<String>> {
    input
        .split_whitespace()
        .map(|x| {
            x.split('=')
                .map(|y: &str| str::replace(y, "\"", "").to_string())
                .collect()
        })
        .collect()
}

#[test]
fn test_extract_kv() {
    let res: Vec<Vec<String>> = extract_kv("a=b c=d e=f g=h");
    let expected: Vec<Vec<String>> = vec![
        vec!["a".into(), "b".into()],
        vec!["c".into(), "d".into()],
        vec!["e".into(), "f".into()],
        vec!["g".into(), "h".into()],
    ];
    assert_eq!(res, expected)
}

#[test]
fn fortigate_parses_remove_kv() {
    let res : Vec<Vec<String>> = extract_kv(
        r##"date=2018-02-21 time=02:46:53 logver=54 devname="VINC-INTRANET-600D" devid="FGT6HD3916801675" vd="servers" date=2018-02-21 time=02:46:55 logid="0000000013" type="traffic" subtype="forward" level="notice" srcip=172.30.148.11 srcport=57789 srcintf="outside" dstip=10.31.3.226 dstport=53 dstintf="dc" poluuid="78023d9e-aed0-51e7-a6c7-27d388c2a131" sessionid=417662316 proto=17 action="accept" policyid=1073741834 policytype="policy" dstcountry="Reserved" srccountry="Reserved" trandisp="noop" service="gDNS" duration=180 sentbyte=57 rcvdbyte=189 sentpkt=1 rcvdpkt=1 appcat="unscanned""##);
    let expected: Vec<Vec<String>> = vec![
        vec!["date".into(), "2018-02-21".into()],
        vec!["time".into(), "02:46:53".into()],
        vec!["logver".into(), "54".into()],
        vec!["devname".into(), r#"VINC-INTRANET-600D"#.into()],
        vec!["devid".into(), r#"FGT6HD3916801675"#.into()],
        vec!["vd".into(), r#"servers"#.into()],
        vec!["date".into(), r#"2018-02-21"#.into()],
        vec!["time".into(), r#"02:46:55"#.into()],
        vec!["logid".into(), r#"0000000013"#.into()],
        vec!["type".into(), r#"traffic"#.into()],
        vec!["subtype".into(), r#"forward"#.into()],
        vec!["level".into(), r#"notice"#.into()],
        vec!["srcip".into(), r#"172.30.148.11"#.into()],
        vec!["srcport".into(), r#"57789"#.into()],
        vec!["srcintf".into(), r#"outside"#.into()],
        vec!["dstip".into(), r#"10.31.3.226"#.into()],
        vec!["dstport".into(), r#"53"#.into()],
        vec!["dstintf".into(), r#"dc"#.into()],
        vec![
            "poluuid".into(),
            r#"78023d9e-aed0-51e7-a6c7-27d388c2a131"#.into(),
        ],
        vec!["sessionid".into(), r#"417662316"#.into()],
        vec!["proto".into(), r#"17"#.into()],
        vec!["action".into(), r#"accept"#.into()],
        vec!["policyid".into(), r#"1073741834"#.into()],
        vec!["policytype".into(), r#"policy"#.into()],
        vec!["dstcountry".into(), r#"Reserved"#.into()],
        vec!["srccountry".into(), r#"Reserved"#.into()],
        vec!["trandisp".into(), r#"noop"#.into()],
        vec!["service".into(), r#"gDNS"#.into()],
        vec!["duration".into(), r#"180"#.into()],
        vec!["sentbyte".into(), r#"57"#.into()],
        vec!["rcvdbyte".into(), r#"189"#.into()],
        vec!["sentpkt".into(), r#"1"#.into()],
        vec!["rcvdpkt".into(), r#"1"#.into()],
        vec!["appcat".into(), r#"unscanned"#.into()],
    ];
    assert_eq!(res, expected)
}
