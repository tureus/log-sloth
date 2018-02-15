extern crate env_logger;
#[macro_use]
extern crate log;
extern crate pretty_bytes;

use std::net::TcpStream;
use std::io::Write;
use std::thread::JoinHandle;

fn main() {
    env_logger::init();
    let args: Vec<String> = std::env::args().collect();

    let mut arg_iter = args.iter();
    let _ = arg_iter.next();

    let num_messages: usize = if let Some(n) = arg_iter.next() {
        n.parse().unwrap()
    } else {
        1000
    };

    let num_threads: usize = if let Some(n) = arg_iter.next() {
        n.parse().unwrap()
    } else {
        1
    };

    info!(
        "spawning stresser num_threads={} num_messages={}",
        num_threads, num_messages
    );
    let hs: Vec<JoinHandle<()>> = (0..num_threads)
        .map(|_| {
            std::thread::spawn(move || {
                stress(num_messages).unwrap();
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
}

fn stress(num_messages: usize) -> std::io::Result<()> {
    let mut stream =
        TcpStream::connect("127.0.0.1:1516").expect("Could not connect to the server!");

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
