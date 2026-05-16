//! Value encoding for PostgREST query parameters.
//!
//! Anything that implements `Display` becomes a `PostgrestValue` automatically.
//! Special encodings (`is.null`, lists, ranges) are handled by the call sites
//! rather than the type.

use std::fmt::Display;

/// A value that can be serialized into a PostgREST query parameter.
pub trait PostgrestValue {
    /// Render the value as its textual form. This is *not* URL-encoded —
    /// the builder applies encoding when assembling the final URL.
    fn render(&self) -> String;
}

impl<T: Display + ?Sized> PostgrestValue for T {
    fn render(&self) -> String {
        self.to_string()
    }
}

/// URL-encode a PostgREST query parameter value. Commas and dots are kept
/// literal because PostgREST treats them as delimiters.
pub(crate) fn encode_value(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

/// URL-encode a column name (kept simple — column names should match `\w+`).
pub(crate) fn encode_column(column: &str) -> String {
    urlencoding::encode(column).into_owned()
}

/// Render a list of values as PostgREST's `(a,b,c)` syntax with each
/// element individually escaped if it contains commas or parens.
pub(crate) fn render_list<I, V>(values: I) -> String
where
    I: IntoIterator<Item = V>,
    V: PostgrestValue,
{
    let parts: Vec<String> = values
        .into_iter()
        .map(|v| {
            let raw = v.render();
            if raw.contains([',', '(', ')', '"']) {
                // Quote and escape embedded quotes.
                format!("\"{}\"", raw.replace('"', "\\\""))
            } else {
                raw
            }
        })
        .collect();
    format!("({})", parts.join(","))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn encode_value_plain_ascii_passthrough() {
        assert_eq!(encode_value("hello"), "hello");
        assert_eq!(encode_value("active"), "active");
    }

    #[test]
    fn encode_value_special_chars() {
        assert_eq!(encode_value("hello world"), "hello%20world");
        assert_eq!(encode_value("a&b"), "a%26b");
        assert_eq!(encode_value("foo=bar"), "foo%3Dbar");
        assert_eq!(encode_value("a+b"), "a%2Bb");
        assert_eq!(encode_value("100%"), "100%25");
    }

    #[test]
    fn encode_value_unicode() {
        // Non-ASCII characters are percent-encoded.
        let encoded = encode_value("café");
        assert!(encoded.contains('%'), "expected percent-encoding: {encoded}");
    }

    #[test]
    fn encode_value_empty() {
        assert_eq!(encode_value(""), "");
    }

    #[test]
    fn encode_column_simple() {
        assert_eq!(encode_column("user_id"), "user_id");
        assert_eq!(encode_column("firstName"), "firstName");
    }

    #[test]
    fn encode_column_dot_notation_preserved() {
        // Embedded-resource columns like "profile.name" must be kept as-is
        // since PostgREST treats the dot as a relationship separator.
        let col = "profile.name";
        let encoded = encode_column(col);
        // urlencoding does NOT encode dots, so the dot passes through.
        assert_eq!(encoded, "profile.name");
    }

    #[test]
    fn render_list_basic() {
        let result = render_list(["a", "b", "c"]);
        assert_eq!(result, "(a,b,c)");
    }

    #[test]
    fn render_list_empty() {
        let result = render_list(Vec::<&str>::new());
        assert_eq!(result, "()");
    }

    #[test]
    fn render_list_single() {
        let result = render_list(["only"]);
        assert_eq!(result, "(only)");
    }

    #[test]
    fn render_list_values_with_commas_are_quoted() {
        let result = render_list(["a,b", "c"]);
        assert_eq!(result, r#"("a,b",c)"#);
    }

    #[test]
    fn render_list_values_with_parens_are_quoted() {
        let result = render_list(["(nested)"]);
        assert_eq!(result, r#"("(nested)")"#);
    }

    #[test]
    fn render_list_values_with_embedded_quotes_escaped() {
        let result = render_list([r#"say "hi""#]);
        assert_eq!(result, r#"("say \"hi\"")"#);
    }

    #[test]
    fn render_list_integers() {
        let result = render_list([1i32, 2, 3, 4]);
        assert_eq!(result, "(1,2,3,4)");
    }

    #[test]
    fn postgrest_value_for_bool() {
        assert_eq!(true.render(), "true");
        assert_eq!(false.render(), "false");
    }

    #[test]
    fn postgrest_value_for_numeric() {
        assert_eq!(42i32.render(), "42");
        assert_eq!(2.5f64.render(), "2.5");
        assert_eq!((-7i64).render(), "-7");
    }

    #[test]
    fn postgrest_value_for_string_ref() {
        let s = String::from("hello");
        assert_eq!(s.render(), "hello");
        assert_eq!("hello".render(), "hello");
    }
}
