// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use leit_text::{Analyzer, CaseMapping, Token, Tokenizer, UnicodeNormalizer};

#[derive(Debug)]
pub(crate) struct IssueTokenizer;

impl Tokenizer for IssueTokenizer {
    fn tokenize<'a>(&self, text: &'a str, output: &mut Vec<Token<'a>>) {
        let mut start = None;
        let mut position = 0_u32;
        for (offset, ch) in text
            .char_indices()
            .chain(std::iter::once((text.len(), ' ')))
        {
            let separator = ch.is_whitespace()
                || ch.is_ascii_punctuation()
                || matches!(ch, '“' | '”' | '‘' | '’' | '—' | '–' | '…');
            if separator {
                if let Some(begin) = start.take() {
                    output.push(Token::new(&text[begin..offset], position, begin, offset));
                    position = position.saturating_add(1);
                }
            } else {
                start.get_or_insert(offset);
            }
        }
    }
}

pub(crate) fn analyzer() -> Analyzer {
    Analyzer::new(IssueTokenizer).with_normalizer(
        UnicodeNormalizer::builder()
            .case_mapping(CaseMapping::Fold)
            .build(),
    )
}

pub(crate) fn terms(text: &str) -> Vec<String> {
    let mut terms: Vec<_> = analyzer()
        .analyze(text)
        .into_iter()
        .map(|(_, word)| word)
        .collect();
    terms.sort();
    terms.dedup();
    terms
}

// Terminal output must not interpret control sequences from imported text.
pub(crate) fn plain(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn excerpt(text: &str, query: &[String]) -> String {
    let tokens = analyzer().analyze(text);
    let focus = tokens
        .iter()
        .find(|(_, word)| query.contains(word))
        .map_or(0, |(t, _)| t.byte_range.start);
    let chars: Vec<_> = text.char_indices().collect();
    let center = chars.partition_point(|(byte, _)| *byte < focus);
    let start = center.saturating_sub(50);
    let end = (start + 240).min(chars.len());
    let from = chars.get(start).map_or(text.len(), |(b, _)| *b);
    let to = chars.get(end).map_or(text.len(), |(b, _)| *b);
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        plain(&text[from..to]),
        if end < chars.len() { "…" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tokenizes_identifiers_and_normalizes_unicode() {
        assert_eq!(
            terms("Foo::BAR(foo_bar) CAFÉ cafe\u{301} Straße"),
            ["bar", "café", "foo", "strasse"]
        );
    }
    #[test]
    fn excerpt_reaches_late_match_on_character_boundaries() {
        let input = format!("{} atlas lifetime", "界".repeat(400));
        let output = excerpt(&input, &["atlas".into()]);
        assert!(output.contains("atlas lifetime"));
        assert!(output.starts_with('…'));
        assert!(!plain("\u{1b}[31m\nhello").contains('\u{1b}'));
    }
}
