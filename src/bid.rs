use std::io;

use reqwest::header::HeaderValue;
use serde::Deserialize;

use crate::{dollars::format_dollars, write_file::write_file};

#[derive(Deserialize)]
pub struct Bid {
    // r#type: String,
    // id: u64,
    // choice, option, challenge
    // redundant with istarget
    // bid_type: String,
    pub full_name: String,
    // goal: Option<f64>,
    total: f64,
}

#[derive(Deserialize)]
struct BidsResponse {
    next: Option<String>,
    results: Vec<Bid>,
}

pub async fn fetch_bids(
    event_id: u64,
    session_cookie: &HeaderValue,
) -> Result<Vec<Bid>, reqwest::Error> {
    let mut url = format!(
        "https://donate.cherry-rush.org/tracker/api/v2/events/{event_id}/bids/feed_all?limit=500"
    );

    let mut bids = Vec::new();

    loop {
        let response = reqwest::Client::new()
            .get(&url)
            .header(reqwest::header::COOKIE, session_cookie)
            // .header("Origin", "https://donate.cherry-rush.org")
            .send()
            .await?;

        let json = response.json::<BidsResponse>().await?;
        bids.extend(json.results);

        match json.next {
            Some(next) => url = next,
            None => break,
        }
    }

    Ok(bids)
}

fn strip_bid_name(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
        .map(|c| {
            if c == ' ' {
                '_'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

pub fn write_bid(bid: &Bid) -> io::Result<()> {
    let fname = format!("{}.txt", strip_bid_name(&bid.full_name));
    write_file(&format_dollars(bid.total, false), &fname)?;
    Ok(())
}
