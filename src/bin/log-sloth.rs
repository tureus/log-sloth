extern crate docopt;

extern crate ctrlc;
extern crate nom;
extern crate nom_syslog;
extern crate pretty_bytes;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate tokio_core;
extern crate tokio_io;
extern crate futures;
extern crate futures_cpupool;
extern crate openssl_probe;
extern crate rusoto_core;
extern crate rusoto_kinesis;

extern crate env_logger;
#[macro_use]
extern crate log;

extern crate time;

extern crate log_sloth;

use std::{env, io};
use std::io::BufReader;
use std::sync::Arc;
use std::sync::atomic::{Ordering};

use docopt::Docopt;

use nom_syslog::parse_syslog;
use nom::IResult;

use futures::{Stream,Future};
use futures_cpupool::{ CpuPool, Builder };

use rusoto_core::{Region};
use rusoto_core::reactor::{CredentialsProvider, RequestDispatcher, DEFAULT_REACTOR};
use rusoto_kinesis::{Kinesis, KinesisClient, ListStreamsInput, PutRecordsError, PutRecordsInput,
                     PutRecordsOutput, PutRecordsRequestEntry};

use log_sloth::stats::Stats;
use log_sloth::fortigate_kv::extract_kv;

type DefaultKinesisClient = KinesisClient<CredentialsProvider, RequestDispatcher>;

lazy_static! {
    static ref CPU_POOL: CpuPool = {
        Builder::new()
            .name_prefix("sloth-cpu")
            .pool_size(4)
            .create()
    };

    static ref STATS: Arc<Stats> = {
        let args: Args = Docopt::new(USAGE)
            .and_then(|d| d.deserialize())
            .unwrap_or_else(|e| e.exit());
        let (stats, _) = Stats::spawn_thread(args.flag_influxdb_url.clone(), args.flag_stats_interval);
        stats
    };

    static ref KINESIS: DefaultKinesisClient = {
        if env::var("USER").unwrap() == "root"
        && (env::var("AWS_ACCESS_KEY_ID").is_err() || env::var("AWS_SECRET_ACCESS_KEY").is_err())
        {
            error!("if running as root, must set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY");
            std::process::exit(1);
        }

        KinesisClient::simple(Region::UsWest2)
    };

    static ref STREAM_NAME: String = {
        get_kinesis_stream_name(&KINESIS).expect("could not get stream name")
    };
}

const USAGE: &str = "
log-sloth.

Usage:
  log-sloth [--bind=ADDR] [--concurrency=N] [--disable-retry] [--enable-stats [--influxdb-url=URL] [--stats-interval=SEC]]
  log-sloth (-h | --help)
  log-sloth --version

