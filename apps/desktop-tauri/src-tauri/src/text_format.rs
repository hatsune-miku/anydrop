//! Receipt classification runs on the network worker, never in the webview.
//! Parsers consume the entire document; no YAML (plain prose is valid YAML).
pub fn detect(text: &str) -> Option<&'static str> {
    let input = text.trim();
    if input.is_empty() || text.len() > anydrop::MAX_CLIPBOARD_BYTES {
        return None;
    }
    if matches!(
        input.as_bytes()[0],
        b'{' | b'[' | b'"' | b'-' | b'0'..=b'9' | b't' | b'f' | b'n'
    ) && serde_json::from_str::<serde_json::Value>(input).is_ok()
    {
        return Some("JSON");
    }
    if input.starts_with('<') {
        // No external entity resolver: clipboard data never causes disk/network reads.
        let options = roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 1_100_000,
            ..Default::default()
        };
        if roxmltree::Document::parse_with_options(input, options).is_ok() {
            return Some("XML");
        }
    }
    if input.contains('=') {
        if let Ok(document) = input.parse::<toml_edit::Document>() {
            fn has_value(table: &toml_edit::Table) -> bool {
                table.iter().any(|(_, item)| {
                    item.is_value()
                        || item.as_table().is_some_and(has_value)
                        || item
                            .as_array_of_tables()
                            .is_some_and(|tables| tables.iter().any(has_value))
                })
            }
            if has_value(document.as_table()) {
                return Some("TOML");
            }
        }
    }
    None
}

pub fn receipt_title(text: &str) -> String {
    let Some(format) = detect(text) else {
        return "收到剪贴板文本".into();
    };
    let digits = text.chars().count().to_string();
    let mut count = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            count.push(',');
        }
        count.push(digit);
    }
    format!("收到 {format} ({count} 字符)")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_documents_only() {
        for text in [r#"{"x": [1, true, null]}"#, "[]", "42", "\"你好😀\""] {
            assert_eq!(detect(text), Some("JSON"), "{text}");
        }
        for text in [
            "<a/>",
            "<?xml version=\"1.0\"?><a>好</a>",
            "<!DOCTYPE a [<!ENTITY x 'ok'>]><a>&x;</a>",
        ] {
            assert_eq!(detect(text), Some("XML"), "{text}");
        }
        for text in ["a = 1", "[a]\nb = '好'", "a = {}", "[[a]]\nb = true"] {
            assert_eq!(detect(text), Some("TOML"), "{text}");
        }
        for text in [
            "",
            "  \n",
            "ordinary text",
            "a: 1",
            "- a\n- b",
            "{} trailing",
            "{\"a\":}",
            "<a/><b/>",
            "<a>",
            "<a>&missing;</a>",
            "# comment = 1",
            "[a]\n# x = 1",
            "a = 1\na = 2",
        ] {
            assert_eq!(detect(text), None, "{text}");
        }
    }
    #[test]
    fn counts_unicode_scalars_and_groups_digits() {
        assert_eq!(receipt_title("\"你好😀\""), "收到 JSON (5 字符)");
        assert_eq!(
            receipt_title(&format!("\"{}\"", "x".repeat(12343))),
            "收到 JSON (12,345 字符)"
        );
    }
}
