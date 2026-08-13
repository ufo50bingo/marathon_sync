use std::io;

use serde::Deserialize;

use crate::bid::Bid;
use crate::config::MAX_DONATIONS;
use crate::dollars::format_dollars;
use crate::write_file;

// do we need to do something about currency?
#[derive(Deserialize)]
pub struct DonationMessage {
    pub event: u64,
    pub all_donors_event_total: f64,
    pub amount: f64,
    // pub timereceived: String,
    pub bids: Vec<Bid>,
    #[allow(non_snake_case)]
    donor__visiblename: String,
}

#[derive(Deserialize)]
pub struct DonationsResponse {
    results: Vec<Donation>,
}

#[derive(Deserialize)]
pub struct Donation {
    donor_name: String,
    amount: f64,
}

pub fn get_donation_from_message(donation: DonationMessage) -> Donation {
    Donation {
        donor_name: donation.donor__visiblename,
        amount: donation.amount,
    }
}

pub async fn fetch_donations(event_id: u64) -> Result<Vec<Donation>, reqwest::Error> {
    let url = format!(
        "https://donate.cherry-rush.org/tracker/api/v2/events/{event_id}/donations/?limit={MAX_DONATIONS}"
    );
    let response = reqwest::get(&url)
        .await?
        .json::<DonationsResponse>()
        .await?;
    Ok(response.results)
}

pub fn write_donations(donations: &Vec<Donation>) -> io::Result<()> {
    for (index, donation) in donations.iter().enumerate() {
        write_file(
            &format_dollars(donation.amount, false),
            &format!("donation_amount_{index}.txt"),
        )?;
        write_file(&donation.donor_name, &format!("donor_name_{index}.txt"))?;
    }
    for index in donations.len()..20 {
        write_file("", &format!("donation_amount_{index}.txt"))?;
        write_file("", &format!("donor_name_{index}.txt"))?;
    }
    Ok(())
}
