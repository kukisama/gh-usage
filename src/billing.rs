//! Billing details shared by journal readers and historical CSV imports.

pub(crate) fn parse_credit_details(details: &str) -> Option<(String, f64)> {
    let trimmed = details.trim();
    let before_suffix = trimmed
        .strip_suffix(" credits")
        .or_else(|| trimmed.strip_suffix(" credit"))?;
    let mut parts = before_suffix.rsplitn(2, char::is_whitespace);
    let credits_text = parts.next()?.trim();
    let model_part = parts.next()?.trim();

    let credits = credits_text.parse::<f64>().ok()?;
    let model = model_part
        .trim_end_matches(|ch: char| {
            ch.is_whitespace() || !(ch.is_ascii_alphanumeric() || ch == ')' || ch == ']')
        })
        .trim()
        .to_owned();

    if model.is_empty() || !credits.is_finite() || credits < 0.0 {
        None
    } else {
        Some((model, credits))
    }
}

pub(crate) fn parse_premium_details(details: &str) -> Option<(String, f64)> {
    let (model, usage) = details.trim().rsplit_once('•')?;
    let usage = usage.trim();
    let value = usage.strip_suffix(" premium requests")
        .or_else(|| usage.strip_suffix(" premium request"))
        .or_else(|| usage.strip_suffix(['x', '×']))?.trim().parse::<f64>().ok()?;
    let model = model.trim();
    (!model.is_empty() && value.is_finite() && value >= 0.0).then(|| (model.to_owned(), value))
}