use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use hostname;
use std::thread;
use std::time::Duration;
use time;
use std::sync::Arc;
use std::default::Default;

use futures::{Future, Stream};
use hyper::{Client, Method, Request, Uri};
use tokio_core::reactor::Core;

#[derive(Default)]
pub struct Stats {
    pub rx_bytes: AtomicUsize,
    pub clients: AtomicUsize,
    pub tx_serialized_bytes: AtomicUsize,
    pub kinesis_failures: AtomicUsize,
    pub kinesis_inflight: AtomicIsize,
}

impl Stats {
    pub fn spawn_thread(influxdb_url: String, interval_sec: u64) -> (Arc<Self>, thread::JoinHandle<()>) {
        info!("spawning stats thread influxdb_url={} interval_sec={}", influxdb_url, interval_sec);
        let stats = Arc::new(Self::new());
        let stats_2 = stats.clone();

        let interval = Duration::from_secs(interval_sec);

        let handle = thread::spawn(move || {
            let stats = stats_2;
            let mut core = Core::new().unwrap();
            let handle = core.handle();
            let influxdb_uri: Uri = influxdb_url.parse().unwrap();

            let hostname = hostname::get_hostname().unwrap();
            let client = Client::new(&handle);

            time_align(interval);

            loop {
                let series = stats.series(&hostname);
                let mut request = Request::new(Method::Post, influxdb_uri.clone());
                request.set_body(series);
                debug!("influxdb request={:?}", request);

                let future = client.request(request).and_then(|res| {
                    debug!("influxdb response status={}", res.status());
                    res.body().concat2()
                });

                let res = core.run(future).unwrap_or_default();

                use std::str::from_utf8;
                debug!(
                    "influxdb response body={}",
                    from_utf8(&res.to_owned()).unwrap()
                );
                time_align(interval);
            }
        });

        (stats, handle)
    }

    fn new() -> Self {
        Default::default()
    }

    pub fn series(&self, hostname: &str) -> String {
        format!(
            r#"log_sloth_stats,host={} rx_byte={}.0
log_sloth_stats,host={} clients={}.0
log_sloth_stats,host={} kinesis_put_failure={}.0
log_sloth_stats,host={} tx_serialized_bytes={}.0
log_sloth_stats,host={} kinesis_inflight={}.0"#,
            hostname,
            self.rx_bytes.load(Ordering::Relaxed),
            hostname,
            self.clients.load(Ordering::Relaxed),
            hostname,
            self.kinesis_failures.load(Ordering::Relaxed),
            hostname,
            self.tx_serialized_bytes.load(Ordering::Relaxed),
            hostname,
            self.kinesis_inflight.load(Ordering::Relaxed),
        )
    }
}

fn time_align(interval: Duration) {
    let now = time::now();
    let til_boundary = (now.tm_sec as u64) % interval.as_secs();
    trace!("sleeping {} seconds to get to the {} boundary", til_boundary, interval.as_secs());
    thread::sleep(Duration::from_secs(til_boundary-4)); // back off a little, buckets are getting messed up
}