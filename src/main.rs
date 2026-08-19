mod auth;
mod bid;
mod config;
mod dollars;
mod donation;
mod event;
mod listener;
mod websocket;
mod write_file;

use std::env;
use std::sync::Arc;
use std::sync::Mutex;

use tokio_util::sync::CancellationToken;

use crate::{
    bid::{fetch_bids, write_bid},
    config::TOTAL_DONATION_FILENAME,
    dollars::format_dollars,
    donation::{fetch_donations, write_donations},
    write_file::write_file,
};
use auth::{load_cookie, login, validate_cookie};
use config::SOCKETS;
use listener::listen;

use crate::event::fetch_event;

#[tokio::main]
async fn main() {
    let event_id_input = env::args().nth(1);
    let event_id = match event_id_input {
        Some(event_str) => match event_str.parse::<u64>() {
            Ok(val) => val,
            Err(_) => panic!(
                "Failed to parse event ID \"{event_str}\"! An integer is expected. To find your event ID, go to https://donate.cherry-rush.org/admin/tracker/event/, then click on your event and look for a number in the URL"
            ),
        },
        None => 1,
    };

    let session_cookie = match load_cookie() {
        Some(cookie) => {
            println!("Found saved session cookie.");

            if validate_cookie(&cookie).await {
                println!("Saved session is still valid.");
                cookie
            } else {
                println!("Saved session is no longer valid.");

                match login().await {
                    Ok(cookie) => cookie,
                    Err(error) => {
                        eprintln!("Login failed: {error}");
                        return;
                    }
                }
            }
        }

        None => {
            println!("No saved session found.");

            match login().await {
                Ok(cookie) => cookie,
                Err(error) => {
                    eprintln!("Login failed: {error}");
                    return;
                }
            }
        }
    };

    println!("Initializing donation total");
    let event = fetch_event(event_id).await;
    match event {
        Ok(e) => {
            let dollars = format_dollars(e.donation_total, false);
            match write_file(&dollars, TOTAL_DONATION_FILENAME) {
                Ok(_) => println!("Initialized {TOTAL_DONATION_FILENAME} to {dollars}"),
                Err(_) => println!("Failed to write to file!"),
            }
        }
        Err(_) => {
            println!("Failed to fetch initial donation total!");
        }
    }

    println!("Initializing bids");
    let bids = fetch_bids(event_id, &session_cookie).await;
    match bids {
        Ok(bids) => {
            for bid in &bids {
                if let Err(_) = write_bid(bid) {
                    println!("Failed to write bid {} to file!", &bid.full_name);
                }
            }
            println!("Finished writing bids to file!");
        }
        Err(err) => {
            eprintln!("Failed to fetch initial bids! {err}");
        }
    }

    println!("Initializing donations");
    let donations = fetch_donations(event_id).await;
    match &donations {
        Ok(proper_donations) => {
            let _ = write_donations(proper_donations);
            println!("Finished writing donations to file!");
        }
        Err(_) => {
            println!("Failed to fetch initial donations!");
        }
    }
    let wrapped_donatons = match donations {
        Ok(donations) => Arc::new(Mutex::new(donations)),
        Err(_) => Arc::new(Mutex::new(vec![])),
    };

    let shutdown = CancellationToken::new();
    let mut tasks = Vec::new();

    for &(name, url) in SOCKETS {
        let shutdown = shutdown.clone();
        let cookie = session_cookie.clone();
        let donos = Arc::clone(&wrapped_donatons);

        tasks.push(tokio::spawn(async move {
            listen(name, url, &cookie, shutdown, donos, event_id).await;
        }));
    }

    println!();
    println!("Listeners started.");
    println!("Press Ctrl+C to exit.");

    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("Failed to listen for Ctrl+C: {error}");
    }

    println!();
    println!("Ctrl+C received.");
    println!("Shutting down listeners...");

    shutdown.cancel();

    for task in tasks {
        if let Err(error) = task.await {
            eprintln!("Listener task failed: {error}");
        }
    }

    println!("Shutdown complete.");
}
