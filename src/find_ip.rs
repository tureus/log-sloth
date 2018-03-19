use nom::{digit, rest_s, space, anychar};

named!(ip<&str,&str>,
  recognize!(
    do_parse!(
      digit >>
      tag_s!(".") >>
      digit >>
      tag_s!(".") >>
      digit >>
      tag_s!(".") >>
      digit >>
      (
        ""
      )
    )
  )
);

named!(extract_ips<&str,Vec<(&str)>>,
  do_parse!(
      list: many0!(
        alt!(
            do_parse!(
                s: ip >>
                (
                    Some(s)
                )
            ) |
            do_parse!(
                anychar >>
                (
                    None
                )
            )
          )
        ) >> ({
            list.into_iter().filter_map(|entry| {
              match entry {
                None => None,
                other => other
              }
            }).collect()
        })
  )
);

#[test]
fn parse_vcisco_1() {
    use nom::IResult;
    let msg1 = r##"a1.2.3.4a3.14a5.6.7.8"##;
    let res: IResult<&str, Vec<&str>> = extract_ips(msg1);
    assert!(res.is_done());

    let (_,opt) : (_,Vec<&str>) = res.unwrap();
    assert_eq!(opt, vec!["1.2.3.4","5.6.7.8"] );
}

#[test]
fn parse_vcisco_2() {
    use nom::IResult;
    let msg1 = r##"<189>2017.12.26T23:33:18+00:00 10.4.104.208 3890188: Dec 26 2017 23:33:17.792 UTC: %ADJ-5-RESOLVE_REQ_FAIL: Adj resolve request failed for 192.168.57.182 on Vlan55"##;
    let res: IResult<&str, Vec<&str>> = extract_ips(msg1);
    assert!(res.is_done());

    let (_,opt) = res.unwrap();
    assert_eq!(opt, vec!["10.4.104.208","192.168.57.182"]);
}