extern crate ctrlc;
extern crate rusoto_core;
extern crate rusoto_kinesis;

use std::{env, io, thread};
use std::error::Error;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::process::exit;
use std::time::Duration;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("must set action to 'server' or 'client'");
        exit(1)
    }

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("shutting down syslog server");
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");

    println!("now starting up stuff...");
    match &args[1][..] {
        "server" => {
            let mut server = SyslogServer::new(running.clone());
            match server.run() {
                Ok(_) => {
                    println!("server shutdown happily");
                }
                Err(err) => {
                    if err.description() == "not connected" {
                        println!("server shutdown happily");
                    } else {
                        println!("server shutdown was NOT happy: {:?}", err.description());
                    }
                }
            }

            println!("Waiting for Ctrl-C...");
            while running.load(Ordering::SeqCst) {}
            println!("Got it! Exiting...");
        }
        "client" => unimplemented!(),
        "kinesis" => kinesis_stuff(),
        other => {
            println!("we don't handle {:?}, use 'server' or 'client'", other);
        }
    };

    ()
}

pub fn kinesis_stuff() {
    use rusoto_core::{default_tls_client, DefaultCredentialsProvider, Region};
    use rusoto_kinesis::{Kinesis, KinesisClient, ListStreamsInput};

    let stream_arn = "arn:aws:kinesis:us-west-2:455567940957:stream/itsecmon-logs";

    let client = KinesisClient::new(
        default_tls_client().unwrap(),
        DefaultCredentialsProvider::new().unwrap(),
        Region::UsWest2,
    );

    let streams = client.list_streams(&ListStreamsInput{
        exclusive_start_stream_name: None,
        limit: None,
    });
    println!("streams: {:?}", streams);
}

pub struct SyslogServer {
    pub running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub streams: Vec<TcpStream>,
    pub listener: Option<TcpListener>,
}

impl SyslogServer {
    fn new(running: Arc<AtomicBool>) -> Self {
        Self {
            running,
            streams: vec![],
            listener: None,
        }
    }

    fn shutdown(&self) {
        //        self.listener
    }

    fn handle_client(&mut self, stream: TcpStream) -> io::Result<()> {
        let tracking_stream = stream.try_clone()?;
        self.streams.push(tracking_stream);

        let syslog_stream = SyslogStream::new(stream, self.running.clone());
        syslog_stream.handle_client()?;

        Ok(())
    }

    fn run(&mut self) -> io::Result<()> {
        let listener = TcpListener::bind("0.0.0.0:1516")?;
        listener.set_nonblocking(true)?;

        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    self.handle_client(stream)?;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(ref e) => {
                    println!("not sure how to handle {:?}", e);
                }
            }

            if !self.running.load(Ordering::SeqCst) {
                break;
            }
        }

        Ok(())
    }
}

//#[derive(Clone)]
pub struct SyslogStream {
    stream: TcpStream,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl SyslogStream {
    fn new(stream: TcpStream, shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { stream, shutdown }
    }

    fn clone(&self) -> io::Result<Self> {
        let stream_copy = self.stream.try_clone()?;
        Ok(Self {
            stream: stream_copy,
            shutdown: self.shutdown.clone(),
        })
    }

    fn shutdown(&self) -> io::Result<()> {
        self.stream.shutdown(Shutdown::Both)
    }

    fn handle_client(&self) -> Result<(), io::Error> {
        let addr = self.stream.peer_addr()?;
        let bufr = BufReader::with_capacity(4 * 1024, &self.stream);
        for line in bufr.lines() {
            let mut log = self.handle_line(line?)?;
            log.sender_ip = Some(addr.clone());
            println!("log: {:?}", log);
        }
        Ok(())
    }

    fn handle_line(&self, line: String) -> Result<Log, io::Error> {
        let f = Fortigate {};
        f.process(&line[..])
    }
}

pub trait LogProcessor {
    fn process(&self, string: &str) -> Result<Log, io::Error>;
}

struct Fortigate {}

impl LogProcessor for Fortigate {
    fn process(&self, line: &str) -> Result<Log, io::Error> {
        // println!("sup {}", string);
        let table: Vec<Vec<String>> = line.split_whitespace()
            .map(|x| x.split('=').map(|y| y.to_string()).collect())
            .collect();
        // Err(Error::new(ErrorKind::InvalidData, "bad line".to_string()))
        Ok(Log {
            app: "fortigate".to_owned(),
            sender_ip: None,
            kv: Some(table),
            message: None,
        })
    }
}

#[derive(PartialEq, Debug)]
pub struct Log {
    pub app: String,
    pub sender_ip: Option<std::net::SocketAddr>,
    pub kv: Option<Vec<Vec<String>>>,
    pub message: Option<String>,
}

#[test]
fn fortigate_parses() {
    let f = Fortigate {};
    let res = { f.process("a=b c=d e=f g=h") };
    assert_eq!(
        res.unwrap(),
        Log {
            app: "fortigate".to_owned(),
            sender_ip: None,
            kv: Some(vec![
                vec!["a".into(), "b".into()],
                vec!["c".into(), "d".into()],
                vec!["e".into(), "f".into()],
                vec!["g".into(), "h".into()],
            ]),
            message: None,
        }
    )
}

#[test]
fn fortigate_parses_bad_kv() {
    let f = Fortigate {};
    let res = { f.process("a=b I AM BAD c=d") };
    assert_eq!(
        res.unwrap(),
        Log {
            app: "fortigate".to_owned(),
            sender_ip: None,
            kv: Some(vec![
                vec!["a".into(), "b".into()],
                vec!["I".into()],
                vec!["AM".into()],
                vec!["BAD".into()],
                vec!["c".into(), "d".into()],
            ]),
            message: None,
        }
    )
}
