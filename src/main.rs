use std::env;
use std::net::{TcpListener, TcpStream};
use std::io;
use std::io::{ BufReader, BufRead, Error, ErrorKind };
use std::process::exit;

fn main() {
  let args : Vec<String> = env::args().collect();

  if args.len() < 2 {
    println!("must set action to 'server' or 'client'");
    exit(1)
  }

  match &args[1][..] {
    "server" => server(),
    "client" => unimplemented!(),
    other => {
      println!("we don't handle {:?}, use 'server' or 'client'", other);
      Ok(())
    }
  }.unwrap();

  ()
}

fn server() -> Result<(),io::Error> {
  let listener = TcpListener::bind("0.0.0.0:1516")?;

  for stream in listener.incoming() {
    handle_client(&mut stream?);
  };

  Ok(())
}

fn handle_client(stream: &mut TcpStream) -> Result<(),io::Error> {
  let bufr = BufReader::with_capacity(4*1024, stream);
  for line in bufr.lines() {
    handle_line(line?)?
  }
  Ok(())
}

fn handle_line(line: String) -> Result<(),io::Error> {
  let f = Fortigate{};
  f.process(&line[..]);

  Ok(())
}

pub trait LogProcessor {
  fn process(&self, string: &str) -> Result<String,io::Error>;
}

struct Fortigate {}

impl LogProcessor for Fortigate {
  fn process(&self, string: &str) -> Result<String,io::Error> {
    // println!("sup {}", string);
    let _ : Vec<Vec<String>> = string.split_whitespace().map(|x| x.split(' ').map(|x| x.to_owned()).collect()).collect();
    // Err(Error::new(ErrorKind::InvalidData, "bad line".to_string()))
    Ok("yes".to_owned())
  }
}

pub struct NLog {
  pub application: String
}

#[test]
fn fortigate_parses(){
  let f = Fortigate{};
  let res = f.process("a=b c=d e=f g=h");
  assert_eq!(res.unwrap(),"123")
}
