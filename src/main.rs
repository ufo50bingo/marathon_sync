mod auth;
mod config;
mod listener;
mod websocket;

use tokio_util::sync::CancellationToken;

use auth::{load_cookie, login, validate_cookie};
use config::SOCKETS;
use listener::listen;

#[tokio::main]
async fn main() {
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
            listen(name, url, &cookie, shutdown).await;
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
