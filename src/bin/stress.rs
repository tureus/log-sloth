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
  stress [--num-messages=<nm>] [--num-threads=<nt>] [--target=<target>]
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

    let buf = r##"<14>Dec 13 17:45:02 SANTA-CLAUS-W764.blerg.com date=2018-02-21 time=20:37:11 logver=54 devname=\"VCA-CORP-INET-900D\" devid=\"FG900D3916800019\" vd=\"edge\" date=2018-02-21 time=20:37:11 logid=\"0000000013\" type=\"traffic\" subtype=\"forward\" level=\"notice\" srcip=172.26.32.44 srcport=53075 srcintf=\"portA\" dstip=172.217.11.163 dstport=443 dstintf=\"portB\" poluuid=\"b018d2b0-4491-51e6-ff20-8b8d11964286\" sessionid=3075354808 proto=17 action=\"accept\" policyid=2 policytype=\"policy\" dstcountry=\"United States\" srccountry=\"Reserved\" trandisp=\"snat\" transip=8.37.96.36 transport=53075 service=\"udp/443\" appid=40169 app=\"QUIC\" appcat=\"Network.Service\" apprisk=\"low\" applist=\"app_ctrl\" appact=\"detected\" duration=181 sentbyte=3900 rcvdbyte=4753 sentpkt=8 rcvdpkt=7 utmaction=\"allow\" countapp=1
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
