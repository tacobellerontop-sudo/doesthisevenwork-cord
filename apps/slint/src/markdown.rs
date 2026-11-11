//! Discord message text → the CommonMark subset Slint's `StyledText` renders.
//!
//! Slint parses CommonMark with only `<u>` and `<font color>` HTML and no code blocks, block
//! quotes or headings, and every line break starts a new paragraph. Discord text is close to
//! CommonMark, so this pass only rewrites what Slint can't show and escapes raw `<` and `&`
//! so message text can never inject markup. Mentions arrive already resolved by the caller.

/// One run of message text: plain prose, an inline code span, or a resolved mention.
pub enum Piece<'a> {
	/// Message text, which may hold markdown, code spans and line breaks.
	Text(&'a str),
	/// Generated text such as a date or `:emoji:`, shown verbatim.
	Plain(String),
	/// A resolved `@user`, `@role` or `#channel`, highlighted.
	Mention(String),
}

const MENTION_COLOR: &str = "#c9cdfb";

/// Escapes text for a markdown paragraph: HTML openers and markdown punctuation that Discord
/// would show literally at the start of a line.
fn escape_html(text: &str, out: &mut String) {
	for c in text.chars() {
		match c {
			'<' => out.push_str("&lt;"),
			'&' => out.push_str("&amp;"),
			_ => out.push(c),
		}
	}
}

/// Generated names (a nickname with `*` in it) must not open emphasis.
fn escape_markdown(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	for c in text.chars() {
		if matches!(c, '\\' | '*' | '_' | '~' | '`' | '[' | ']') {
			out.push('\\');
		}
		out.push(c);
	}
	out
}

/// Link text and targets must not open emphasis or close the link early.
fn escape_link(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	for c in text.chars() {
		if matches!(
			c,
			'\\' | '[' | ']' | '(' | ')' | '*' | '_' | '~' | '`' | '<' | '>'
		) {
			out.push('\\');
		}
		out.push(c);
	}
	out
}

/// Prose outside code: escaped, with bare http(s) URLs turned into links.
fn prose(text: &str, out: &mut String) {
	let mut rest = text;
	while let Some(start) = ["https://", "http://"]
		.iter()
		.filter_map(|scheme| rest.find(scheme))
		.min()
	{
		escape_html(&rest[..start], out);
		let tail = &rest[start..];
		let mut end = tail.find(char::is_whitespace).unwrap_or(tail.len());
		// Trailing punctuation belongs to the sentence, as Discord treats it.
		while end > 0 && tail[..end].ends_with(['.', ',', ')', '!', '?', ':', ';', '"', '\'']) {
			end -= 1;
		}
		let url = &tail[..end];
		if url.len() > "https://".len() {
			let escaped = escape_link(url);
			out.push('[');
			out.push_str(&escaped);
			out.push_str("](");
			out.push_str(&escaped);
			out.push(')');
		} else {
			escape_html(url, out);
		}
		rest = &tail[end..];
	}
	escape_html(rest, out);
}

