use futures_util::{SinkExt, StreamExt};
use reqwest::cookie::{CookieStore, Jar};
use scraper::{Html, Selector};
use std::time::Duration;
use std::{
    fs,
    io::{self, Write},
    sync::Arc,
};
use tokio::time::sleep;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tokio_util::sync::CancellationToken;
use url::Url;

const ADMIN_LOGIN_URL: &str = "https://donate.cherry-rush.org/admin/login/";
const COOKIE_FILE: &str = "cookie.txt";

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

/// Load the saved sessionid from cookie.txt.
///
/// The file contains:
///
/// sessionid=abc123...
fn load_cookie() -> Option<String> {
    let contents = fs::read_to_string(COOKIE_FILE).ok()?;

    let cookie = contents.trim();

    if cookie.starts_with("sessionid=") && cookie.len() > "sessionid=".len() {
        Some(cookie.to_string())
    } else {
        eprintln!("Ignoring malformed {COOKIE_FILE}");
        None
    }
}

/// Save the session cookie.
///
/// On Unix-like systems, try to make the file readable only by the
/// current user because the session cookie is effectively a credential.
fn save_cookie(cookie: &str) -> io::Result<()> {
    fs::write(COOKIE_FILE, cookie)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(COOKIE_FILE)?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(COOKIE_FILE, permissions)?;
    }

    Ok(())
}

/// Try the saved cookie against one of the WebSocket endpoints.
///
/// A successful WebSocket handshake means the server accepted the
/// connection with this session cookie.
async fn validate_cookie(cookie: &str) -> bool {
    let (_, url) = SOCKETS[0];

    println!("Checking saved session against {url}...");

    match connect_socket(url, cookie).await {
        Ok((_stream, response)) => {
            println!(
                "Saved session accepted by WebSocket (HTTP status {}).",
                response.status()
            );
            true
        }

        Err(error) => {
            println!("Saved session rejected: {error}");
            false
        }
    }
}

/// Perform the Django admin login and return the resulting session cookie.
async fn login() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    println!();
    println!("Django admin login");
    println!("------------------");

    print!("Username: ");
    io::stdout().flush()?;

    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    let username = username.trim();

    let password = rpassword::prompt_password("Password: ")?;

    // A cookie jar lets reqwest retain both the CSRF cookie and the
    // session cookie across the GET and POST requests.
    let jar = Arc::new(Jar::default());

    let client = reqwest::Client::builder()
        .cookie_provider(jar.clone())
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let login_url = Url::parse(ADMIN_LOGIN_URL)?;

    // First request the login page so Django gives us a CSRF token.
    println!("Fetching login page...");

    let response = client.get(ADMIN_LOGIN_URL).send().await?;

    if !response.status().is_success() {
        return Err(format!("GET {ADMIN_LOGIN_URL} returned HTTP {}", response.status()).into());
    }

    let html = response.text().await?;

    let csrf_token = extract_csrf_token(&html)
        .ok_or("Could not find csrfmiddlewaretoken in the Django login page")?;

    // Django's admin login form uses these fields.
    let params = [
        ("username", username),
        ("password", password.as_str()),
        ("csrfmiddlewaretoken", csrf_token.as_str()),
        ("next", "/admin/"),
    ];

    println!("Logging in...");

    let response = client
        .post(ADMIN_LOGIN_URL)
        .header("Referer", ADMIN_LOGIN_URL)
        .form(&params)
        .send()
        .await?;

    let final_url = response.url().clone();
    let status = response.status();

    // Django normally redirects a successful login to /admin/.
    // If we ended up back at /admin/login/, login failed.
    let login_path = final_url.path().trim_end_matches('/');

    if !status.is_success() {
        return Err(format!("Login request returned HTTP {status}").into());
    }

    if login_path == "/admin/login" {
        return Err("Django rejected the username/password (still on the login page)".into());
    }

    // Ask the cookie jar what cookies it has for the site.
    let cookie_header = jar
        .cookies(&login_url)
        .ok_or("No cookies were returned by Django")?
        .to_str()
        .map_err(|_| "Cookie header contains invalid characters")?
        .to_string();

    let session_cookie = cookie_header
        .split(';')
        .map(str::trim)
        .find(|cookie| cookie.starts_with("sessionid="))
        .ok_or("Django login succeeded, but no sessionid cookie was found")?
        .to_string();

    save_cookie(&session_cookie)?;

    println!("Login successful.");
    println!("Session saved to {COOKIE_FILE}.");

    Ok(session_cookie)
}

