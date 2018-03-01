//extern crate docopt;
//
//extern crate ctrlc;
//extern crate nom;
//extern crate nom_syslog;
//extern crate pretty_bytes;
//
//#[macro_use]
//extern crate lazy_static;
//
//#[macro_use]
//extern crate serde_derive;
//extern crate serde_json;
//
//extern crate tokio_core;
//extern crate tokio_io;
//extern crate futures;
//extern crate futures_cpupool;
//extern crate openssl_probe;
//extern crate rusoto_core;
//extern crate rusoto_kinesis;
//
//extern crate env_logger;
//#[macro_use]
//extern crate log;
//
//extern crate time;
//
//extern crate log_sloth;
//
//use std::{env, io};
//use std::net::{Shutdown, TcpStream};
//use std::io::BufReader;
//use std::sync::Arc;
//use std::sync::atomic::{Ordering};
//
//use docopt::Docopt;
//
//use nom_syslog::parse_syslog;
//use nom::IResult;
//
//use futures::{Stream,Future};
//use futures_cpupool::CpuPool;
//
//lazy_static! {
//    static ref CPU_POOL: CpuPool = {
//        CpuPool::new(4)
//    };
//}
//
//lazy_static! {
//    static ref STATS: Arc<Stats> = {
//        let args: Args = Docopt::new(USAGE)
//            .and_then(|d| d.deserialize())
//            .unwrap_or_else(|e| e.exit());
//        let (stats, stats_thread) = Stats::spawn_thread(args.flag_influxdb_url.clone(), args.flag_stats_interval);
//        stats
//    };
//}
//
//lazy_static! {
//    static ref KINESIS: DefaultKinesisClient = {
//        if env::var("USER").unwrap() == "root"
//        && (env::var("AWS_ACCESS_KEY_ID").is_err() || env::var("AWS_SECRET_ACCESS_KEY").is_err())
//        {
//            error!("if running as root, must set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY");
//            std::process::exit(1);
//        }
//
//        KinesisClient::simple(Region::UsWest2)
//    };
//}
//
//lazy_static! {
//    static ref STREAM_NAME: String = {
//        get_kinesis_stream_name(&KINESIS).expect("could not get stream name")
//    };
//}
//
//use rusoto_core::{Region};
//use rusoto_core::reactor::{CredentialsProvider, RequestDispatcher, DEFAULT_REACTOR};
//use rusoto_kinesis::{Kinesis, KinesisClient, ListStreamsInput, PutRecordsError, PutRecordsInput,
//                     PutRecordsOutput, PutRecordsRequestEntry};
//
//use log_sloth::stats::Stats;
//use log_sloth::fortigate_kv::extract_kv;
//use log_sloth::rename_thread;
//
//const USAGE: &str = "
//log-sloth.
//
//Usage:
//  log-sloth [--bind=ADDR] [--concurrency=N] [--disable-retry] [--enable-stats [--influxdb-url=URL] [--stats-interval=SEC]]
//  log-sloth (-h | --help)
//  log-sloth --version
//
//Options:
//  -h --help             Show this screen.
//  --version             Show version.
//  --bind=<ADDR>         Listen to ADDR:IP [default: 127.0.0.1:1516]
//  --concurrency=<kn>    Connections to Kinesis per client [default: 10]
//  --enable-stats        Send stats to InfluxDB backend
//  --influxdb-url=<url>  Target InfluxDB server [default: http://127.0.0.1:8086/write?db=telegraf]
//  --stats-interval=<s>  Stats interval in seconds [default: 15]
//  --disable-retry       Skip the feedback loop for retrying failed requests
//";
//
//#[derive(Debug, Deserialize)]
//struct Args {
//    pub flag_concurrency: usize,
//    pub flag_bind: String,
//    pub flag_influxdb_url: String,
//    pub flag_stats_interval: u64,
//    pub flag_enable_stats: bool,
//    pub flag_disable_retry: bool,
//}
//
//fn main() {
//    openssl_probe::init_ssl_cert_env_vars(); // required for musl static ssl deploy
//
//    let args: Args = Docopt::new(USAGE)
//        .and_then(|d| d.deserialize())
//        .unwrap_or_else(|e| e.exit());
//
//    env_logger::init();
//
//    SyslogServer::run(args);
//    loop {
//        std::thread::sleep(std::time::Duration::from_secs(60));
//    }
//}
//
//// I don't want to leak ARNs in to public code, so this little ditty pulls the name out of AWS
//fn get_kinesis_stream_name(thing: &DefaultKinesisClient) -> io::Result<String> {
//    let streams_response = thing.list_streams(&ListStreamsInput {
//        exclusive_start_stream_name: None,
//        limit: None,
//    });
//
//    let streams = match streams_response.sync() {
//        Ok(output) => output,
//        Err(e) => {
//            return Err(io::Error::new(
//                io::ErrorKind::InvalidData,
//                format!("could not list Kinesis streams: {:?}", e),
//            ));
//        }
//    };
//
//    if streams.stream_names.len() > 1 {
//        return Err(io::Error::new(
//            io::ErrorKind::InvalidData,
//            "I can only auto-discover one Kinesis stream",
//        ));
//    }
//
//    Ok(streams.stream_names[0].clone())
//}
//
//type DefaultKinesisClient = KinesisClient<CredentialsProvider, RequestDispatcher>;
//
//pub struct SyslogServer {}
//
//impl SyslogServer {
//    fn run(args: Args) {
//        info!("starting log sloth server: bind={} concurrency={} stream-name={}", args.flag_bind, args.flag_concurrency, &STREAM_NAME[..]);
//
//        let bind_addr = args.flag_bind.clone();
//
//        use futures::{Future, Sink, Stream};
//
//        DEFAULT_REACTOR.remote.spawn(move |handle| {
//            let addr = bind_addr.parse().expect(&format!("could not parse addr {}", bind_addr));
//            let listener = tokio_core::net::TcpListener::bind(&addr, handle).unwrap();
//
//            use futures::unsync::mpsc::{Sender, Receiver};
//            let (tx,rx): (Sender<Vec<PutRecordsRequestEntry>>, Receiver<Vec<PutRecordsRequestEntry>>) = futures::unsync::mpsc::channel(1000);
//            let rx_task = rx.map(|batch: Vec<PutRecordsRequestEntry>| {
//                info!("got thing={:?}", batch);
//                Ok::<_,()>(())
//            });
//
//            let server_task = listener.incoming().map_err(|e| {
//                error!("incoming failure: {:?}", e);
//            }).for_each(move |(tcp_stream,sockaddr)| {
//                STATS.clients.fetch_add(1, Ordering::Relaxed);
//
//                let lines = tokio_io::io::lines(BufReader::new(tcp_stream));
//                let processed_data = lines.map(|l| Ok(l)).filter_map(|res : Result<String,io::Error>| match res {
//                    Ok(string) => {
//                        STATS.rx_bytes.fetch_add(string.len(), Ordering::Relaxed);
//                        Some(string)
//                    },
//                    Err(e) => {
//                        error!("dropping line error: {:?}", e);
//                        None
//                    }
//                }).map_err(|e| {
//                    error!("lines error: {}", e);
//                }).chunks({
//                    500
//                }).and_then(|batch: Vec<String>| {
//                    info!("got a batch {:?}", batch.len());
//
//                    let t = CPU_POOL.spawn_fn( move||{
//                        let parsed: Vec<PutRecordsRequestEntry> = batch.into_iter().map(|str| {
//                            SyslogClient::parse_syslog_line(&str[..]).unwrap()
//                        }).enumerate().map(|(i,log)|{
//                            let mut json_vecu8 = serde_json::to_vec(&log).expect("could not serialize");
//                            json_vecu8.push('\n' as u8);
//                            STATS.tx_serialized_bytes.fetch_add(json_vecu8.len(), Ordering::Relaxed);
//                            PutRecordsRequestEntry {
//                                data: json_vecu8,
//                                explicit_hash_key: None,
//                                partition_key: format!("{}", i),
//                            }
//                        }).collect();
//                        Ok::<_,()>(parsed)
//                    });
//
//                    Ok(t)
//                }).map(|_| ());
//
//                let processed_data : futures::stream::Map<PutRecordsRequestEntry, fn(String) -> bool> = processed_data;
//                tx.send_all(processed_data).map(|_| ()).map_err(|_| ())
//            });
//
//            server_task
//        });
//    }
//}
//
//type RecordsChannel = futures::sync::mpsc::Sender<rusoto_kinesis::PutRecordsRequestEntry>;
//
//pub fn kinesis_tx(
//    client: DefaultKinesisClient,
//    stream_name: String,
//    chan_buf: usize,
//    inflight: usize,
//    disable_retry: bool,
//    stats: Option<Arc<Stats>>,
//) -> RecordsChannel {
//    debug!("CREATING kinesis channel");
//
//    use futures::sync::mpsc::channel;
//    use futures::{Future, Sink, Stream};
//
//    let (tx, rx) = channel(chan_buf);
//
//    let mut retry_tx = if disable_retry {
//        None
//    } else {
//        Some(tx.clone().wait())
//    };
//
//    std::thread::spawn(move || {
//        debug!("STARTING kinesis batch put thread");
//        rename_thread(&format!("k rx {}", time::now().tm_sec));
//        let puts = rx.chunks(500)
//            .map(|batch: Vec<PutRecordsRequestEntry>| {
//                let input = PutRecordsInput {
//                    records: batch,
//                    stream_name: stream_name.clone(),
//                };
//
//                if let Some(ref s) = stats {
//                    s.kinesis_inflight.fetch_add(1, Ordering::Relaxed);
//                }
//                client.put_records(&input).then(|put_res| {
//                    if let Some(ref s) = stats {
//                        s.kinesis_inflight.fetch_sub(1, Ordering::Relaxed);
//                    }
//                    match put_res {
//                        Ok(res) => {
//                            trace!("match put_res: it worked");
//                            Ok(Ok(res))
//                        }
//                        Err(err) => {
//                            trace!("match put_res: failed");
//                            Ok(Err((err, input)))
//                        }
//                    }
//                }
//                )
//            })
//            .buffer_unordered(inflight);
//
//        for put_res in puts.wait() {
//            match put_res {
//                Ok(Ok(put)) => {
//                    if let Some(failed) = put.failed_record_count {
//                        if failed > 0 {
//                            if let Some(ref s) = stats {
//                                s.kinesis_failures
//                                    .fetch_add(failed as usize, Ordering::Relaxed);
//                            }
//                            error!("{} record(s) failed to commit to kinesis. not sure what to do. dropping them. printed below:", failed);
//                            let put: PutRecordsOutput = put;
//                            for rec in put.records {
//                                if rec.error_code.is_some() {
//                                    error!("failed record: {:?}", rec);
//                                }
//                            }
//                        }
//                    }
//                }
//                Ok(Err((put_records_err, put_records_input))) => match put_records_err {
//                    PutRecordsError::HttpDispatch(dispatch_err) => {
//                        error!(
//                            "http dispatch error: {:?}. retrying records...",
//                            dispatch_err
//                        );
//                        for record in put_records_input.records {
//                            if let Some(ref mut retry_tx) = retry_tx {
//                                retry_tx
//                                    .send(record)
//                                    .unwrap_or_else(|r| error!("Wait#send error {:?}", r));
//                            }
//                        }
//                    }
//                    PutRecordsError::Unknown(raw_message) => {
//                        error!("unknown error: '{:?}', retrying records...", raw_message);
//                        for record in put_records_input.records {
//                            if let Some(ref mut retry_tx) = retry_tx {
//                                retry_tx
//                                    .send(record)
//                                    .unwrap_or_else(|r| error!("Wait#send error {:?}", r));
//                            }
//                        }
//                    }
//                    other => error!("unhandled kinesis error: {:?}", other),
//                },
//                other => error!("puts.wait() fallthrough: {:?}", other),
//            }
//        }
//        debug!("STOPPING kinesis batch put thread");
//    });
//
//    tx
//}
//
////#[derive(Clone)]
//pub struct SyslogClient {
//    stream: TcpStream,
//    #[allow(dead_code)]
//    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
//    lines_read: usize,
//    bytes_read: usize,
//}
//
//impl std::fmt::Debug for SyslogClient {
//    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
//        write!(f, "stream={:?}", self.stream.peer_addr().unwrap())
//    }
//}
//
//impl SyslogClient {
//    fn new(stream: TcpStream, shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
//        Self {
//            stream,
//            shutdown,
//            lines_read: 0,
//            bytes_read: 0,
//        }
//    }
//
//    #[allow(dead_code)]
//    fn shutdown(&self) -> io::Result<()> {
//        self.stream.shutdown(Shutdown::Both)
//    }
//
////    fn run(
////        &mut self,
////        kinesis_stream: RecordsChannel,
////        stats: Option<Arc<Stats>>,
////    ) -> Result<(), io::Error> {
////        let kinesis_stream = kinesis_stream;
////        let addr = self.stream.peer_addr()?;
////        let bufr = BufReader::with_capacity(4 * 1024, &self.stream);
////
////        use futures::Sink;
////        let mut kinesis_wait = kinesis_stream.wait();
////
////        for (counter, maybe_line) in bufr.lines().enumerate() {
////            let line: String = match maybe_line {
////                Ok(l) => l,
////                Err(e) => {
////                    error!("error while reading lines. breaking out. got {:?}", e);
////                    break;
////                }
////            };
////            self.bytes_read += line.len();
////            self.lines_read += 1;
////            let mut log = SyslogClient::parse_syslog_line(&line[..])?;
////            log.sender_ip = Some(addr);
////            log.kv = extract_kv(log.message.unwrap());
////            debug!("log: {:?}", log);
////
////            let mut json_vecu8 = serde_json::to_vec(&log)?;
////            json_vecu8.push('\n' as u8);
////
////            // Only count clients who send at least 1 message. This stops counting ELB health checks.
////            if let Some(ref stats) = stats {
////                if self.lines_read == 1 {
////                    stats.clients.fetch_add(1, Ordering::Relaxed);
////                }
////                stats.rx_bytes.fetch_add(line.len(), Ordering::Relaxed);
////                stats
////                    .tx_serialized_bytes
////                    .fetch_add(json_vecu8.len(), Ordering::Relaxed);
////            }
////
////            let partition_key = format!("{}", counter);
////
////            let record = PutRecordsRequestEntry {
////                data: json_vecu8.clone(),
////                explicit_hash_key: None,
////                partition_key: partition_key.clone(),
////            };
////
////            kinesis_wait
////                .send(record)
////                .unwrap_or_else(|e| error!("Wait#send error {:?}", e));
////        }
////
////        if self.lines_read != 0 {
////            info!(
////                "{:?} done. {} bytes, {} lines",
////                self,
////                convert(self.bytes_read as f64),
////                self.lines_read
////            );
////        }
////        Ok(())
////    }
//
//    fn parse_syslog_line(line: & str) -> Result<Log, io::Error> {
//        let res: IResult<&str, nom_syslog::Syslog3164Message> = parse_syslog(line);
//        match res {
//            IResult::Done(_, datum) => Ok(Log {
//                app: String::new(),
//                sender_ip: None,
//                kv: extract_kv(datum.msg),
//                message: Some(datum.msg.into()),
//                host: Some(datum.host.into()),
//                pri: Some(datum.pri.into()),
//                tag: None, // datum.tag.clone(),
//                ts: Some(datum.ts.to_utc().to_timespec().sec),
//            }),
//            IResult::Incomplete(a) => {
//                error!("incomplete: {:?} on {}", a, line);
//                Err(io::Error::new(
//                    io::ErrorKind::UnexpectedEof,
//                    "incomplete parse",
//                ))
//            }
//            IResult::Error(e) => {
//                error!("parse error: {:?} on {}", e, line);
//                Err(io::Error::new(io::ErrorKind::UnexpectedEof, "bad data"))
//            }
//        }
//    }
//}
//
//#[derive(PartialEq, Debug, Serialize)]
//pub struct Log {
//    pub app: String,
//    pub sender_ip: Option<std::net::SocketAddr>,
//    pub kv: Option<Vec<(String, String)>>,
//
//    pub pri: Option<String>,
//    pub ts: Option<i64>,
//    pub host: Option<String>,
//    pub tag: Option<(String, Option<String>)>,
//    pub message: Option<String>,
//}
