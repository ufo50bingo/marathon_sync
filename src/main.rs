pub fn main() {
    // totals
    // let url = "https://donate.cherry-rush.org/tracker/api/v2/events/2/?totals";
    // incentives
    let url = "https://donate.cherry-rush.org/tracker/api/v2/bids/";
    let result = reqwest::blocking::get(url)
        .expect("request failed")
        .text()
        .expect("body failed");

    println!("{}", result);
}
