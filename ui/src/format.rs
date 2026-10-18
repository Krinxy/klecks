// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
// Small text helpers shared by the screens.
use wasm_bindgen::JsValue;

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "17 Oct, 10:25" in the user's local time.
pub fn when(modified_ms: u64) -> String {
    let date = js_sys::Date::new(&JsValue::from_f64(modified_ms as f64));
    format!(
        "{} {}, {:02}:{:02}",
        date.get_date(),
        MONTHS[date.get_month() as usize % 12],
        date.get_hours(),
        date.get_minutes()
    )
}

/// 12345678 -> "12,345,678"
pub fn with_commas(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// "1 video" / "3 videos"
pub fn count_of(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

/// 64 -> "1:04", 3725 -> "1:02:05"
pub fn clock(seconds: u32) -> String {
    let (hours, minutes, rest) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{rest:02}")
    } else {
        format!("{minutes}:{rest:02}")
    }
}

/// "Draft" from "draft".
pub fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Splat count in millions: 3_000_000 -> "3 million", 1_500_000 -> "1.5 million".
pub fn millions(value: u32) -> String {
    let millions = value as f32 / 1_000_000.0;
    if (millions - millions.round()).abs() < 0.05 {
        format!("{:.0} million", millions.round())
    } else {
        format!("{millions:.1} million")
    }
}
