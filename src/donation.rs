use serde::Deserialize;

use crate::bid::Bid;

// do we need to do something about currency?
#[derive(Deserialize)]
pub struct DonationMessage {
    pub event: u64,
    pub all_donors_event_total: f64,
    pub amount: f64,
    // pub timereceived: String,
    pub bids: Vec<Bid>,
}
