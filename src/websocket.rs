use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tokio_util::sync::CancellationToken;

use crate::{
    TOTAL_DONATION_FILENAME, bid::write_bid, dollars::format_dollars, donation::DonationMessage,
    write_file::write_file,
};

pub async fn connect_socket(
    url: &str,
    cookie: &str,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    ),
    tokio_tungstenite::tungstenite::Error,
> {
    let mut request = url
        .into_client_request()
        .map_err(tokio_tungstenite::tungstenite::Error::from)?;

    request.headers_mut().insert(
        "Cookie",
        cookie.parse().map_err(|_| {
            tokio_tungstenite::tungstenite::Error::Http(
                tokio_tungstenite::tungstenite::http::Response::builder()
                    .status(400)
                    .body(Some("Invalid cookie".as_bytes().to_vec()))
                    .unwrap(),
            )
        })?,
    );

    request
        .headers_mut()
        .insert("Origin", "https://donate.cherry-rush.org".parse().unwrap());

    connect_async(request).await
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
                          Ok(d) => {
                            println!("Got new donation amount {}", d.amount);
                            let dollars = format_dollars(d.all_donors_event_total);
                            match write_file(&dollars, TOTAL_DONATION_FILENAME) {
                                Ok(_) => println!("Updated {TOTAL_DONATION_FILENAME} to {dollars}"),
                                Err(_) => println!("Failed to write to file!"),
                            };
                            for bid in d.bids.iter().filter(|b| b.istarget) {
                                if let Err(_) = write_bid(bid) {
                                    println!("Failed to write bid {} to file!", &bid.full_name);
                                }
                            }
                            println!("Finished writing bids to file!");
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
