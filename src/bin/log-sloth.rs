extern crate docopt;

extern crate ctrlc;
extern crate nom;
extern crate nom_syslog;
extern crate pretty_bytes;

#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate futures;
extern crate rusoto_core;
extern crate rusoto_kinesis;

extern crate prctl;

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

use docopt::Docopt;

use nom_syslog::parse_syslog;
use nom::IResult;

use pretty_bytes::converter::convert;

use rusoto_core::Region;
use rusoto_core::reactor::{CredentialsProvider, RequestDispatcher};
use rusoto_kinesis::{Kinesis, KinesisClient, ListStreamsInput, PutRecordsInput,
                     PutRecordsRequestEntry, PutRecordsOutput, PutRecordsError};

const USAGE: &'static str = "
log-sloth.

Usage:
  log-sloth --concurrency=N
  log-sloth (-h | --help)
  log-sloth --version

Options:
  -h --help     Show this screen.
  --version     Show version.
  --concurrency=<kn>  connections to Kinesis per client [default: 10]
";

#[derive(Debug, Deserialize)]
struct Args {
    pub flag_concurrency: usize,
}

fn main() {
    #[cfg(linux)]
        prctl::set_name("log-sloth main thread").unwrap();

    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    env_logger::init();

    if env::var("USER").unwrap() == "root"
        && (env::var("AWS_ACCESS_KEY_ID").is_err() || env::var("AWS_SECRET_ACCESS_KEY").is_err())
        {
            error!("if running as root, must set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY");
            std::process::exit(1);
        }

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        info!("shutting down syslog server");
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");

    let mut server = SyslogServer::new(running.clone(), args.flag_concurrency);
    server.run().expect("syslog server died");

    info!("Waiting for Ctrl-C...");
    while running.load(Ordering::SeqCst) {}
    info!("Received Ctrl-C. Exiting...");

    ()
}

