use std::sync::Arc;
use std::sync::Mutex;

use futures_util::{SinkExt, StreamExt};
use reqwest::header::HeaderValue;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

use crate::auth::get_request_with_auth;
use crate::donation::Donation;
use crate::donation::get_donation_from_message;
use crate::donation::write_donations;
use crate::{
    TOTAL_DONATION_FILENAME, bid::write_bid, dollars::format_dollars, donation::DonationMessage,
    write_file::write_file,
};

pub async fn connect_socket(
    url: &str,
    cookie: &HeaderValue,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    ),
    tokio_tungstenite::tungstenite::Error,
> {
    connect_async(get_request_with_auth(url, cookie)?).await
}

pub enum ConnectionResult {
    Shutdown,
    Disconnected(String),
}

pub async fn run_connection(
    name: &str,
    ws_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    shutdown: CancellationToken,
    donations: Arc<Mutex<Vec<Donation>>>,
    event_id: u64,
) -> ConnectionResult {
    let (mut write, mut read) = ws_stream.split();

    loop {
        tokio::select! {
            result = read.next() => {
                match result {
                    Some(Ok(Message::Text(text))) => {
                        println!("\n[{name}] MESSAGE:");
                        println!("{text}");
                        let donation = serde_json::from_str::<DonationMessage>(&text);
                        match donation {
                          Ok(d) => 'dono: {
                            if d.event != event_id {
                                break 'dono;
                            }
                            println!("Got new donation amount {}", d.amount);
                            let dollars = format_dollars(d.all_donors_event_total, false);
                            match write_file(&dollars, TOTAL_DONATION_FILENAME) {
                                Ok(_) => println!("Updated {TOTAL_DONATION_FILENAME} to {dollars}"),
                                Err(_) => println!("Failed to write to file!"),
                            };
                            for bid in &d.bids {
                                if let Err(_) = write_bid(bid) {
                                    println!("Failed to write bid {} to file!", &bid.full_name);
                                }
                            }
                            println!("Finished writing bids to file!");
                            let plain_donation = get_donation_from_message(d);
                            let mut mutable_donations = donations.lock().unwrap();
                            mutable_donations.insert(0, plain_donation);
                            if mutable_donations.len() > 20 {
                                mutable_donations.pop();
                            }
                            if let Ok(_) = write_donations(&mutable_donations) {
                                println!("Finished updating donation names/amounts!");
                            } else {
                                println!("Failed to update donation names/amounts!");
                            }
                          },
                          Err(_) => {
                            println!("Failed to parse donation message!");
                          }
                        }
                    }

                    Some(Ok(Message::Binary(data))) => {
                        println!(
                            "\n[{name}] BINARY MESSAGE ({} bytes):",
                            data.len()
                        );
                        println!("{data:?}");
                    }

                    Some(Ok(Message::Ping(data))) => {
                        println!(
                            "\n[{name}] PING ({} bytes)",
                            data.len()
                        );
                    }

                    Some(Ok(Message::Pong(data))) => {
                        println!(
                            "\n[{name}] PONG ({} bytes)",
                            data.len()
                        );
                    }

                    Some(Ok(Message::Close(frame))) => {
                        println!(
                            "[{name}] Server closed connection: {frame:?}"
                        );

                        return ConnectionResult::Disconnected(
                            "server sent Close".to_string()
                        );
                    }

                    Some(Ok(_)) => {}

                    Some(Err(error)) => {
                        return ConnectionResult::Disconnected(
                            error.to_string()
                        );
                    }

                    None => {
                        return ConnectionResult::Disconnected(
                            "WebSocket stream ended".to_string()
                        );
                    }
                }
            }

            _ = shutdown.cancelled() => {
                println!("[{name}] Closing WebSocket...");

                match write.close().await {
                    Ok(()) => {
                        println!(
                            "[{name}] WebSocket closed cleanly."
                        );
                    }

                    Err(error) => {
                        eprintln!(
                            "[{name}] Error closing WebSocket: {error}"
                        );
                    }
                }

                return ConnectionResult::Shutdown;
            }
        }
    }
}
