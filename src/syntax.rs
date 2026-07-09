//! DopeSyntax → style spans (markers stay visible).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleKind {
    Heading1,
    Heading2,
    Heading3,
    Bullet,
    TaskPending,
    TaskDone,
    Quote,
    Callout,
    Highlight,
    Bold,
    Code,
    Tag,
    Frontmatter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleSpan {
    /// Char offset (GTK TextBuffer).
    pub start: usize,
    pub end: usize,
    pub kind: StyleKind,
}

fn byte_to_char(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].chars().count()
}

pub fn parse_syntax(text: &str) -> Vec<StyleSpan> {
    let mut spans = Vec::new();
    if text.is_empty() {
        return spans;
    }

    let mut byte_pos = 0usize;
    let mut line_index = 0usize;
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    let mut fence_count = 0u8;

    for line in text.split_inclusive('\n') {
        let line_bytes = line.len();
        let line_content = line.trim_end_matches(['\n', '\r']);
        let line_start = byte_pos;
        let line_end = line_start + line_content.len();

        if line_index == 0 && line_content.trim() == "---" {
            in_frontmatter = true;
            fence_count = 1;
            spans.push(StyleSpan {
                start: byte_to_char(text, line_start),
                end: byte_to_char(text, line_end),
                kind: StyleKind::Frontmatter,
            });
            byte_pos += line_bytes;
            line_index += 1;
            continue;
        }

        if in_frontmatter && !frontmatter_done {
            if line_content.trim() == "---" {
                fence_count += 1;
                spans.push(StyleSpan {
                    start: byte_to_char(text, line_start),
                    end: byte_to_char(text, line_end),
                    kind: StyleKind::Frontmatter,
                });
                if fence_count >= 2 {
                    in_frontmatter = false;
                    frontmatter_done = true;
                }
            } else {
                spans.push(StyleSpan {
                    start: byte_to_char(text, line_start),
                    end: byte_to_char(text, line_end),
                    kind: StyleKind::Frontmatter,
                });
            }
            byte_pos += line_bytes;
            line_index += 1;
            continue;
        }

        let trimmed = line_content.trim_start();
        let indent = line_content.len() - trimmed.len();
        let content_start = line_start + indent;

        if trimmed.starts_with("### ") {
            spans.push(StyleSpan {
                start: byte_to_char(text, content_start),
                end: byte_to_char(text, line_end),
                kind: StyleKind::Heading3,
            });
        } else if trimmed.starts_with("## ") && !trimmed.starts_with("### ") {
            spans.push(StyleSpan {
                start: byte_to_char(text, content_start),
                end: byte_to_char(text, line_end),
                kind: StyleKind::Heading2,
            });
        } else if trimmed.starts_with("# ") && !trimmed.starts_with("## ") {
            spans.push(StyleSpan {
                start: byte_to_char(text, content_start),
                end: byte_to_char(text, line_end),
                kind: StyleKind::Heading1,
            });
        } else if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
            spans.push(StyleSpan {
                start: byte_to_char(text, content_start),
                end: byte_to_char(text, line_end),
                kind: StyleKind::TaskDone,
            });
        } else if trimmed.starts_with("- [ ] ") {
            spans.push(StyleSpan {
                start: byte_to_char(text, content_start),
                end: byte_to_char(text, line_end),
                kind: StyleKind::TaskPending,
            });
        } else if trimmed.starts_with("- ") {
            spans.push(StyleSpan {
                start: byte_to_char(text, content_start),
                end: byte_to_char(text, line_end),
                kind: StyleKind::Bullet,
            });
        } else if trimmed.starts_with("> ") || trimmed == ">" {
            spans.push(StyleSpan {
                start: byte_to_char(text, content_start),
                end: byte_to_char(text, line_end),
                kind: StyleKind::Quote,
            });
        } else if trimmed.starts_with('!') && (trimmed.len() == 1 || trimmed.starts_with("! ")) {
            spans.push(StyleSpan {
                start: byte_to_char(text, content_start),
                end: byte_to_char(text, line_end),
                kind: StyleKind::Callout,
            });
        }

        parse_inline(text, line_content, line_start, &mut spans);

        byte_pos += line_bytes;
        line_index += 1;
    }

    spans
}

fn parse_inline(text: &str, line: &str, line_start: usize, spans: &mut Vec<StyleSpan>) {
    scan_delimited(
        text,
        line,
        line_start,
        "==",
        "==",
        StyleKind::Highlight,
        spans,
    );
    scan_delimited(text, line, line_start, "**", "**", StyleKind::Bold, spans);
    scan_delimited(text, line, line_start, "`", "`", StyleKind::Code, spans);
    scan_tags(text, line, line_start, spans);
}

