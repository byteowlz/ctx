//! Instant offline fuzzy matching over the offered catalog scope.
//!
//! Pure ranking, no dependencies and no network: the model upgrades results
//! when it answers, and refusals still take precedence. Scores never arm
//! timers or execute anything; only explicit human gestures act on previews.

/// Best subsequence score of `query` inside `text`, case-insensitive.
/// None when the query is not a subsequence. Bonus for word starts,
/// consecutive runs and leading prefixes; small gap penalties.
fn score(query: &str, text: &str) -> Option<i32> {
    let query: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if query.is_empty() {
        return None;
    }
    let text: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let mut total = 0;
    let mut at = 0;
    let mut previous_matched = false;
    for (position, wanted) in query.iter().copied().enumerate() {
        let found = text[at..].iter().position(|got| *got == wanted)? + at;
        total += 1;
        if found == 0 {
            total += 16; // word or text start
        } else if !text[found - 1].is_alphanumeric() {
            total += 12; // word boundary
        } else if previous_matched && found == at {
            total += 8; // consecutive run
        }
        if position == 0 && found == 0 && query.len() == text.len() {
            total += 24; // exact-length prefix
        }
        total -= ((found - at).min(8)) as i32; // gap penalty, bounded
        at = found + 1;
        previous_matched = true;
    }
    Some(total)
}

/// Ranked offered indices for `query`, best first, at most `limit`.
/// An item matches on its label (weighted double) or its tool/interface/id.
pub fn rank(query: &str, items: &[crate::model::Item], limit: usize) -> Vec<usize> {
    let query = query.trim();
    if query.is_empty() {
        return vec![];
    }
    let mut scored: Vec<(i32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let label = score(query, &item.label);
            let secondary = score(
                query,
                &format!("{} {} {}", item.tool, item.interface, item.id),
            );
            let total = match (label, secondary) {
                (None, None) => return None,
                (Some(label), secondary) => label * 2 + secondary.unwrap_or(0),
                (None, Some(secondary)) => secondary,
            };
            (total > 0).then_some((total, index))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, index)| index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Item;

    fn item(id: &str, label: &str) -> Item {
        serde_json::from_value(serde_json::json!({
            "id": id, "tool": "System", "interface": "Sound",
            "label": label, "description": "d", "icon": "sun"
        }))
        .unwrap()
    }

    #[test]
    fn ranks_prefixes_word_starts_and_consecutive_runs_first() {
        let items = [
            item("a.open", "Open appearance settings"),
            item("volume", "Adjust volume"),
            item("vol-mgr", "Volume manager"), // word start + consecutive
            item("revolver", "Revolver history"), // subsequence only
        ];
        let ranked = rank("vol", &items, 10);
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0], 2, "word-start consecutive wins: {ranked:?}");
        assert_eq!(ranked[1], 1, "label word start beats mid-word subsequence");
        assert!(ranked.contains(&3));
        assert!(!ranked.contains(&0), "missing characters must not match");
    }

    #[test]
    fn empty_and_limit_edges() {
        let items = [item("a", "Alpha"), item("b", "Beta")];
        assert!(rank("", &items, 5).is_empty());
        assert!(rank("   ", &items, 5).is_empty());
        assert!(rank("zzz", &items, 5).is_empty());
        assert_eq!(rank("a", &items, 1).len(), 1);
    }

    #[test]
    fn case_and_secondary_fields_match() {
        let items = [item("sound.volume", "Adjust volume")];
        assert_eq!(rank("VOL", &items, 5), vec![0]);
        assert_eq!(
            rank("sound vol", &items, 5),
            vec![0],
            "secondary field matches"
        );
        assert_eq!(
            rank("sys", &items, 5),
            vec![0],
            "tool/interface/id matches too"
        );
    }
}
