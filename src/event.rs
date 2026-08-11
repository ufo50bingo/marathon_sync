use serde::Deserialize;

#[derive(Deserialize)]
pub struct Event {
    // r#type: String,
    // short: String,
    // name: String,
    // id: u64,
    pub donation_total: f64,
}

pub async fn fetch_event(id: u64) -> Result<Event, reqwest::Error> {
    let url = format!("https://donate.cherry-rush.org/tracker/api/v2/events/{id}/?totals");
    let event = reqwest::get(&url).await?.json::<Event>().await?;
    Ok(event)
}
