use std::sync::atomic::{AtomicUsize, Ordering};
use hostname;
use std::thread;
use std::sync::Arc;

use futures::{Future, Stream};
use hyper::{Client, Method, Request, Uri};
use tokio_core::reactor::Core;

pub struct Stats {
    pub rx_bytes: AtomicUsize,
    pub clients: AtomicUsize,
    pub tx_serialized_bytes: AtomicUsize,
    pub kinesis_failures: AtomicUsize,
}

impl Stats {
    pub fn spawn_thread(influxdb_url: String) -> (Arc<Self>, thread::JoinHandle<()>) {
        info!("spawning stats thread");
        let stats = Arc::new(Self::new());
        let stats_2 = stats.clone();

        let handle = thread::spawn(move || {
            let stats = stats_2;
            let mut core = Core::new().unwrap();
            let handle = core.handle();
            let influxdb_uri: Uri = influxdb_url.parse().unwrap();

            let hostname = hostname::get_hostname().unwrap();
            let client = Client::new(&handle);

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
                thread::sleep_ms(15*1000);
            }
        });

        (stats, handle)
    }

    fn new() -> Self {
        Self {
            rx_bytes: AtomicUsize::new(0),
            tx_serialized_bytes: AtomicUsize::new(0),
            clients: AtomicUsize::new(0),
            kinesis_failures: AtomicUsize::new(0)
        }
    }

    pub fn series(&self, hostname: &str) -> String {
        format!(
            r#"log_sloth_stats,host={} rx_byte={}.0
log_sloth_stats,host={} clients={}.0
log_sloth_stats,host={} kinesis_put_failure={}.0
log_sloth_stats,host={} tx_serialized_bytes={}.0"#,
            hostname,
            self.rx_bytes.load(Ordering::Relaxed),
            hostname,
            self.clients.load(Ordering::Relaxed),
            hostname,
            self.kinesis_failures.load(Ordering::Relaxed),
            hostname,
            self.tx_serialized_bytes.load(Ordering::Relaxed),
        )
    }
}
