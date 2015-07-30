extern crate docopt;
extern crate rustc_serialize;
extern crate clock_ticks;

use std::thread;
use std::sync::mpsc::channel;
use std::str;
use std::str::FromStr;
use backend::backend::Backend;


// Local module imports.
mod metric;
mod cli;
mod server;
mod buckets;
mod backend {
    pub mod backend;
    pub mod console;
}


// Max Packet Size is 256.
// static FLUSH_INTERVAL_MS: u64 = 10000;


fn main() {
    let args = cli::parse_args();

    let backends = backend::backend::factory(
        &args.flag_console,
        &args.flag_graphite);

    let (event_send, event_recv) = channel();
    let flush_send = event_send.clone();
    let udp_send = event_send.clone();

    let mut buckets = buckets::Buckets::new();

    // Setup the UDP server which publishes events to the event channel
    let port = args.flag_port;
    thread::spawn(move || {
        server::udp_server(udp_send, port);
    });

    // Run the timer that flushes metrics to the backends.
    let flush_interval = args.flag_flush_interval;
    thread::spawn(move || {
        server::flush_timer_loop(flush_send, flush_interval);
    });

    // Main event loop.
    loop {
        let result = match event_recv.recv() {
            Ok(res) => res,
            Err(e) => panic!(format!("Event channel has hung up: {:?}", e)),
        };

        match result {
            server::Event::TimerFlush => {
                for backend in backends.iter() {
                    backend.flush_buckets(&buckets);
                }
            },

            server::Event::UdpMessage(buf) => {
                // Create the metric and push it into the buckets.
                str::from_utf8(&buf).map(|val| {
                    metric::Metric::from_str(&val)
                    .and_then(|metric| {
                        buckets.add(&metric);
                        Ok(metric)
                    })
                    .or_else(|err| {
                        buckets.add_bad_message();
                        Err(err)
                    }).ok();
                }).ok();
            },
        }
    }
}
