use num_format::Locale;
use num_format::ToFormattedString;

pub fn format_dollars(value: f64) -> String {
    let total_cents = (value * 100.0).round() as i64;

    let dollars = total_cents / 100;
    let cents = total_cents.abs() % 100;

    format!("${}.{:02}", dollars.to_formatted_string(&Locale::en), cents)
}
