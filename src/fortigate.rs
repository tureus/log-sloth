pub trait LogProcessor {
    fn process(&self, string: &str) -> Result<Log, io::Error>;
}

#[allow(dead_code)]
struct Fortigate {}

impl LogProcessor for Fortigate {
    fn process(&self, line: &str) -> Result<Log, io::Error> {
        debug!("processing {}", line);
        let table: Vec<Vec<String>> = line.split_whitespace()
            .map(|x| x.split('=').map(|y| y.to_string()).collect())
            .collect();
        Ok(Log {
            app: "fortigate".to_owned(),
            sender_ip: None,
            kv: Some(table),
            message: None,
        })
    }
}
