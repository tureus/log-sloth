extern crate env_logger;
#[macro_use]
extern crate log;
extern crate pretty_bytes;

extern crate docopt;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use std::net::TcpStream;
use std::io::Write;
use std::thread::JoinHandle;

use docopt::Docopt;

const USAGE: &'static str = "
stress.

Usage:
  stress [--num-messages=<nm>] [--target=<target>]
  stress (-h | --help)
  stress --version

Options:
  -h --help     Show this screen.
  --version     Show version.
  --num-messages=<nm>  number of syslog lines to send [default: 10]
  --num-threads=<nt>   number of threads to spawn for sending messages [default: 1]
  --target=<t>         hostname:port to resolve for sending messages [default: 127.0.0.1:1516]
";

#[derive(Debug, Deserialize)]
struct Args {
    pub flag_target: String,
    pub flag_num_messages: usize,
    pub flag_num_threads: usize,
}

fn main() {
    env_logger::init();
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    info!(
        "spawning stresser target={} num_threads={} num_messages={}",
        args.flag_target, args.flag_num_threads, args.flag_num_messages
    );
    let hs: Vec<JoinHandle<()>> = (0..args.flag_num_threads)
        .map(|_| {
            let target = args.flag_target.clone();
            let num_messages = args.flag_num_messages;
            std::thread::spawn(move || {
                stress(target, num_messages).unwrap();
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
}

fn stress(target: String, num_messages: usize) -> std::io::Result<()> {
    let mut stream = TcpStream::connect(target).expect("Could not connect to the server!");

    let buf = r##"<14>Dec 13 17:45:02 SANTA-CLAUS-W764.blerg.com nxWinEvt: {"EventTime":"2017-12-19 17:45:02","Hostname":"fake-hostname","Keywords":-9214364837600034816,"EventType":"AUDIT_SUCCESS","SeverityValue":2,"Severity":"INFO","EventID":4656,"SourceName":"Microsoft-Windows-Security-Auditing","ProviderGuid":"{54849625-5478-4994-A5BA-3E3B0328C30D}","Version":1,"Task":12804,"OpcodeValue":0,"RecordNumber":7613465324,"ProcessID":892,"ThreadID":908,"Channel":"Security","AccessReason":"-","AccessMask":"0x2","PrivilegeList":"-","RestrictedSidCount":"0","ProcessName":"C:\\Windows\\System32\\svchost.exe","EventReceivedTime":"2017-12-19 17:52:27","SourceModuleName":"eventlog","SourceModuleType":"im_msvistalog"}
"##;

    let mut bytes_written = 0;

    let buf_bytes = buf.as_bytes();
    for _ in 0..num_messages {
        bytes_written += stream.write(buf_bytes)?;
    }
    stream.flush()?;

    println!(
        "done! wrote {}",
        pretty_bytes::converter::convert(bytes_written as f64)
    );

    Ok(())
}
