use nom::IResult;

fn quote_delim(ch: char) -> bool {
    ch == '"'
}

fn space_delim(ch: char) -> bool {
    ch == ' '
}
named!(kv_value<&str, &str>,
        alt!(
            do_parse!(
                tag_s!("\"") >>
                s: take_till_s!(quote_delim) >>
                tag_s!("\"") >>
                (
                    s
                )
            ) |
            do_parse!(
                s: take_till_s!(space_delim) >>
                ({
                    s
                })
            )
        )
);

named!(pair<&str,(&str,&str)>,
    do_parse!(
        k: take_till_s!(equal_delim) >>
        tag_s!("=") >>
        v: kv_value >> (
            (k,v)
        )
    )
);

fn equal_delim(ch: char) -> bool {
    ch == '='
}
named!(
    kv<&str, Vec<(&str,&str)>>,
    many0!(ws!(pair))
);

pub fn extract_kv(input: &str) -> Option<Vec<(String, String)>> {
    match kv(input) {
        IResult::Done(_, datum) => {
            if datum.len() > 0 {
                Some(datum.into_iter().map(|(k,v)| (k.into(),v.into())).collect())
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn extract_kv_zc(input: &str) -> Option<Vec<(&str, &str)>> {
    match kv(input) {
        IResult::Done(_, datum) => {
            if datum.len() > 0 {
                Some(datum)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[test]
fn test_extract_kv_with_spaces_in_value() {
    let res: IResult<&str, &str> = kv_value("\"asdf is cool\"");
    assert!(res.is_done());
    let (_leftover, parsed) = res.unwrap();
    assert_eq!(parsed, "asdf is cool");
}

#[test]
fn test_extract_kv_without_spaces_in_value() {
    let res: IResult<&str, &str> = kv_value("you-will-work-right-???");
    assert!(res.is_done());
    let (_leftover, parsed) = res.unwrap();
    assert_eq!(parsed, "you-will-work-right-???");
}

#[test]
fn test_extract_kv_simple() {
    let res: IResult<&str, Vec<(&str, &str)>> = kv("a=b c=d");
    assert!(res.is_done());
    let (_leftover, parsed) = res.unwrap();
    assert_eq!(parsed, vec![("a", "b"), ("c", "d")]);
}

#[test]
fn test_extract_kv_fancy() {
    let res: IResult<&str, Vec<(&str, &str)>> = kv("a=\"b is your friend\" c=d");
    assert!(res.is_done());
    let (_leftover, parsed) = res.unwrap();
    assert_eq!(parsed, vec![("a", "b is your friend"), ("c", "d")]);
}

#[test]
fn test_extract_kv_hard() {
    let res: IResult<&str, Vec<(&str,&str)>> = kv("date=2018-02-23 time=20:21:47 logver=54 devname=\"NAE02-DCS-CORP-900D\" devid=\"FG900D3916800491\" vd=\"root\" date=2018-02-23 time=20:21:47 logid=\"0100000000\" type=\"event\" subtype=\"system\" level=\"notice\" logdesc=\"System performance statistics\" action=\"perf-stats\" cpu=0 mem=18 totalsession=3356 disk=1 bandwidth=\"133857/131838\" setuprate=0 disklograte=0 fazlograte=7 msg=\"Performance statistics: average CPU: 0, memory:  18, concurrent sessions:  3356, setup-rate: 0\"");
    assert!(res.is_done());
    let (_leftover, parsed) = res.unwrap();
    assert_eq!(parsed, vec![("date", "2018-02-23"), ("time", "20:21:47"), ("logver", "54"), ("devname", "NAE02-DCS-CORP-900D"), ("devid", "FG900D3916800491"), ("vd", "root"), ("date", "2018-02-23"), ("time", "20:21:47"), ("logid", "0100000000"), ("type", "event"), ("subtype", "system"), ("level", "notice"), ("logdesc", "System performance statistics"), ("action", "perf-stats"), ("cpu", "0"), ("mem", "18"), ("totalsession", "3356"), ("disk", "1"), ("bandwidth", "133857/131838"), ("setuprate", "0"), ("disklograte", "0"), ("fazlograte", "7"), ("msg", "Performance statistics: average CPU: 0, memory:  18, concurrent sessions:  3356, setup-rate: 0")]);
}

