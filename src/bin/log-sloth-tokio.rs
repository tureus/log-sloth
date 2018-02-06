extern crate ctrlc;

extern crate env_logger;
#[macro_use]
extern crate log;

extern crate bytes;
extern crate time;

extern crate nom;
extern crate nom_syslog;

extern crate rusoto_core;
extern crate rusoto_kinesis;

extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_proto;
extern crate tokio_service;

use nom_syslog::parse_syslog;
use nom::IResult;

use bytes::{BufMut, BytesMut};

use tokio_core::net::TcpListener;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::codec::{Decoder, Encoder, Framed};
use tokio_proto::TcpServer;
use tokio_proto::pipeline::ServerProto;
use tokio_service::{NewService, Service};

use futures::future;
use futures::future::{Executor, Future};
use futures::stream::Stream;

use rusoto_core::Region;
use rusoto_core::reactor::{CredentialsProvider, RequestDispatcher};
use rusoto_kinesis::{Kinesis, KinesisClient, ListStreamsInput, PutRecordsError, PutRecordsInput,
                     PutRecordsRequestEntry};

use std::str;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use std::io;

fn main() {
    env_logger::init().unwrap();

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        info!("shutting down syslog server");
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");

    std::thread::spawn(do_server);

    info!("Waiting for Ctrl-C...");
    while running.load(Ordering::SeqCst) {}
    debug!("Got it! Exiting...");
}

type DefaultKinesisClient = Arc<KinesisClient<CredentialsProvider, RequestDispatcher>>;
fn do_server() -> () {
    debug!("creating kinesis client");
    let kinesis_client = Arc::new(KinesisClient::simple(Region::UsWest2));

    info!("fetching kinesis stream name");
    let stream_name =
        get_kinesis_stream_name(&kinesis_client).expect("could not get kinesis stream name");

    let addr: std::net::SocketAddr = "0.0.0.0:1516".parse().unwrap();
    let server = TcpServer::new(LineProto, addr);

    let syslog_app = SyslogServer {};
    server.serve(syslog_app);
}

pub struct OwnedSyslog3164Message {
    pub pri: String,
    pub ts: time::Tm,
    pub host: String,
    pub tag: Option<(String, Option<String>)>,
    pub msg: String,
}

struct SyslogServer;
impl<'a> Service for SyslogServer {
    type Request = String;
    type Response = String;
    type Error = io::Error;
    // For simplicity, box the future.
    type Future = Box<Future<Item = Self::Response, Error = io::Error>>;

    fn call(&self, req: String) -> Self::Future {
        let f = future::done(Ok(req)).and_then(|x| {
            info!("got {:?}", x);
            Ok(x)
        });
        Box::new(f)
    }
}

impl NewService for SyslogServer {
    type Request = String;
    type Response = String;
    type Error = io::Error;
    type Instance = SyslogServer;

    fn new_service(&self) -> io::Result<Self::Instance> {
        Ok(SyslogServer {})
    }
}

/// Our line-based codec
pub struct LineCodec;
/// Protocol definition
struct LineProto;

impl<T: AsyncRead + AsyncWrite + 'static> ServerProto<T> for LineProto {
    type Request = String;
    type Response = String;

    /// `Framed<T, LineCodec>` is the return value of `io.framed(LineCodec)`
    type Transport = Framed<T, LineCodec>;
    type BindTransport = Result<Self::Transport, io::Error>;

    fn bind_transport(&self, io: T) -> Self::BindTransport {
        Ok(io.framed(LineCodec))
    }
}

/// Implementation of the simple line-based protocol.
///
/// Frames consist of a UTF-8 encoded string, terminated by a '\n' character.
impl Decoder for LineCodec {
    type Item = String;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<String>, io::Error> {
        // Check to see if the frame contains a new line
        if let Some(n) = buf.as_ref().iter().position(|b| *b == b'\n') {
            // remove the serialized frame from the buffer.
            let line = buf.split_to(n);

            // Also remove the '\n'
            buf.split_to(1);

            // Turn this data into a UTF string and return it in a Frame.
            return match str::from_utf8(&line.as_ref()) {
                Ok(s) => Ok(Some(s.to_string())),
                Err(_) => Err(io::Error::new(io::ErrorKind::Other, "invalid string")),
            };
        }

        Ok(None)
    }
}

impl Encoder for LineCodec {
    type Item = String;
    type Error = io::Error;

    fn encode(&mut self, msg: String, buf: &mut BytesMut) -> io::Result<()> {
        // Reserve enough space for the line
        buf.reserve(msg.len() + 1);

        buf.extend(msg.as_bytes());
        buf.put_u8(b'\n');

        Ok(())
    }
}

fn get_kinesis_stream_name(thing: &DefaultKinesisClient) -> io::Result<String> {
    let streams_response = thing.list_streams(&ListStreamsInput {
        exclusive_start_stream_name: None,
        limit: None,
    });

    let streams = match streams_response.sync() {
        Ok(output) => output,
        Err(_) => {
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
            format!(
                "I can only auto-discover one Kinesis stream, you have {}",
                streams.stream_names.len()
            ),
        ));
    }

    Ok(streams.stream_names[0].clone())
}
