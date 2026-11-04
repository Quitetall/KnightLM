//! A small, deliberately limited Markdown renderer for study notes.
//!
//! Study notes need headings, lists, emphasis, code and quotes — not a full
//! CommonMark implementation with HTML passthrough. Everything is **escaped
//! first and transformed after**, so a note can never inject markup: the output
//! is built from text this module produced, which is the only way a preview can
//! be safe while rendering what someone typed.
//!
//! What it does not do is stated rather than silently ignored: no tables, no
//! footnotes, no raw HTML, no nested lists. A writer who needs those needs a
//! different tool, and this module should not pretend otherwise.

/// Render note text as HTML.
pub fn render(source: &str) -> String {
    let mut out = String::new();
    let mut in_list = false;
    let mut in_code = false;
    let mut paragraph: Vec<String> = Vec::new();

    let flush_paragraph = |out: &mut String, paragraph: &mut Vec<String>| {
        if paragraph.is_empty() {
            return;
        }
        let joined = paragraph.join(" ");
        out.push_str("<p>");
        out.push_str(&inline(&joined));
        out.push_str("</p>\n");
        paragraph.clear();
    };

    for line in source.lines() {
        let trimmed = line.trim_end();

        // A fenced block keeps everything inside it literal.
        if trimmed.trim_start().starts_with("```") {
            flush_paragraph(&mut out, &mut paragraph);
            if in_code {
                out.push_str("</code></pre>\n");
            } else {
                out.push_str("<pre><code>");
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push_str(&escape(trimmed));
            out.push('\n');
            continue;
        }

        if trimmed.trim().is_empty() {
            flush_paragraph(&mut out, &mut paragraph);
            continue;
        }

        // Headings.
        if let Some(rest) = trimmed.strip_prefix("### ") {
            flush_paragraph(&mut out, &mut paragraph);
            out.push_str(&format!("<h3>{}</h3>\n", inline(rest)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            flush_paragraph(&mut out, &mut paragraph);
            out.push_str(&format!("<h2>{}</h2>\n", inline(rest)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            flush_paragraph(&mut out, &mut paragraph);
            out.push_str(&format!("<h1>{}</h1>\n", inline(rest)));
            continue;
        }

        // Quotes.
        if let Some(rest) = trimmed.strip_prefix("> ") {
            flush_paragraph(&mut out, &mut paragraph);
            out.push_str(&format!("<blockquote>{}</blockquote>\n", inline(rest)));
            continue;
        }

        // Lists: a run of them becomes one list.
        if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            flush_paragraph(&mut out, &mut paragraph);
            if !in_list {
                out.push_str("<ul>\n");
                in_list = true;
            }
            out.push_str(&format!("<li>{}</li>\n", inline(rest)));
            continue;
        }
        if let Some((number, rest)) = ordered_item(trimmed) {
            flush_paragraph(&mut out, &mut paragraph);
            if !in_list {
                out.push_str(&format!("<ol start=\"{number}\">\n"));
                in_list = true;
            }
            out.push_str(&format!("<li>{}</li>\n", inline(rest)));
            continue;
        }

        if in_list {
            out.push_str("</ul>\n");
            in_list = false;
        }
        paragraph.push(trimmed.trim().to_string());
    }

    flush_paragraph(&mut out, &mut paragraph);
    if in_list {
        out.push_str("</ul>\n");
    }
    if in_code {
        out.push_str("</code></pre>\n");
    }
    out
}

/// `1. text` and friends, returning the number and the rest.
fn ordered_item(line: &str) -> Option<(u32, &str)> {
    let (digits, rest) = line.split_once(". ")?;
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((digits.parse().ok()?, rest))
}

/// Escape the five characters that would otherwise become markup.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Inline spans, applied to already-escaped text.
///
/// Escaping first means the markers below can only ever see text: a note
/// containing `<b>` has it escaped before `**` is looked for, and the output is
/// assembled only from this function's own tags.
fn inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = escape(text);
    loop {
        // Code first, so emphasis inside backticks stays literal.
        if let Some((before, after)) = rest.split_once('`')
            && let Some((code, tail)) = after.split_once('`')
        {
            out.push_str(&emphasis(&before.replace("<b>", "")));
            out.push_str("<code>");
            out.push_str(code);
            out.push_str("</code>");
            rest = tail.to_string();
            continue;
        }
        out.push_str(&emphasis(&rest));
        break;
    }
    out
}

/// Bold and italic, in that order, on escaped text.
fn emphasis(text: &str) -> String {
    let bold = replace_pairs(text, "**", "<b>", "</b>");
    let italic = replace_pairs(&bold, "*", "<i>", "</i>");
    replace_pairs(&italic, "_", "<i>", "</i>")
}

/// Rewrite every `marker ... marker` pair, leaving unmatched markers alone.
fn replace_pairs(text: &str, marker: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after)) = rest.split_once(marker) {
        let Some((inner, tail)) = after.split_once(marker) else {
            out.push_str(before);
            out.push_str(marker);
            out.push_str(after);
            return out;
        };
        out.push_str(before);
        out.push_str(open);
        out.push_str(inner);
        out.push_str(close);
        rest = tail;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn headings_lists_and_emphasis_render() {
        let html = render("# Title\n\n- one\n- two\n\n**bold** and *italic*");
        assert!(html.contains("<h1>Title</h1>"), "{html}");
        assert!(html.contains("<li>one</li>"), "{html}");
        assert!(html.contains("<b>bold</b>"), "{html}");
        assert!(html.contains("<i>italic</i>"), "{html}");
    }

    #[test]
    fn html_in_a_note_is_escaped_not_executed() {
        let html = render("<script>alert(1)</script> and <b>not bold</b>");
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(!html.contains("<b>not bold"), "{html}");
    }

    #[test]
    fn a_fenced_block_keeps_its_text_literal() {
        let html = render("```\nlet x = *y*;\n```");
        assert!(html.contains("<pre><code>"), "{html}");
        assert!(html.contains("*y*"), "code is not emphasised: {html}");
    }

    #[test]
    fn an_unmatched_marker_stays_visible() {
        // A stray asterisk is text, not an error.
        let html = render("2 * 3 = 6");
        assert!(html.contains("2 * 3 = 6"), "{html}");
    }

    #[test]
    fn an_ordered_list_keeps_its_start() {
        let html = render("3. three\n4. four");
        assert!(html.contains("<ol start=\"3\">"), "{html}");
        assert!(html.contains("<li>four</li>"), "{html}");
    }

    #[test]
    fn a_quote_becomes_a_blockquote() {
        assert!(render("> a thought").contains("<blockquote>a thought</blockquote>"));
    }
}
