//! Shared locale identifiers and resolution rules.
//!
//! UI catalogs remain in the frontends; the core only defines the stable
//! preference values and normalization policy both frontends follow.

pub const SYSTEM: &str = "system";
pub const EN_US: &str = "en-US";
pub const ZH_CN: &str = "zh-CN";

/// Normalizes a supported locale or alias to its canonical application code.
pub fn normalize_locale(value: &str) -> Option<&'static str> {
    let normalized = value.trim().replace('_', "-").to_ascii_lowercase();
    if normalized == "en" || normalized.starts_with("en-") {
        return Some(EN_US);
    }
    if matches!(normalized.as_str(), "zh" | "zh-cn" | "zh-sg" | "zh-hans")
        || normalized.starts_with("zh-hans-")
    {
        return Some(ZH_CN);
    }
    None
}

/// Resolves a stored preference against the current system locale.
/// Unknown explicit preferences fail closed to English.
pub fn resolve_locale(preference: &str, system_locale: Option<&str>) -> &'static str {
    if preference.eq_ignore_ascii_case(SYSTEM) {
        return system_locale.and_then(normalize_locale).unwrap_or(EN_US);
    }
    normalize_locale(preference).unwrap_or(EN_US)
}