/// One line of prose that may contain `inline code` spans, which stay verbatim.
fn line(pieces: &[Piece<'_>], out: &mut String) {
	for piece in pieces {
		match piece {
			Piece::Plain(text) => {
				let mut escaped = String::new();
				escape_html(text, &mut escaped);
				out.push_str(&escape_markdown(&escaped));
			}
			Piece::Mention(name) => {
				out.push_str("<font color=\"");
				out.push_str(MENTION_COLOR);
				out.push_str("\">");
				let mut escaped = String::new();
				escape_html(name, &mut escaped);
				out.push_str(&escape_markdown(&escaped));
				out.push_str("</font>");
			}
			Piece::Text(text) => {
				let mut rest = *text;
				while let Some(open) = rest.find('`') {
					let after = &rest[open + 1..];
					let Some(close) = after.find('`') else { break };
					prose(&rest[..open], out);
					code(&after[..close], out);
					rest = &after[close + 1..];
				}
				prose(rest, out);
			}
		}
	}
}

/// A code span whose content can contain backticks.
fn code(text: &str, out: &mut String) {
	if text.is_empty() {
		return;
	}
	let fence = if text.contains('`') { "`` " } else { "`" };
	let close = if text.contains('`') { " ``" } else { "`" };
	out.push_str(fence);
	out.push_str(text);
	out.push_str(close);
}

/// Converts message pieces (resolved mentions between text runs) into Slint markdown.
pub fn to_markdown(pieces: &[Piece<'_>]) -> String {
	// Split the pieces into lines, keeping each mention with its line.
	let mut lines: Vec<Vec<Piece<'_>>> = vec![Vec::new()];
	for piece in pieces {
		match piece {
			Piece::Mention(name) => lines
				.last_mut()
				.expect("one line")
				.push(Piece::Mention(name.clone())),
			Piece::Plain(text) => lines
				.last_mut()
				.expect("one line")
				.push(Piece::Plain(text.clone())),
			Piece::Text(text) => {
				let mut parts = text.split('\n');
				if let Some(first) = parts.next() {
					lines.last_mut().expect("one line").push(Piece::Text(first));
				}
				for part in parts {
					lines.push(vec![Piece::Text(part)]);
				}
			}
		}
	}

	let mut out = String::new();
	let mut in_fence = false;
	for pieces in &lines {
		let raw: String = pieces
			.iter()
			.map(|p| match p {
				Piece::Text(text) => *text,
				Piece::Plain(_) | Piece::Mention(_) => "_",
			})
			.collect();
		if raw.trim_start().starts_with("```") && pieces.len() == 1 {
			// The fence line itself (and a language tag) is not shown.
			let inner = raw.trim_start().trim_start_matches('`');
			let closes_inline = !in_fence && inner.contains("```");
			if closes_inline {
				// ```one-line code``` on a single line.
				let body = inner.split("```").next().unwrap_or_default();
				code(body, &mut out);
				out.push('\n');
			} else {
				in_fence = !in_fence;
			}
			continue;
		}
		if in_fence {
			if raw.trim().is_empty() {
				out.push_str("\u{00a0}\n");
			} else {
				// Leading spaces would be dropped by markdown; keep indentation visible.
				let indent = raw.len() - raw.trim_start_matches(' ').len();
				out.push_str(&"\u{00a0}".repeat(indent));
				code(raw.trim_start_matches(' '), &mut out);
				out.push('\n');
			}
			continue;
		}
		let trimmed = raw.trim_start();
		if raw.trim().is_empty() {
			// An empty line keeps Discord's visible gap between paragraphs.
			out.push_str("\u{00a0}\n");
			continue;
		}
		if let Some(rest) = heading(trimmed).filter(|_| pieces.len() == 1) {
			out.push_str("**");
			let mut body = String::new();
			line(&[Piece::Text(rest)], &mut body);
			out.push_str(body.trim());
			out.push_str("**\n");
			continue;
		}
		let quoted = trimmed.starts_with("> ") || trimmed == ">";
		if quoted {
			out.push_str("<font color=\"#949ba4\">▎</font> ");
		}
		let mut body = String::new();
		let mut stripped: Vec<Piece<'_>> = Vec::with_capacity(pieces.len());
		for (index, piece) in pieces.iter().enumerate() {
			match piece {
				Piece::Text(text) if index == 0 && quoted => {
					let text = text.trim_start();
					let text = text
						.strip_prefix("> ")
						.or(text.strip_prefix('>'))
						.unwrap_or(text);
					stripped.push(Piece::Text(text));
				}
				Piece::Text(text) => stripped.push(Piece::Text(text)),
				Piece::Plain(text) => stripped.push(Piece::Plain(text.clone())),
				Piece::Mention(name) => stripped.push(Piece::Mention(name.clone())),
			}
		}
		line(&stripped, &mut body);
		// A line that markdown would read as a block (list marker, thematic break, numbered
		// item) is written as Discord shows it; Slint draws real lists itself.
		if needs_block_escape(&body) {
			out.push('\\');
		}
		out.push_str(&body);
		out.push('\n');
	}
	while out.ends_with(['\n', '\u{00a0}']) {
		out.pop();
	}
	out
}

/// `# `, `## ` and `### ` headings, which Slint can't show; they become bold lines.
fn heading(line: &str) -> Option<&str> {
	["### ", "## ", "# "]
		.iter()
		.find_map(|prefix| line.strip_prefix(prefix))
}

/// Setext underlines, thematic breaks and indented code would change the block structure.
fn needs_block_escape(line: &str) -> bool {
	let trimmed = line.trim();
	(!trimmed.is_empty()
		&& trimmed
			.chars()
			.all(|c| matches!(c, '-' | '=' | '_' | '*' | ' ')))
		|| line.starts_with('#')
		|| line.starts_with("    ")
}

#[cfg(test)]
mod tests {
	use super::*;

	fn parses(markdown: &str) {
		slint::StyledText::from_markdown(markdown)
			.unwrap_or_else(|error| panic!("{markdown:?} did not parse: {error}"));
	}

	fn convert(text: &str) -> String {
		let markdown = to_markdown(&[Piece::Text(text)]);
		parses(&markdown);
		markdown
	}

	#[test]
	fn plain_lines_and_emphasis_pass_through() {
		assert_eq!(convert("hello **bold** _it_"), "hello **bold** _it_");
		assert_eq!(convert("one\ntwo"), "one\ntwo");
	}

	#[test]
	fn raw_html_is_escaped() {
		assert_eq!(convert("<b>x</b> & y"), "&lt;b>x&lt;/b> &amp; y");
		assert_eq!(
			convert("<font color=red>hi</font>"),
			"&lt;font color=red>hi&lt;/font>"
		);
	}

	#[test]
	fn code_blocks_become_code_lines() {
		let out = convert("```rust\nfn main() {\n    run();\n}\n```\nafter");
		assert_eq!(
			out,
			"`fn main() {`\n\u{a0}\u{a0}\u{a0}\u{a0}`run();`\n`}`\nafter"
		);
		assert_eq!(convert("```one line```"), "`one line`");
		assert_eq!(convert("use `a<b` here"), "use `a<b` here");
	}

	#[test]
	fn quotes_headings_and_links_render() {
		assert!(convert("> quoted").contains("▎</font> quoted"));
		assert_eq!(convert("# Title"), "**Title**");
		assert_eq!(
			convert("see https://example.com/a_b."),
			"see [https://example.com/a\\_b](https://example.com/a\\_b)."
		);
		convert("---");
		convert("    indented");
		convert("1. one\n2. two");
		convert("- item");
	}

	#[test]
	fn mentions_are_coloured_and_escaped() {
		let out = to_markdown(&[
			Piece::Text("hi "),
			Piece::Mention("@<Robin>".into()),
			Piece::Text("!"),
		]);
		parses(&out);
		assert_eq!(out, "hi <font color=\"#c9cdfb\">@&lt;Robin></font>!");
		let out = to_markdown(&[Piece::Mention("@a*b".into()), Piece::Plain(":wave:".into())]);
		parses(&out);
		assert_eq!(out, "<font color=\"#c9cdfb\">@a\\*b</font>:wave:");
	}
}
