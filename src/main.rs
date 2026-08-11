// use reqwest;
// use serde::Deserialize;

// #[derive(Deserialize)]
// struct Event {
//     r#type: String,
//     short: String,
//     donation_total: f64,
//     donation_count: u64,
//     donation_max: f64,
// }

// pub fn main() {
//     // totals
//     let url = "https://donate.cherry-rush.org/tracker/api/v2/events/2/?totals";
//     // incentives
//     // let url = "https://donate.cherry-rush.org/tracker/api/v2/bids/";
//     let result = reqwest::blocking::get(url)
//         .unwrap()
//         .json::<Event>()
//         .unwrap();

//     println!("{} {} {} {} {}", result.r#type, result.short, result.donation_total, result.donation_count, result.donation_max);
// }

use futures_util::StreamExt;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest, handshake::client::Request},
};

const SOCKETS: &[(&str, &str)] = &[
    (
        "donations",
        "wss://donate.cherry-rush.org/tracker/ws/donations/",
    ),
    (
        "processing",
        "wss://donate.cherry-rush.org/tracker/ws/processing/",
    ),
];

#[tokio::main]
async fn main() {
    let mut tasks = Vec::new();

    for &(name, url) in SOCKETS {
        tasks.push(tokio::spawn(listen(name, url)));
    }

    for task in tasks {
        if let Err(error) = task.await {
            eprintln!("Listener task failed: {error}");
        }
    }
}

async fn listen(name: &str, url: &str) {
    println!("[{name}] Connecting to {url}");

    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        "Cookie",
        "sessionid=pbe8cet6nopnhm0ffqfuvnyg6fsatl9p"
            .parse()
            .unwrap(),
    );

    // let request = Request::builder()
    //     .uri(url)
    //     .header("Cookie", "sessionid=pbe8cet6nopnhm0ffqfuvnyg6fsatl9p")
    //     .body(())
    //     .unwrap();

    let (ws_stream, response) = match connect_async(request).await {
        Ok(result) => result,
        Err(error) => {
            eprintln!("[{name}] Connection failed: {error}");
            return;
        }
    };

    println!("[{name}] Connected (HTTP status {})", response.status());

    let (_, mut read) = ws_stream.split();

    while let Some(result) = read.next().await {
        match result {
            Ok(Message::Text(text)) => {
                println!("\n[{name}] MESSAGE:");
                println!("{text}");
            }

            Ok(Message::Binary(data)) => {
                println!("\n[{name}] BINARY MESSAGE ({} bytes):", data.len());
                println!("{data:?}");
            }

            Ok(Message::Ping(data)) => {
                println!("\n[{name}] PING ({} bytes)", data.len());
            }

            Ok(Message::Pong(data)) => {
                println!("\n[{name}] PONG ({} bytes)", data.len());
            }

            Ok(Message::Close(frame)) => {
                println!("[{name}] Connection closed: {frame:?}");
                break;
            }

            Ok(_) => {}

            Err(error) => {
                eprintln!("[{name}] WebSocket error: {error}");
                break;
            }
        }
    }

    println!("[{name}] Listener stopped");
}
