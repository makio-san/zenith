//! Pure input helpers: bracketed paste, mouse-reporting encodings and URL detection.
//! Kept free of UI state so they can be unit-tested.

use alacritty_terminal::term::TermMode;
use egui::Modifiers;

/// Prepare pasted text for the shell.
///
/// With bracketed paste on, the text is fenced so programs can tell it from typing. Embedded fence markers are
/// stripped, so pasted content cannot break out of the fence. Without it, line breaks become carriage returns.
pub fn wrap_paste(text: &str, bracketed: bool) -> Vec<u8> {
    let clean = text.replace("\x1b[201~", "").replace("\x1b[200~", "");
    if bracketed {
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(clean.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        clean.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

/// Encode a mouse event for the running program.
///
/// `code` is the button (0 left, 1 middle, 2 right, 64/65 wheel up/down, +32 for motion); `col` and `row` are
/// zero-based cells. Returns `None` when the legacy encoding cannot express the cell (beyond column/row 222).
pub fn encode_mouse(mode: TermMode, code: u8, col: usize, row: usize, press: bool, m: Modifiers) -> Option<Vec<u8>> {
    let mods = u32::from(m.shift) * 4 + u32::from(m.alt) * 8 + u32::from(m.ctrl) * 16;
    if mode.contains(TermMode::SGR_MOUSE) {
        let tail = if press { 'M' } else { 'm' };
        Some(format!("\x1b[<{};{};{}{}", u32::from(code) + mods, col + 1, row + 1, tail).into_bytes())
    } else {
        if col + 33 > 255 || row + 33 > 255 {
            return None;
        }
        // Legacy X10 encoding: a release is reported as button 3.
        let base = if press { u32::from(code) } else { 3 };
        Some(vec![0x1b, b'[', b'M', (32 + base + mods) as u8, (33 + col) as u8, (33 + row) as u8])
    }
}

/// Find an `http(s)://` URL under `col` in one row of terminal text.
/// Returns the first and last column of the URL and the URL itself, without trailing punctuation.
pub fn find_url(text: &[char], col: usize) -> Option<(usize, usize, String)> {
    if col >= text.len() || text[col].is_whitespace() {
        return None;
    }
    let mut s = col;
    while s > 0 && !text[s - 1].is_whitespace() {
        s -= 1;
    }
    let mut e = col;
    while e + 1 < text.len() && !text[e + 1].is_whitespace() {
        e += 1;
    }
    let token: String = text[s..=e].iter().collect();
    let byte = token.find("https://").or_else(|| token.find("http://"))?;
    let offset = token[..byte].chars().count();
    let mut chars: Vec<char> = token[byte..].chars().collect();
    while matches!(chars.last(), Some('.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '>' | '\'' | '"')) {
        chars.pop();
    }
    if chars.len() < 9 {
        return None;
    }
    let (first, last) = (s + offset, s + offset + chars.len() - 1);
    if col < first || col > last {
        return None;
    }
    Some((first, last, chars.into_iter().collect()))
}

/// Open a URL in the default browser. Only http(s) URLs are accepted, and the URL is passed as a single
/// argument, never through a shell.
pub fn open_url(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(not(target_os = "macos"))]
    let opener = "xdg-open";
    if let Ok(mut child) = std::process::Command::new(opener).arg(url).spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn bracketed_paste_is_fenced_and_cannot_escape() {
        let out = wrap_paste("ls\x1b[201~; rm -rf ~", true);
        assert_eq!(out, b"\x1b[200~ls; rm -rf ~\x1b[201~".to_vec());
    }

    #[test]
    fn plain_paste_turns_newlines_into_carriage_returns() {
        assert_eq!(wrap_paste("a\r\nb\nc", false), b"a\rb\rc".to_vec());
    }

    #[test]
    fn sgr_mouse_press_and_release() {
        let mode = TermMode::SGR_MOUSE;
        assert_eq!(encode_mouse(mode, 0, 4, 9, true, Modifiers::NONE).unwrap(), b"\x1b[<0;5;10M".to_vec());
        assert_eq!(encode_mouse(mode, 0, 4, 9, false, Modifiers::NONE).unwrap(), b"\x1b[<0;5;10m".to_vec());
        let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
        assert_eq!(encode_mouse(mode, 64, 0, 0, true, ctrl).unwrap(), b"\x1b[<80;1;1M".to_vec());
    }

    #[test]
    fn legacy_mouse_encoding_and_limits() {
        let mode = TermMode::empty();
        assert_eq!(encode_mouse(mode, 0, 0, 0, true, Modifiers::NONE).unwrap(), vec![0x1b, b'[', b'M', 32, 33, 33]);
        // release is button 3
        assert_eq!(encode_mouse(mode, 2, 1, 1, false, Modifiers::NONE).unwrap(), vec![0x1b, b'[', b'M', 35, 34, 34]);
        assert!(encode_mouse(mode, 0, 300, 0, true, Modifiers::NONE).is_none());
    }

    #[test]
    fn url_under_cursor_is_found_and_trimmed() {
        let text = row("see https://example.com/a?b=1, then go");
        let (a, b, url) = find_url(&text, 10).unwrap();
        assert_eq!(url, "https://example.com/a?b=1");
        assert_eq!((a, b), (4, 28));
        assert!(find_url(&text, 1).is_none()); // on "see"
        assert!(find_url(&text, 29).is_none()); // on the trailing comma
        assert!(find_url(&text, 31).is_none()); // on "then"
    }

    #[test]
    fn urls_in_parentheses_and_plain_http() {
        let text = row("(http://localhost:3000/x)");
        assert_eq!(find_url(&text, 5).unwrap().2, "http://localhost:3000/x");
        assert!(find_url(&row("just some text"), 3).is_none());
        assert!(find_url(&row("ftp://example.com/file"), 8).is_none());
    }
}