// I don't want to leak ARNs in to public code, so this little ditty pulls the name out of AWS
fn get_kinesis_stream_name(thing: &DefaultKinesisClient) -> io::Result<String> {
    let streams_response = thing.list_streams(&ListStreamsInput {
        exclusive_start_stream_name: None,
        limit: None,
    });

    let streams = match streams_response.sync() {
        Ok(output) => output,
        Err(_) => {
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

type DefaultKinesisClient = Arc<KinesisClient<CredentialsProvider, RequestDispatcher>>;

pub struct SyslogServer {
    pub running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub streams: Vec<TcpStream>,
    pub listener: Option<TcpListener>,
    pub kinesis_client: DefaultKinesisClient,
    pub concurrency: usize,
}

impl SyslogServer {
    fn new(running: Arc<AtomicBool>, concurrency: usize) -> Self {
        let kinesis = KinesisClient::simple(Region::UsWest2);

        Self {
            running,
            streams: vec![],
            listener: None,
            kinesis_client: Arc::new(kinesis),
            concurrency: concurrency,
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
        info!("starting log sloth server");
        info!("concurrency={}", self.concurrency);
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

                    let stream_name = stream_name.clone();
                    let kinesis_client = self.kinesis_client.clone();
                    let inflight = self.concurrency;
                    thread::spawn(move || {
                        let tx = kinesis_tx(kinesis_client, stream_name, 1, inflight);

                        info!("STARTING: thread for client {:?}", client);
                        #[cfg(linux)]
                            {
                                let name = format!(
                                    "log-sloth client thread ({:?})",
                                    client.stream.peer_addr()
                                );
                                prctl::set_name(&name[..]).unwrap();
                            }
                        let res = client.run(tx.clone());
                        info!("STOPPING: thread for client ending with {:?}", res);
                    });
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(ref e) => {
                    error!("not sure how to handle {:?}", e);
                }
            }

            if !self.running.load(Ordering::SeqCst) {
                break;
            }
        }

        Ok(())
    }
}

type RecordsChannel = futures::sync::mpsc::Sender<rusoto_kinesis::PutRecordsRequestEntry>;

pub fn kinesis_tx(kinesis_client: DefaultKinesisClient, stream_name: String, chan_buf: usize, inflight: usize) -> RecordsChannel {
    use futures::sync::mpsc::{channel, spawn};
    use futures::{Future, Sink, Stream};
    use futures::stream::Sender;

    let (mut tx, mut rx) = channel(chan_buf);
    let client = Arc::new(KinesisClient::simple(Region::UsWest2));

    let mut retry_tx = tx.clone().wait();
    std::thread::spawn(move || {
        let puts = rx.chunks(500).map(|batch: Vec<PutRecordsRequestEntry>| {
            let input = PutRecordsInput {
                records: batch,
                stream_name: stream_name.clone(),
            };
            //  : futures::Then<rusoto_core::RusotoFuture<PutRecordsOutput,(PutRecordsError,usize)>, Result<(usize,usize),(usize,usize)>, _>
            let chain = client.put_records(&input).then(|put_res| {
                let retval : Result<Result<PutRecordsOutput,(PutRecordsError,PutRecordsInput)>,()> = match put_res {
                    Ok(res) => {
                        trace!("match put_res: it worked");
                        Ok(Ok(res))
                    },
                    Err(err) => {
                        trace!("match put_res: failed");
                        Ok(Err((err,input)))
                    }
                };
                retval
            });
            chain
        }).buffer_unordered(inflight);

        for put_res in puts.wait() {
            match put_res {
                Ok(Ok(put)) => {
                    if let Some(failed) = put.failed_record_count {
                        if failed > 0 {
                            error!("{} record(s) failed to commit to kinesis", failed);
                            let put: PutRecordsOutput = put;
                            for rec in put.records {
                                if rec.error_code.is_some() {
                                    error!("failed record: {:?}", rec);
                                }
                            }
                        }
                    }
                },
                Ok(Err((put_records_err,put_records_input))) => {
                    match put_records_err {
                        PutRecordsError::HttpDispatch(dispatch_err) => {
                            error!("http dispatch error: {:?}. retrying records...", dispatch_err);
                            for record in put_records_input.records {
                                match retry_tx.send(record) {
                                    Ok(()) => {
                                        debug!("Wait#send succeeded")
                                    },
                                    Err(e) => {
                                        error!("Wait#send error {:?}", e)
                                    }
                                }
                            }
                        },
                        PutRecordsError::Unknown(raw_message) => {
                            error!("unknown error: '{:?}', retrying records...", raw_message);
                            for record in put_records_input.records {
                                match retry_tx.send(record) {
                                    Ok(()) => {
                                        debug!("Wait#send succeeded")
                                    },
                                    Err(e) => {
                                        error!("Wait#send error {:?}", e)
                                    }
                                }
                            }
                        }
                        other => {
                            error!("unhandled kinesis error: {:?}", other);
                        }
                    }
                },
                other => {
                    error!("puts.wait() fallthrough: {:?}", other);
                }

            }
        }
    });

    return tx
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

    fn run(
        &mut self,
        kinesis_stream: RecordsChannel,
    ) -> Result<(), io::Error> {
        let mut kinesis_stream = kinesis_stream;
        let addr = self.stream.peer_addr()?;
        let bufr = BufReader::with_capacity(4 * 1024, &self.stream);

        let capacity = 500;
        let mut recs: Vec<PutRecordsRequestEntry> = Vec::with_capacity(capacity);

        let mut counter = 0;

        let writer_threads = 10;
        use futures::Sink;
        let mut kinesis_wait = kinesis_stream.wait();

        for maybe_line in bufr.lines() {
            let line: String = match maybe_line {
                Ok(l) => l,
                Err(e) => {
                    error!("error while reading lines. breaking out. got {:?}", e);
                    break;
                }
            };
            self.bytes_read += line.len();
            self.lines_read += 1;
            let mut log = self.parse_syslog_line(&line[..])?;
            log.sender_ip = Some(addr.clone());

            let json_vecu8 = serde_json::to_vec(&log)?;

            let partition_key = format!("{}", counter);
            counter += 1;

            let record = PutRecordsRequestEntry {
                data: json_vecu8.clone(),
                explicit_hash_key: None,
                partition_key: partition_key.clone(),
            };
            use futures::{ Sink, Future, AsyncSink };


            match kinesis_wait.send(record) {
                 Ok(()) => {
                     debug!("Wait#send succeeded")
                 },
                Err(e) => {
                    error!("Wait#send error {:?}", e)
                }
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
                return Ok(Log {
                    app: String::new(),
                    sender_ip: None,
                    kv: None,
                    message: Some(datum.msg.into()),
                });
            }
            IResult::Incomplete(a) => {
                error!("incomplete: {:?}", a);
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "incomplete parse"))
            },
            IResult::Error(e) => {
                error!("error: {:?}", e);
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