/// Extract Django's hidden csrfmiddlewaretoken input.
fn extract_csrf_token(html: &str) -> Option<String> {
    let document = Html::parse_document(html);

    let selector = Selector::parse(r#"input[name="csrfmiddlewaretoken"]"#).ok()?;

    document
        .select(&selector)
        .next()
        .and_then(|element| element.value().attr("value"))
        .map(str::to_string)
}

/// Connect to a WebSocket using the supplied session cookie.
async fn connect_socket(
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

#[tokio::main]
async fn main() {
    // ------------------------------------------------------------------
    // Login / load saved cookie
    // ------------------------------------------------------------------

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

    // ------------------------------------------------------------------
    // Start listeners
    // ------------------------------------------------------------------

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

    // ------------------------------------------------------------------
    // Wait for Ctrl+C
    // ------------------------------------------------------------------

    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("Failed to listen for Ctrl+C: {error}");
    }

    println!();
    println!("Ctrl+C received.");
    println!("Shutting down listeners...");

    // Tell all listener tasks to shut down.
    shutdown.cancel();

    // Wait for every listener to finish its graceful shutdown.
    for task in tasks {
        if let Err(error) = task.await {
            eprintln!("Listener task failed: {error}");
        }
    }

    println!("Shutdown complete.");
}

/// Listen forever, automatically reconnecting when the WebSocket
/// connection is lost.
async fn listen(name: &str, url: &str, cookie: &str, shutdown: CancellationToken) {
    // Start with a short reconnect delay.
    //
    // It increases after repeated failures, up to MAX_RECONNECT_DELAY.
    let mut reconnect_delay = Duration::from_secs(1);

    const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

    loop {
        // Check whether shutdown was requested before attempting
        // another connection.
        if shutdown.is_cancelled() {
            println!("[{name}] Shutdown requested.");
            break;
        }

        println!("[{name}] Connecting to {url}");

        match connect_socket(url, cookie).await {
            Ok((ws_stream, response)) => {
                println!("[{name}] Connected (HTTP status {})", response.status());

                // Successful connection: reset the reconnect delay.
                reconnect_delay = Duration::from_secs(1);

                // Run this connection until either:
                //
                //   1. the server disconnects us, or
                //   2. Ctrl+C causes shutdown.cancel()
                //
                // returns.
                let connection_result = run_connection(name, ws_stream, shutdown.clone()).await;

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

        // Don't reconnect immediately.
        //
        // IMPORTANT: sleep is also cancellable, so Ctrl+C doesn't
        // leave us waiting for the entire reconnect delay.
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

        // Exponential backoff:
        //
        // 1s -> 2s -> 4s -> 8s -> 16s -> 30s -> 30s...
        reconnect_delay = std::cmp::min(reconnect_delay * 2, MAX_RECONNECT_DELAY);
    }

    println!("[{name}] Listener stopped.");
}

enum ConnectionResult {
    Shutdown,
    Disconnected(String),
}

/// Process one WebSocket connection.
///
/// Returns when the server disconnects us or when shutdown is requested.
async fn run_connection(
    name: &str,
    ws_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    shutdown: CancellationToken,
) -> ConnectionResult {
    let (mut write, mut read) = ws_stream.split();

    loop {
        tokio::select! {
            // ----------------------------------------------------------
            // WebSocket message
            // ----------------------------------------------------------
            result = read.next() => {
                match result {
                    Some(Ok(Message::Text(text))) => {
                        println!("\n[{name}] MESSAGE:");
                        println!("{text}");
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

                        // tungstenite normally handles the Pong
                        // response automatically when the stream
                        // is driven.
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

            // ----------------------------------------------------------
            // Shutdown
            // ----------------------------------------------------------
            _ = shutdown.cancelled() => {
                println!(
                    "[{name}] Closing WebSocket..."
                );

                // Try to perform the WebSocket close handshake.
                //
                // If the server doesn't respond, we still return and
                // allow the stream to be dropped.
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
