//! Instant offline matching over the offered catalog scope.
//!
//! Launcher-style token matching, not scattered-letter subsequences: every
//! typed word must match a whole word, a word start, a 3+ character run inside
//! a word, or the initials of consecutive words ("dnd" for "Do Not Disturb").
//! Pure ranking with no dependencies and no network: the model upgrades results
//! when it answers, refusals still take precedence, scores never arm timers and
//! only explicit human gestures act on previews.

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Best score of one typed token against a field's words, or None.
fn token_score(token: &str, words: &[String]) -> Option<i32> {
    let mut best = None;
    let mut take = |score: i32| best = Some(best.map_or(score, |old: i32| old.max(score)));
    for (position, word) in words.iter().enumerate() {
        let first_word_bonus = if position == 0 { 10 } else { 0 };
        if word == token {
            take(100 + first_word_bonus);
        } else if word.starts_with(token) {
            take(60 + token.len() as i32 + first_word_bonus);
        } else if token.chars().count() >= 3 && word.contains(token) {
            take(30);
        }
    }
    // Initials of consecutive words, e.g. "dnd" -> "Do Not Disturb".
    let initials: String = words
        .iter()
        .filter_map(|word| word.chars().next())
        .collect();
    if token.chars().count() >= 2 && initials.contains(token) {
        take(40);
    }
    best
}

/// Field score: every token must match, otherwise the field does not match.
fn field_score(tokens: &[String], text: &str) -> Option<i32> {
    let words = words(text);
    tokens
        .iter()
        .map(|token| token_score(token, &words))
        .sum::<Option<i32>>()
}

/// Ranked offered indices for `query`, best first, at most `limit`.
/// Label matches count double; tool/interface/id are a secondary field. All
/// typed words must match within the same field, so unrelated words never
/// combine into a false hit.
pub fn rank(query: &str, items: &[crate::model::Item], limit: usize) -> Vec<usize> {
    let tokens = words(query);
    if tokens.is_empty() {
        return vec![];
    }
    let mut scored: Vec<(i32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let label = field_score(&tokens, &item.label).map(|score| score * 2);
            let secondary = field_score(
                &tokens,
                &format!("{} {} {}", item.tool, item.interface, item.id),
            );
            let total = label.into_iter().chain(secondary).max()?;
            Some((total, index))
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
    fn ranks_whole_words_then_word_starts_then_inner_runs() {
        let items = [
            item("a.open", "Open appearance settings"),
            item("volume", "Adjust volume"),
            item("vol-mgr", "Volume manager"),
            item("revolver", "Revolver history"),
        ];
        let ranked = rank("vol", &items, 10);
        assert_eq!(
            ranked,
            vec![2, 1, 3],
            "first-word start, later word start, inner run"
        );
        assert_eq!(rank("volume", &items, 10)[0], 2, "whole first word wins");
    }

    #[test]
    fn scattered_letters_do_not_match() {
        let items = [
            item("shot", "Take a screenshot"),
            item("lock", "Lock the screen"),
            item("rate", "Choose refresh rate"),
        ];
        assert!(
            rank("tree", &items, 10).is_empty(),
            "t..r..e..e is not a match"
        );
        assert_eq!(
            rank("rate", &items, 10),
            vec![2],
            "whole later word still found"
        );
        assert_eq!(
            rank("screen", &items, 10),
            vec![1, 0],
            "whole word beats prefix"
        );
    }

    #[test]
    fn multi_word_queries_need_every_word_in_one_field() {
        let items = [
            item("dnd", "Turn on Do Not Disturb"),
            item("mute", "Mute sound output"),
        ];
        assert_eq!(rank("do not", &items, 5), vec![0]);
        assert_eq!(
            rank("dnd", &items, 5),
            vec![0],
            "initials of consecutive words"
        );
        assert_eq!(rank("mute output", &items, 5), vec![1]);
        assert!(
            rank("mute disturb", &items, 5).is_empty(),
            "words from two items never combine"
        );
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
            "secondary field holds both words"
        );
        assert_eq!(
            rank("sys", &items, 5),
            vec![0],
            "tool/interface/id matches too"
        );
    }

    #[test]
    fn real_catalog_queries_behave() {
        let catalog = crate::model::embedded_catalog().unwrap();
        let labels = |query: &str| -> Vec<String> {
            rank(query, &catalog.items, 5)
                .into_iter()
                .map(|index| catalog.items[index].label.clone())
                .collect()
        };
        assert!(
            labels("tree").is_empty(),
            "no catalog item is about trees: {:?}",
            labels("tree")
        );
        assert!(labels("volum").iter().any(|label| label == "Adjust volume"));
        assert!(
            labels("dark")
                .iter()
                .any(|label| label == "Switch to dark mode")
        );
        assert!(
            labels("screenshot")
                .iter()
                .any(|label| label == "Take a screenshot")
        );
    }
}