Options:
  -h --help             Show this screen.
  --version             Show version.
  --bind=<ADDR>         Listen to ADDR:IP [default: 127.0.0.1:1516]
  --concurrency=<kn>    Connections to Kinesis per client [default: 10]
  --influxdb-url=<url>  Target InfluxDB server [default: http://127.0.0.1:8086/write?db=telegraf]
  --stats-interval=<s>  Stats interval in seconds [default: 15]
";

const RECS_LOAD_FACTOR : usize = 10; // number of messages per record
const RECS_PER_REQ : usize = 500; // API limit on records per request

#[derive(Debug, Deserialize)]
struct Args {
    pub flag_bind: String,
    pub flag_concurrency: usize,
    pub flag_influxdb_url: String,
    pub flag_stats_interval: u64,
}

fn main() {
    openssl_probe::init_ssl_cert_env_vars(); // required for musl static ssl deploy

    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    env_logger::init();

    SyslogServer::run(args);

    log_sloth::rename_thread("main");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
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

pub struct SyslogServer {}

impl SyslogServer {
    fn run(args: Args) {
        log_sloth::stats::Stats::spawn_loop(args.flag_influxdb_url.clone(), args.flag_stats_interval);
        info!("starting log sloth server: bind={} concurrency={} stream-name={}", args.flag_bind, args.flag_concurrency, &STREAM_NAME[..]);

        let bind_addr = args.flag_bind.clone();

        DEFAULT_REACTOR.remote.spawn(move |handle| {
            let addr = bind_addr.parse().expect(&format!("could not parse addr {}", bind_addr));
            let listener = tokio_core::net::TcpListener::bind(&addr, handle).unwrap();

            listener
                .incoming()
                // .from_err::<Box<std::error::Error + Send + Sync + 'static>>()
                .map(|(tcp_stream, _)| {
                    STATS.clients.fetch_add(1, Ordering::Relaxed);
                    tokio_io::io::lines(BufReader::new(tcp_stream))
                        .inspect(|l| {
                            STATS.rx_bytes.fetch_add(l.len(), Ordering::Relaxed);
                        })
                        // .from_err::<Box<std::error::Error + Send + Sync + 'static>>()
                        .chunks(RECS_LOAD_FACTOR * RECS_PER_REQ)
                })
                .flatten()
                .and_then(|batch| CPU_POOL.spawn_fn(|| Ok(entries(batch))))
                .map(|records| {
                    STATS.kinesis_inflight.fetch_add(1, Ordering::Relaxed);
                    Ok(KINESIS.put_records(&PutRecordsInput {
                            records: records,
                            stream_name: STREAM_NAME.clone(),
                    }).then(inspect_kinesis_response))
                })
                .buffer_unordered(args.flag_concurrency)
                .for_each(|_| Ok(()) )
                .map_err(|e| error!("listener error: {:?}", e))
        });
    }
}

fn entries(batch: Vec<String>) -> Vec<PutRecordsRequestEntry> {
    // TODO: serialize in chunks to as writer handle of the buf
    let intermediate_batch : Vec<Vec<u8>> = batch
        .into_iter()
        .filter_map(|message| match parse_syslog_line(&message[..]) {
            Ok(log) => Some(log),
            Err(e) => {
                error!("could not parse message `{}` because {:?}", message, e);
                None
            }
        })
        .map(|log| {
            let mut data = serde_json::to_vec(&log).expect("could not serialize");
            data.push(b'\n');
            data
        }).collect();

    intermediate_batch.chunks(RECS_LOAD_FACTOR)
        .enumerate()
        .map(|(i, data)| {
            let total_bytes = data.iter().map(|d| d.len()).sum();

            let mut buf : Vec<u8> = vec![0; total_bytes];
            let mut start = 0;
            for d in data {
                let sub_buf = &mut buf[start .. start+d.len()];
                assert_eq!(sub_buf.len(), d.len());
                sub_buf.copy_from_slice(&d[..]);
                start += d.len();
            };

            STATS.tx_serialized_bytes.fetch_add(buf.len(), Ordering::Relaxed);
            PutRecordsRequestEntry {
                data: buf,
                explicit_hash_key: None,
                partition_key: i.to_string(),
            }
        })
        .collect()
}

fn inspect_kinesis_response(response: Result<PutRecordsOutput,PutRecordsError>) -> Result<(),()> {
    STATS.kinesis_inflight.fetch_sub(1, Ordering::Relaxed);
    match response {
        Ok(put) => {
            if let Some(failed) = put.failed_record_count {
                STATS.kinesis_failures.fetch_add(failed as usize, Ordering::Relaxed);

                for rec in put.records {
                    if rec.error_code.is_some() {
                        error!("failed record: {:?}", rec);
                    }
                }

            }
        },
        Err(put_records_err) => {
            // TODO: gotta STATS count all 500 lost records if the request is rejected
            match put_records_err {
                PutRecordsError::HttpDispatch(dispatch_err) => {
                    error!("http dispatch error: {:?}", dispatch_err);
                },
                PutRecordsError::Unknown(raw_message) => {
                    error!("unknown error: '{:?}'", raw_message);
                }
                other => error!("unhandled kinesis error: {:?}", other),
            }
        }
    };
    Ok(())
}

fn parse_syslog_line(line: & str) -> Result<Log, io::Error> {
    let res: IResult<&str, nom_syslog::Syslog3164Message> = parse_syslog(line);
    match res {
        IResult::Done(_, datum) => Ok(Log {
            app: String::new(),
            sender_ip: None,
            kv: extract_kv(datum.msg),
            message: Some(datum.msg.into()),
            host: Some(datum.host.into()),
            pri: Some(datum.pri.into()),
            tag: None, // datum.tag.clone(),
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

#[derive(PartialEq, Debug, Serialize)]
pub struct Log {
    pub app: String,
    pub sender_ip: Option<std::net::SocketAddr>,
    pub kv: Option<Vec<(String, String)>>,

    pub pri: Option<String>,
    pub ts: Option<i64>,
    pub host: Option<String>,
    pub tag: Option<(String, Option<String>)>,
    pub message: Option<String>,
}
