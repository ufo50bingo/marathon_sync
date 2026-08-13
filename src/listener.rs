use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::{
    donation::Donation,
    websocket::{ConnectionResult, connect_socket, run_connection},
};

const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

pub async fn listen(
    name: &str,
    url: &str,
    cookie: &str,
    shutdown: CancellationToken,
    donations: &Arc<Mutex<Vec<Donation>>>,
) {
    let mut reconnect_delay = Duration::from_secs(1);

    loop {
        if shutdown.is_cancelled() {
            println!("[{name}] Shutdown requested.");
            break;
        }

        println!("[{name}] Connecting to {url}");

        match connect_socket(url, cookie).await {
            Ok((ws_stream, response)) => {
                println!("[{name}] Connected (HTTP status {})", response.status());

                reconnect_delay = Duration::from_secs(1);

                let connection_result =
                    run_connection(name, ws_stream, shutdown.clone(), donations).await;

                match connection_result {
                    ConnectionResult::Shutdown => {
                        println!("[{name}] Shutting down.");
                        break;
                    }

                    ConnectionResult::Disconnected(reason) => {
                        eprintln!("[{name}] Connection lost: {reason}");
                    }
                }
            }

            Err(error) => {
                eprintln!("[{name}] Connection failed: {error}");
            }
        }

        println!(
            "[{name}] Reconnecting in {} second(s)...",
            reconnect_delay.as_secs()
        );

        tokio::select! {
            _ = sleep(reconnect_delay) => {}

            _ = shutdown.cancelled() => {
                println!("[{name}] Shutdown requested.");
                break;
            }
        }

        reconnect_delay = std::cmp::min(reconnect_delay * 2, MAX_RECONNECT_DELAY);
    }

    println!("[{name}] Listener stopped.");
}
