mod auth;
mod bid;
mod config;
mod dollars;
mod donation;
mod event;
mod listener;
mod websocket;
mod write_file;

use std::sync::Arc;
use std::sync::Mutex;

use tokio_util::sync::CancellationToken;

use crate::{
    bid::{fetch_bids, write_bid},
    config::{EVENT_ID, TOTAL_DONATION_FILENAME},
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
    println!("Initializing donation total");
    let event = fetch_event(EVENT_ID).await;
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
    let bids = fetch_bids(EVENT_ID).await;
    match bids {
        Ok(bids) => {
            for bid in &bids {
                if let Err(_) = write_bid(bid) {
                    println!("Failed to write bid {} to file!", &bid.full_name);
                }
            }
            println!("Finished writing bids to file!");
        }
        Err(_) => {
            println!("Failed to fetch initial bids!");
        }
    }

    println!("Initializing donations");
    let donations = fetch_donations(EVENT_ID).await;
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

    let shutdown = CancellationToken::new();
    let mut tasks = Vec::new();

    for &(name, url) in SOCKETS {
        let shutdown = shutdown.clone();
        let cookie = session_cookie.clone();

        tasks.push(tokio::spawn(async move {
            listen(name, url, &cookie, shutdown, &wrapped_donatons).await;
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
