Log Sloth
===

The Rust log server SDK. Processes RFC3164-flavored data and sends it out to AWS Kinesis.

Running the server:

    git clone https://github.com/tureus/log-sloth.git
    cargo run --bin log-sloth --release

Running the stress test:

    cargo run --bin stress --release

Performance
===

  cargo build --release
  RUST_LOG=info ./target/release/log-sloth server