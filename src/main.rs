use std::env;
use std::net::{TcpListener, TcpStream};
use std::io;
use std::io::{ BufReader, BufRead };

fn main() {
  let args : Vec<String> = env::args().collect();

  match &args[1][..] {
    "server" => server(),
    "client" => unimplemented!(),
    other => unimplemented!()
  };

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
  let mut bufr = BufReader::with_capacity(4*1024, stream);
  for line in bufr.lines() {
    handle_line(line?)?
  }
  Ok(())
}

pub trait LogProcessor {
  fn process(string: String) -> String
}

struct FortigateProcessor {}

impl LogProcessor for FortigateProcessor {
  fn process(string: String) -> Result<String,()> {
    println!("sup {}", string);
    Err(())
  }
}

fn handle_line(line: String) -> Result<(),io::Error> {
  let fp : FortigateProcessor = FortigateProcessor{};

  fp.process(line);
  Ok(())
}