extern crate ctrlc;
extern crate nom;
extern crate nom_syslog;
extern crate pretty_bytes;

#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate hyper;
extern crate rusoto_core;
extern crate rusoto_kinesis;

extern crate env_logger;
#[macro_use]
extern crate log;

use std::{env, io, thread};
use std::error::Error;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::process::exit;
use std::time::Duration;

use nom_syslog::parse_syslog;
use nom::IResult;

use pretty_bytes::converter::convert;

use hyper::Client;
use rusoto_core::{default_tls_client, DefaultCredentialsProviderSync, Region};
use rusoto_kinesis::{Kinesis, KinesisClient, ListStreamsInput, PutRecordInput, PutRecordsInput, PutRecordsRequestEntry};

fn main() {
    env_logger::init().unwrap();

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        error!("must set action to 'server' or 'client'");
        exit(1)
    }

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        info!("shutting down syslog server");
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");

    debug!("now starting up stuff...");
    match &args[1][..] {
        "server" => {
            let mut server = SyslogServer::new(running.clone());
            match server.run() {
                Ok(_) => {
                    info!("server shutdown happily");
                }
                Err(err) => {
                    if err.description() == "not connected" {
                        info!("server shutdown happily");
                    } else {
                        error!("server shutdown was NOT happy: {:?}", err.description());
                    }
                }
            }
        }
        "client" => unimplemented!(),
        other => {
            error!("we don't handle {:?}, use 'server' or 'client'", other);
        }
    };

    info!("Waiting for Ctrl-C...");
    while running.load(Ordering::SeqCst) {}
    debug!("Got it! Exiting...");

    ()
}

// I don't want to leak ARNs in to public code, so this little ditty pulls the name out of AWS
fn get_kinesis_stream_name(thing: &DefaultKinesisClient) -> io::Result<String> {
    let streams_response = thing.list_streams(&ListStreamsInput {
        exclusive_start_stream_name: None,
        limit: None,
    });

    let streams = match streams_response {
        Ok(output) => output,
        Err(list_err) => {
            println!("oh no");
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "could not list Kinesis streams",
            ));
        }
    };

    if streams.stream_names.len() > 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "I can only auto-discover one Kinesis stream",
        ));
    }

    Ok(streams.stream_names[0].clone())
}

type DefaultKinesisClient = Arc<KinesisClient<DefaultCredentialsProviderSync, Client>>;

pub struct SyslogServer {
    pub running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub streams: Vec<TcpStream>,
    pub listener: Option<TcpListener>,
    pub kinesis_client: DefaultKinesisClient,
}

impl SyslogServer {
    fn new(running: Arc<AtomicBool>) -> Self {
        let k = KinesisClient::new(
            default_tls_client().unwrap(),
            DefaultCredentialsProviderSync::new().unwrap(),
            Region::UsWest2,
        );
        Self {
            running,
            streams: vec![],
            listener: None,
            kinesis_client: Arc::new(k),
        }
    }

    fn shutdown(&self) {
        //        self.listener
    }

    fn init_client(&mut self, stream: TcpStream) -> io::Result<SyslogClient> {
        let tracking_stream = stream.try_clone()?;
        self.streams.push(tracking_stream);

        Ok(SyslogClient::new(stream, self.running.clone()))
    }

    fn run(&mut self) -> io::Result<()> {
        let listener = TcpListener::bind("0.0.0.0:1516")?;
        listener.set_nonblocking(true)?;

        let stream_name = get_kinesis_stream_name(&self.kinesis_client)?;

        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_nonblocking(false)
                        .expect("Could not set nonblocking mode on client stream");
                    let mut client = match self.init_client(stream) {
                        Ok(c) => c,
                        Err(e) => {
                            error!("failed to spawn client {:?}", e);
                            continue;
                        }
                    };
                    let kclient_clone = self.kinesis_client.clone();
                    let stream_name_clone = stream_name.clone();
                    thread::spawn(move || {
                        info!("STARTING: thread for client {:?}", client);
                        let res = client.run(kclient_clone, stream_name_clone);
                        info!("STOPPING: thread for client ending with {:?}", res);
                    });
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
pub struct SyslogClient {
    stream: TcpStream,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    lines_read: usize,
    bytes_read: usize,
}

impl std::fmt::Debug for SyslogClient {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "stream={:?}", self.stream.peer_addr().unwrap())
    }
}

impl SyslogClient {
    fn new(stream: TcpStream, shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self {
            stream,
            shutdown,
            lines_read: 0,
            bytes_read: 0,
        }
    }

    fn shutdown(&self) -> io::Result<()> {
        self.stream.shutdown(Shutdown::Both)
    }

    fn run(&mut self, kinesis_client: DefaultKinesisClient, stream_name: String) -> Result<(), io::Error> {
        let addr = self.stream.peer_addr()?;
        let bufr = BufReader::with_capacity(4 * 1024, &self.stream);

        let capacity = 1000;
        let mut recs : Vec<PutRecordsRequestEntry> = Vec::with_capacity(capacity);

        let mut counter = 0;

        for maybe_line in bufr.lines() {
            let line: String = maybe_line?;
            self.bytes_read += line.len();
            self.lines_read += 1;
            let mut log = self.parse_syslog_line(&line[..])?;
            log.sender_ip = Some(addr.clone());

            let json_vecu8 = serde_json::to_vec(&log)?;

            let partition_key = format!("{}", counter);

            recs.push(
                PutRecordsRequestEntry{
                    data: json_vecu8,
                    explicit_hash_key: None,
                    partition_key: partition_key,
                }
            );

            if recs.len() >= capacity {
                let put_records_res = kinesis_client.put_records(&PutRecordsInput{
                    records: recs.clone(),
                    stream_name: stream_name.clone(),
                });

                recs.clear();
            }
        }

        info!(
            "{:?} done. {} bytes, {} lines",
            self,
            convert(self.bytes_read as f64),
            self.lines_read
        );
        Ok(())
    }

    fn parse_syslog_line<'a>(&self, line: &'a str) -> Result<Log<'a>, io::Error> {
        let res: IResult<&'a str, nom_syslog::Syslog3164Message<'a>> = parse_syslog(line);
        match res {
            IResult::Done(_, datum) => {
                // println!("datum: {:?}", datum);
                return Ok(Log {
                    app: String::new(),
                    sender_ip: None,
                    kv: None,
                    message: Some(datum.msg.into()),
                });
            }
            IResult::Incomplete(_) => panic!("incomplete!"),
            IResult::Error(e) => {
                println!("error: {:?}", e);
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "bad data"));
            }
        }
    }
}

pub trait LogProcessor {
    fn process(&self, string: &str) -> Result<Log, io::Error>;
}

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

#[derive(PartialEq, Debug, Serialize)]
pub struct Log<'a> {
    pub app: String,
    pub sender_ip: Option<std::net::SocketAddr>,
    pub kv: Option<Vec<Vec<String>>>,
    pub message: Option<&'a str>,
}