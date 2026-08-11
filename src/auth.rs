use reqwest::cookie::{CookieStore, Jar};
use scraper::{Html, Selector};
use std::{
    fs,
    io::{self, Write},
    sync::Arc,
};
use url::Url;

use crate::{
    config::{ADMIN_LOGIN_URL, COOKIE_FILE, SOCKETS},
    websocket::connect_socket,
};

pub fn load_cookie() -> Option<String> {
    let contents = fs::read_to_string(COOKIE_FILE).ok()?;

    let cookie = contents.trim();

    if cookie.starts_with("sessionid=") && cookie.len() > "sessionid=".len() {
        Some(cookie.to_string())
    } else {
        eprintln!("Ignoring malformed {COOKIE_FILE}");
        None
    }
}

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

pub async fn validate_cookie(cookie: &str) -> bool {
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

pub async fn login() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    println!();
    println!("Django admin login");
    println!("------------------");

    print!("Username: ");
    io::stdout().flush()?;

    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    let username = username.trim();

    let password = rpassword::prompt_password("Password: ")?;

    let jar = Arc::new(Jar::default());

    let client = reqwest::Client::builder()
        .cookie_provider(jar.clone())
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let login_url = Url::parse(ADMIN_LOGIN_URL)?;

    println!("Fetching login page...");

    let response = client.get(ADMIN_LOGIN_URL).send().await?;

    if !response.status().is_success() {
        return Err(format!("GET {ADMIN_LOGIN_URL} returned HTTP {}", response.status()).into());
    }

    let html = response.text().await?;

    let csrf_token = extract_csrf_token(&html)
        .ok_or("Could not find csrfmiddlewaretoken in the Django login page")?;

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

    let login_path = final_url.path().trim_end_matches('/');

    if !status.is_success() {
        return Err(format!("Login request returned HTTP {status}").into());
    }

    if login_path == "/admin/login" {
        return Err("Django rejected the username/password (still on the login page)".into());
    }

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

fn extract_csrf_token(html: &str) -> Option<String> {
    let document = Html::parse_document(html);

    let selector = Selector::parse(r#"input[name="csrfmiddlewaretoken"]"#).ok()?;

    document
        .select(&selector)
        .next()
        .and_then(|element| element.value().attr("value"))
        .map(str::to_string)
}
