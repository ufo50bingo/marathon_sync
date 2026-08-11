use reqwest;
use serde::Deserialize;

#[derive(Deserialize)]
struct Event {
    r#type: String,
    short: String,
    donation_total: f64,
    donation_count: u64,
    donation_max: f64,
}

pub fn main() {
    // totals
    let url = "https://donate.cherry-rush.org/tracker/api/v2/events/2/?totals";
    // incentives
    // let url = "https://donate.cherry-rush.org/tracker/api/v2/bids/";
    let result = reqwest::blocking::get(url)
        .unwrap()
        .json::<Event>()
        .unwrap();

    println!("{} {} {} {} {}", result.r#type, result.short, result.donation_total, result.donation_count, result.donation_max);
}
