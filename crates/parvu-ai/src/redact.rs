//! Removing API keys from text before it is logged or shown (plans/AI_PLAN.md
//! §8.4).

/// Key-like tokens replaced with `***`: Anthropic (`sk-ant-…`), OpenAI and
/// OpenRouter (`sk-…`), Groq (`gsk_…`), Google (`AIza…`), and anything after
/// `Bearer ` or `x-api-key`.
pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let next = ["sk-", "gsk_", "AIza", "Bearer ", "x-api-key: ", "x-api-key\":\""]
            .iter()
            .filter_map(|p| rest.find(p).map(|i| (i, *p)))
            .min_by_key(|(i, _)| *i);
        let Some((i, prefix)) = next else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..i]);
        let after = &rest[i + prefix.len()..];
        let token_len = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_')).unwrap_or(after.len());
        if token_len >= 8 || prefix.starts_with("Bearer") || prefix.starts_with("x-api-key") {
            let keep = if prefix.starts_with("Bearer") || prefix.starts_with("x-api-key") { prefix } else { "" };
            out.push_str(keep);
            out.push_str("***");
        } else {
            out.push_str(prefix);
            out.push_str(&after[..token_len]);
        }
        rest = &after[token_len..];
    }
    out
}

/// Whether text looks like it contains an API key (warn before sending a
/// chat message that does).
pub fn looks_like_key(text: &str) -> bool {
    redact(text) != text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_keys_and_keeps_the_rest() {
        assert_eq!(redact("key sk-ant-api03-abcdefgh1234 done"), "key *** done");
        assert_eq!(redact("Authorization: Bearer abc.def"), "Authorization: Bearer ***.def");
        assert_eq!(redact("gsk_0123456789abcdef and AIzaSyA1234567890"), "*** and ***");
        assert_eq!(redact("task-list and sk-1 short"), "task-list and sk-1 short");
        assert!(looks_like_key("my key is sk-proj-1234567890abcdef"));
        assert!(!looks_like_key("SELECT * FROM sales"));
    }
}