fn scan_delimited(
    text: &str,
    line: &str,
    line_start: usize,
    open: &str,
    close: &str,
    kind: StyleKind,
    spans: &mut Vec<StyleSpan>,
) {
    let bytes = line.as_bytes();
    let open_b = open.as_bytes();
    let close_b = close.as_bytes();
    let mut i = 0usize;
    while i + open_b.len() < bytes.len() {
        if bytes[i..].starts_with(open_b) {
            let content_start = i + open_b.len();
            if let Some(rel) = find_subslice(&bytes[content_start..], close_b) {
                let end = content_start + rel + close_b.len();
                if rel > 0 {
                    let abs_start = line_start + i;
                    let abs_end = line_start + end;
                    spans.push(StyleSpan {
                        start: byte_to_char(text, abs_start),
                        end: byte_to_char(text, abs_end),
                        kind,
                    });
                    i = end;
                    continue;
                }
            }
        }
        i += 1;
    }
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    for i in 0..=hay.len() - needle.len() {
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

fn scan_tags(text: &str, line: &str, line_start: usize, spans: &mut Vec<StyleSpan>) {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let start = i;
            i += 1;
            let tag_start = i;
            while i < bytes.len() {
                let c = bytes[i] as char;
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '/' {
                    i += 1;
                } else {
                    break;
                }
            }
            if i > tag_start {
                let abs_start = line_start + start;
                let abs_end = line_start + i;
                spans.push(StyleSpan {
                    start: byte_to_char(text, abs_start),
                    end: byte_to_char(text, abs_end),
                    kind: StyleKind::Tag,
                });
                continue;
            }
        }
        i += 1;
    }
}

pub fn tag_name(kind: StyleKind) -> &'static str {
    match kind {
        StyleKind::Heading1 => "h1",
        StyleKind::Heading2 => "h2",
        StyleKind::Heading3 => "h3",
        StyleKind::Bullet => "bullet",
        StyleKind::TaskPending => "task_pending",
        StyleKind::TaskDone => "task_done",
        StyleKind::Quote => "quote",
        StyleKind::Callout => "callout",
        StyleKind::Highlight => "highlight",
        StyleKind::Bold => "bold",
        StyleKind::Code => "code",
        StyleKind::Tag => "tag",
        StyleKind::Frontmatter => "frontmatter",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds_at(spans: &[StyleSpan], kind: StyleKind) -> Vec<&StyleSpan> {
        spans.iter().filter(|s| s.kind == kind).collect()
    }

    #[test]
    fn parses_headings() {
        let text = "# Title\n## Section\n### Sub\n";
        let spans = parse_syntax(text);
        assert!(!kinds_at(&spans, StyleKind::Heading1).is_empty());
        assert!(!kinds_at(&spans, StyleKind::Heading2).is_empty());
        assert!(!kinds_at(&spans, StyleKind::Heading3).is_empty());
    }

    #[test]
    fn parses_tasks() {
        let text = "- [ ] pending\n- [x] done\n- normal\n";
        let spans = parse_syntax(text);
        assert_eq!(kinds_at(&spans, StyleKind::TaskPending).len(), 1);
        assert_eq!(kinds_at(&spans, StyleKind::TaskDone).len(), 1);
        assert_eq!(kinds_at(&spans, StyleKind::Bullet).len(), 1);
    }

    #[test]
    fn parses_inline_bold_highlight_code_tag() {
        let text = "see **bold** and ==hi== and `code` plus @tag\n";
        let spans = parse_syntax(text);
        assert_eq!(kinds_at(&spans, StyleKind::Bold).len(), 1);
        assert_eq!(kinds_at(&spans, StyleKind::Highlight).len(), 1);
        assert_eq!(kinds_at(&spans, StyleKind::Code).len(), 1);
        assert_eq!(kinds_at(&spans, StyleKind::Tag).len(), 1);

        let bold = kinds_at(&spans, StyleKind::Bold)[0];
        let slice: String = text
            .chars()
            .skip(bold.start)
            .take(bold.end - bold.start)
            .collect();
        assert_eq!(slice, "**bold**");

        let tag = kinds_at(&spans, StyleKind::Tag)[0];
        let t: String = text
            .chars()
            .skip(tag.start)
            .take(tag.end - tag.start)
            .collect();
        assert_eq!(t, "@tag");
    }

    #[test]
    fn parses_quote_and_callout() {
        let text = "> thought\n! alert\n";
        let spans = parse_syntax(text);
        assert_eq!(kinds_at(&spans, StyleKind::Quote).len(), 1);
        assert_eq!(kinds_at(&spans, StyleKind::Callout).len(), 1);
    }

    #[test]
    fn frontmatter_is_styled() {
        let text = "---\nkind: daily\n---\n\n# Hello\n";
        let spans = parse_syntax(text);
        assert!(!kinds_at(&spans, StyleKind::Frontmatter).is_empty());
        assert!(!kinds_at(&spans, StyleKind::Heading1).is_empty());
    }

    #[test]
    fn heading_char_offsets_match() {
        let text = "# Título\n";
        let spans = parse_syntax(text);
        let h = kinds_at(&spans, StyleKind::Heading1)[0];
        assert_eq!(h.start, 0);
        assert_eq!(h.end, text.trim_end().chars().count());
    }
}
