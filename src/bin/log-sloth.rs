extern crate docopt;

extern crate ctrlc;
extern crate nom;
extern crate nom_syslog;
extern crate pretty_bytes;

#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate futures;
extern crate openssl_probe;
extern crate rusoto_core;
extern crate rusoto_kinesis;

extern crate env_logger;
#[macro_use]
extern crate log;

extern crate time;

extern crate log_sloth;

use std::{env, io, thread};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use docopt::Docopt;

use nom_syslog::parse_syslog;
use nom::IResult;

use pretty_bytes::converter::convert;

use rusoto_core::Region;
use rusoto_core::reactor::{CredentialsProvider, RequestDispatcher};
use rusoto_kinesis::{Kinesis, KinesisClient, ListStreamsInput, PutRecordsError, PutRecordsInput,
                     PutRecordsOutput, PutRecordsRequestEntry};

use log_sloth::stats::Stats;
use log_sloth::fortigate_kv::extract_kv;
use log_sloth::rename_thread;

const USAGE: &str = "
log-sloth.

Usage:
  log-sloth [--concurrency=N] [--disable-retry] [--enable-stats [--influxdb-url=URL] [--stats-interval=SEC]]
  log-sloth (-h | --help)
  log-sloth --version

Options:
  -h --help             Show this screen.
  --version             Show version.
  --concurrency=<kn>    Connections to Kinesis per client [default: 10]
  --enable-stats        Send stats to InfluxDB backend
  --influxdb-url=<url>  Target InfluxDB server [default: http://127.0.0.1:8086/write?db=telegraf]
  --stats-interval=<s>  Stats interval in seconds [default: 15]
  --disable-retry       Skip the feedback loop for retrying failed requests
";

#[derive(Debug, Deserialize)]
struct Args {
    pub flag_concurrency: usize,
    pub flag_influxdb_url: String,
    pub flag_stats_interval: u64,
    pub flag_enable_stats: bool,
    pub flag_disable_retry: bool,
}

fn main() {
    openssl_probe::init_ssl_cert_env_vars();

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

    let server_running = running.clone();
    let server = thread::spawn(move || {
        rename_thread("acceptor");

        let mut server = SyslogServer::new(server_running.clone(), args.flag_concurrency);
        server.run(args).expect("syslog server died");
    });

    rename_thread("main");
    info!("Waiting for Ctrl-C...");
    while running.load(Ordering::SeqCst) {
        thread::sleep(std::time::Duration::from_millis(500));
    }
    info!("Received Ctrl-C. Exiting...");

    server.join().unwrap();
}

// I don't want to leak ARNs in to public code, so this little ditty pulls the name out of AWS
fn get_kinesis_stream_name(thing: &DefaultKinesisClient) -> io::Result<String> {
    let streams_response = thing.list_streams(&ListStreamsInput {
        exclusive_start_stream_name: None,
        limit: None,
    });

    let streams = match streams_response.sync() {
        Ok(output) => output,
        Err(e) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("could not list Kinesis streams: {:?}", e),
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

    #[allow(dead_code)]
    fn shutdown(&self) {
        //        self.listener
    }

    fn run(&mut self, args: Args) -> io::Result<()> {
        info!("starting log sloth server");
        info!("concurrency={}", self.concurrency);
        let listener = TcpListener::bind("0.0.0.0:1516")?;
        listener.set_nonblocking(true)?;

        let stream_name = get_kinesis_stream_name(&self.kinesis_client)?;
        let disable_retry = args.flag_disable_retry;

        let stats = if args.flag_enable_stats {
            let (stats, _stats_thread) =
                Stats::spawn_thread(args.flag_influxdb_url, args.flag_stats_interval);
            Some(stats)
        } else {
            None
        };

        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_nonblocking(false)
                        .expect("Could not set nonblocking mode on client stream");
                    //                    let tracking_stream = stream.try_clone().expect("could not clone stream");
                    //                    self.streams.push(tracking_stream);

                    let mut client = SyslogClient::new(stream, self.running.clone());

                    let stream_name = stream_name.clone();
                    let inflight = self.concurrency;
                    let stats = stats.clone();
                    let kinesis_client = self.kinesis_client.clone();
                    thread::spawn(move || {
                        let tx = kinesis_tx(
                            kinesis_client,
                            stream_name.clone(),
                            1,
                            inflight,
                            disable_retry,
                            stats.clone(),
                        );

                        debug!("STARTING: thread for client {:?}", client);
                        rename_thread(&format!("{:?}", client.stream.peer_addr().unwrap()));
                        let res = client.run(tx.clone(), stats);
                        if res.is_err() {
                            error!("STOPPING: thread for client ending with {:?}", res);
                        }
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

pub fn kinesis_tx(
    client: DefaultKinesisClient,
    stream_name: String,
    chan_buf: usize,
    inflight: usize,
    disable_retry: bool,
    stats: Option<Arc<Stats>>,
) -> RecordsChannel {
    debug!("CREATING kinesis channel");

    use futures::sync::mpsc::channel;
    use futures::{Future, Sink, Stream};

    let (tx, rx) = channel(chan_buf);

    let mut retry_tx = if disable_retry {
        None
    } else {
        Some(tx.clone().wait())
    };

    std::thread::spawn(move || {
        debug!("STARTING kinesis batch put thread");
        rename_thread(&format!("k rx {}", time::now().tm_sec));
        let puts = rx.chunks(500)
            .map(|batch: Vec<PutRecordsRequestEntry>| {
                let input = PutRecordsInput {
                    records: batch,
                    stream_name: stream_name.clone(),
                };

                if let Some(ref s) = stats {
                    s.kinesis_inflight.fetch_add(1, Ordering::Relaxed);
                }
                client.put_records(&input).then(|put_res| {
                    if let Some(ref s) = stats {
                        s.kinesis_inflight.fetch_sub(1, Ordering::Relaxed);
                    }
                    match put_res {
                        Ok(res) => {
                            trace!("match put_res: it worked");
                            Ok(Ok(res))
                        }
                        Err(err) => {
                            trace!("match put_res: failed");
                            Ok(Err((err, input)))
                        }
                    }
                }
                )
            })
            .buffer_unordered(inflight);

        for put_res in puts.wait() {
            match put_res {
                Ok(Ok(put)) => {
                    if let Some(failed) = put.failed_record_count {
                        if failed > 0 {
                            if let Some(ref s) = stats {
                                s.kinesis_failures
                                    .fetch_add(failed as usize, Ordering::Relaxed);
                            }
                            error!("{} record(s) failed to commit to kinesis. not sure what to do. dropping them. printed below:", failed);
                            let put: PutRecordsOutput = put;
                            for rec in put.records {
                                if rec.error_code.is_some() {
                                    error!("failed record: {:?}", rec);
                                }
                            }
                        }
                    }
                }
                Ok(Err((put_records_err, put_records_input))) => match put_records_err {
                    PutRecordsError::HttpDispatch(dispatch_err) => {
                        error!(
                            "http dispatch error: {:?}. retrying records...",
                            dispatch_err
                        );
                        for record in put_records_input.records {
                            if let Some(ref mut retry_tx) = retry_tx {
                                retry_tx
                                    .send(record)
                                    .unwrap_or_else(|r| error!("Wait#send error {:?}", r));
                            }
                        }
                    }
                    PutRecordsError::Unknown(raw_message) => {
                        error!("unknown error: '{:?}', retrying records...", raw_message);
                        for record in put_records_input.records {
                            if let Some(ref mut retry_tx) = retry_tx {
                                retry_tx
                                    .send(record)
                                    .unwrap_or_else(|r| error!("Wait#send error {:?}", r));
                            }
                        }
                    }
                    other => error!("unhandled kinesis error: {:?}", other),
                },
                other => error!("puts.wait() fallthrough: {:?}", other),
            }
        }
        debug!("STOPPING kinesis batch put thread");
    });

    tx
}

//#[derive(Clone)]
pub struct SyslogClient {
    stream: TcpStream,
    #[allow(dead_code)]
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

    #[allow(dead_code)]
    fn shutdown(&self) -> io::Result<()> {
        self.stream.shutdown(Shutdown::Both)
    }

    fn run(
        &mut self,
        kinesis_stream: RecordsChannel,
        stats: Option<Arc<Stats>>,
    ) -> Result<(), io::Error> {
        let kinesis_stream = kinesis_stream;
        let addr = self.stream.peer_addr()?;
        let bufr = BufReader::with_capacity(4 * 1024, &self.stream);

        use futures::Sink;
        let mut kinesis_wait = kinesis_stream.wait();

        for (counter, maybe_line) in bufr.lines().enumerate() {
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
            log.sender_ip = Some(addr);
            log.kv = extract_kv(log.message.unwrap());
            debug!("log: {:?}", log);

            let mut json_vecu8 = serde_json::to_vec(&log)?;
            json_vecu8.push('\n' as u8);

            // Only count clients who send at least 1 message. This stops counting ELB health checks.
            if let Some(ref stats) = stats {
                if self.lines_read == 1 {
                    stats.clients.fetch_add(1, Ordering::Relaxed);
                }
                stats.rx_bytes.fetch_add(line.len(), Ordering::Relaxed);
                stats
                    .tx_serialized_bytes
                    .fetch_add(json_vecu8.len(), Ordering::Relaxed);
            }

            let partition_key = format!("{}", counter);

            let record = PutRecordsRequestEntry {
                data: json_vecu8.clone(),
                explicit_hash_key: None,
                partition_key: partition_key.clone(),
            };

            kinesis_wait
                .send(record)
                .unwrap_or_else(|e| error!("Wait#send error {:?}", e));
        }

        if self.lines_read != 0 {
            info!(
                "{:?} done. {} bytes, {} lines",
                self,
                convert(self.bytes_read as f64),
                self.lines_read
            );
        }
        Ok(())
    }

    fn parse_syslog_line<'a>(&self, line: &'a str) -> Result<Log<'a>, io::Error> {
        let res: IResult<&'a str, nom_syslog::Syslog3164Message<'a>> = parse_syslog(line);
        match res {
            IResult::Done(_, datum) => Ok(Log {
                app: String::new(),
                sender_ip: None,
                kv: None,
                message: Some(datum.msg),
                host: Some(datum.host),
                pri: Some(datum.pri),
                tag: datum.tag,
                ts: Some(datum.ts.to_utc().to_timespec().sec),
            }),
            IResult::Incomplete(a) => {
                error!("incomplete: {:?} on {}", a, line);
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "incomplete parse",
                ))
            }
            IResult::Error(e) => {
                error!("parse error: {:?} on {}", e, line);
                Err(io::Error::new(io::ErrorKind::UnexpectedEof, "bad data"))
            }
        }
    }
}

#[derive(PartialEq, Debug, Serialize)]
pub struct Log<'a> {
    pub app: String,
    pub sender_ip: Option<std::net::SocketAddr>,
    pub kv: Option<Vec<(&'a str, &'a str)>>,

    pub pri: Option<&'a str>,
    pub ts: Option<i64>,
    pub host: Option<&'a str>,
    pub tag: Option<(&'a str, Option<&'a str>)>,
    pub message: Option<&'a str>,
}
