//! Text normalization for semantic matching.
//!
//! Cleans and standardizes event descriptions to improve matching accuracy.

use std::sync::OnceLock;
use rustc_hash::FxHashMap;

/// Synonym mappings for common terms
static SYNONYMS: OnceLock<FxHashMap<&'static str, &'static str>> = OnceLock::new();

fn get_synonyms() -> &'static FxHashMap<&'static str, &'static str> {
    SYNONYMS.get_or_init(|| {
        let mut map = FxHashMap::default();
        
        // Outcome synonyms
        map.insert("win", "winner");
        map.insert("wins", "winner");
        map.insert("victory", "winner");
        map.insert("victorious", "winner");
        map.insert("defeat", "loser");
        map.insert("lose", "loser");
        map.insert("loses", "loser");
        
        // Team/player terms
        map.insert("ml", "moneyline");
        map.insert("team", "");
        map.insert("player", "");
        
        // Political terms
        map.insert("election", "");
        map.insert("presidency", "president");
        map.insert("presidential", "president");
        map.insert("democrat", "democratic");
        map.insert("republican", "republicans");
        map.insert("gop", "republicans");
        
        // Sports terms
        map.insert("vs", "versus");
        map.insert("@", "at");
        
        map
    })
}

/// Normalize event description for matching
///
/// Steps:
/// 1. Lowercase
/// 2. Remove special characters (keep alphanumeric + spaces)
/// 3. Remove common dates and odds patterns
/// 4. Apply synonym substitution
/// 5. Collapse whitespace
pub fn normalize_text(text: &str) -> String {
    // Step 1: Lowercase
    let mut result = text.to_lowercase();

    // Step 2: Remove dates (YYYY-MM-DD, MM/DD/YYYY, etc.)
    result = remove_dates(&result);

    // Step 3: Remove odds and probability patterns
    result = remove_odds(&result);

    // Step 4: Remove special characters (keep letters, numbers, spaces)
    result = result.chars()
        .map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' })
        .collect();

    // Step 5: Apply synonyms
    result = apply_synonyms(&result);

    // Step 6: Remove extra whitespace
    result = result.split_whitespace().collect::<Vec<_>>().join(" ");

    // Step 7: Trim
    result.trim().to_string()
}

/// Remove date patterns from text
fn remove_dates(text: &str) -> String {
    let result = text.to_string();
    
    // Common date patterns to remove
    // TODO: Implement date pattern removal using regex crate if needed
    // For now, this is a no-op as simple string matching would be insufficient
    // Examples to handle:
    // - ISO format: 2024-01-15
    // - US format: 01/15/2024 or 1/15/24
    // - Month names: January 15, Jan 15

    result
}

/// Remove odds and probability patterns
fn remove_odds(text: &str) -> String {
    // Remove patterns like: +150, -110, 55%, 0.55
    let chars: Vec<char> = text.chars().collect();
    let mut cleaned = String::with_capacity(text.len());
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '+' || chars[i] == '-' {
            // Check if followed by digits (odds format)
            if i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                // Skip the odds
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                continue;
            }
        } else if chars[i].is_ascii_digit() {
            // Check for percentage (e.g., 55%)
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < chars.len() && chars[i] == '%' {
                // Skip percentage
                i += 1;
                continue;
            }
            // Not a percentage, keep the digits
            for j in start..i {
                cleaned.push(chars[j]);
            }
            continue;
        }
        
        cleaned.push(chars[i]);
        i += 1;
    }

    cleaned
}

/// Apply synonym substitution
fn apply_synonyms(text: &str) -> String {
    let synonyms = get_synonyms();
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut result = Vec::with_capacity(words.len());

    for word in words {
        if let Some(replacement) = synonyms.get(word) {
            if !replacement.is_empty() {
                result.push(*replacement);
            }
            // If empty, skip the word
        } else {
            result.push(word);
        }
    }

    result.join(" ")
}

/// Extract team names from event description
///
/// Attempts to extract team/participant names from common formats:
/// - "Team A vs Team B"
/// - "Team A @ Team B"
/// - "Team A - Team B"
pub fn extract_teams(text: &str) -> Option<(String, String)> {
    let normalized = normalize_text(text);
    
    // Look for separators
    for separator in &["versus", "vs", "at", "-"] {
        if let Some(pos) = normalized.find(separator) {
            let team1 = normalized[..pos].trim();
            let after_sep = pos + separator.len();
            if after_sep < normalized.len() {
                let team2 = normalized[after_sep..].trim();
                if !team1.is_empty() && !team2.is_empty() {
                    return Some((team1.to_string(), team2.to_string()));
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_basic() {
        assert_eq!(
            normalize_text("Lakers vs Warriors"),
            "lakers versus warriors"
        );
    }

    #[test]
    fn test_normalize_synonyms() {
        assert_eq!(
            normalize_text("Trump wins the election"),
            "trump winner the"
        );
    }

    #[test]
    fn test_normalize_special_chars() {
        assert_eq!(
            normalize_text("Inter vs. Lecce!"),
            "inter versus lecce"
        );
    }

    #[test]
    fn test_remove_odds() {
        let text = "Lakers -110 vs Warriors +150";
        let normalized = normalize_text(text);
        assert!(!normalized.contains("110"));
        assert!(!normalized.contains("150"));
    }

    #[test]
    fn test_extract_teams() {
        let (t1, t2) = extract_teams("Lakers vs Warriors").unwrap();
        assert_eq!(t1, "lakers");
        assert_eq!(t2, "warriors");
    }
}
