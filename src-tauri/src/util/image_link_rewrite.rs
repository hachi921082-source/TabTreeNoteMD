//! GitHub issue #261: rewrites a memo body's image links after `memo_rename` follow-renamed the
//! memo's per-memo image folder (`<tab>/<old>/` -> `<tab>/<new>/`, issue #139).
//!
//! Scope (decided in the issue): only `![alt](dest)` / `![alt](<dest>)` image links whose
//! destination starts with `<old>/` (raw, or percent-encoded as markdown-it/CommonMark may emit
//! for non-ASCII names, cf. #248). Plain text, normal `[text](url)` links, fenced code blocks and
//! inline code spans are left untouched.
//!
//! Output form: the link keeps the form it already used — a raw prefix becomes raw `<new>/`, a
//! percent-encoded prefix becomes percent-encoded `<new>/`. This avoids mixing raw and encoded
//! segments inside one destination (e.g. `%E6%97%A5/a%20b.png`), and leaves untouched links
//! byte-identical to before.
//!
//! Limitation: processing is per line, so an image whose `![alt](dest)` syntax is split across
//! lines, or an inline code span spanning lines, is not recognised (the app itself always emits
//! single-line `![](relativePath)` markup — `editor/imagePaste.ts`).

/// Returns the rewritten body, or `None` when no image link needed rewriting.
pub fn rewrite_image_links_for_rename(body: &str, old_name: &str, new_name: &str) -> Option<String> {
    if old_name.is_empty() || old_name == new_name {
        return None;
    }
    let old_prefix = format!("{old_name}/");
    let new_raw = format!("{new_name}/");
    let new_encoded = format!("{}/", percent_encode_segment(new_name));

    rewrite_image_destinations(body, |dest| {
        let (prefix_len, encoded) = match_prefix(dest, &old_prefix)?;
        let new_prefix = if encoded { &new_encoded } else { &new_raw };
        Some(format!("{new_prefix}{}", &dest[prefix_len..]))
    })
}

/// 本文中の画像リンク `![alt](dest)` / `![alt](<dest>)` のリンク先を `replace` で置換する
/// (GitHub issue #265 でインポート時の画像コピーと共用するため #261 の走査部分を汎用化)。
/// `replace` にはリンク先の生テキスト(山括弧形なら括弧の内側、エスケープ・パーセントエンコードは
/// 未処理のまま)を渡し、`Some(新リンク先)` ならその範囲だけを置き換える。フェンス・インライン
/// コード内は対象外。置換が1件も無ければ `None`。
pub fn rewrite_image_destinations<F: FnMut(&str) -> Option<String>>(body: &str, mut replace: F) -> Option<String> {
    let mut out = String::with_capacity(body.len() + 16);
    let mut changed = false;
    // フェンスの状態: (フェンス文字, 開始フェンスの長さ)
    let mut fence: Option<(char, usize)> = None;

    for line in body.split_inclusive('\n') {
        if let Some(marker) = fence_marker(line) {
            match fence {
                None => {
                    fence = Some(marker);
                    out.push_str(line);
                    continue;
                }
                Some((ch, len)) if marker.0 == ch && marker.1 >= len && is_closing_fence(line, ch) => {
                    fence = None;
                    out.push_str(line);
                    continue;
                }
                _ => {}
            }
        }
        if fence.is_some() {
            out.push_str(line);
            continue;
        }
        if rewrite_line(line, &mut replace, &mut out) {
            changed = true;
        }
    }

    changed.then_some(out)
}

/// 行頭(インデント3以下)の ``` / ~~~ フェンスなら (文字, 長さ) を返す。
fn fence_marker(line: &str) -> Option<(char, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let ch = rest.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = rest.chars().take_while(|&c| c == ch).count();
    if len < 3 {
        return None;
    }
    // バッククォートフェンスの info 文字列にはバッククォートを含められない(CommonMark)
    if ch == '`' && rest[len..].contains('`') {
        return None;
    }
    Some((ch, len))
}

/// 閉じフェンスはフェンス文字の後に空白以外を持たない。
fn is_closing_fence(line: &str, ch: char) -> bool {
    line.trim_start_matches(' ').trim_start_matches(ch).trim().is_empty()
}

/// 1行分を走査し、インラインコード外の画像リンク先を書き換えて `out` へ追記する。
fn rewrite_line<F: FnMut(&str) -> Option<String>>(line: &str, replace: &mut F, out: &mut String) -> bool {
    let bytes = line.as_bytes();
    let mut changed = false;
    let mut i = 0;
    let mut copied = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'`' => {
                // インラインコード: 同じ長さのバッククォート列までスキップ。閉じが無ければリテラル扱い。
                let run = count_run(bytes, i, b'`');
                i = find_closing_backticks(bytes, i + run, run).unwrap_or(i + run);
            }
            b'!' if bytes.get(i + 1) == Some(&b'[') => {
                let Some(dest) = parse_image_destination(bytes, i + 1) else {
                    i += 1;
                    continue;
                };
                if let Some(new_dest) = replace(&line[dest.start..dest.end]) {
                    out.push_str(&line[copied..dest.start]);
                    out.push_str(&new_dest);
                    copied = dest.end;
                    changed = true;
                }
                i = dest.end;
            }
            _ => i += 1,
        }
    }
    out.push_str(&line[copied..]);
    changed
}

fn count_run(bytes: &[u8], start: usize, b: u8) -> usize {
    bytes[start..].iter().take_while(|&&c| c == b).count()
}

/// 長さ `run` のバッククォート列の終端位置を返す。
fn find_closing_backticks(bytes: &[u8], mut i: usize, run: usize) -> Option<usize> {
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let n = count_run(bytes, i, b'`');
            if n == run {
                return Some(i + n);
            }
            i += n;
        } else {
            i += 1;
        }
    }
    None
}

/// `[` 位置から `[alt](dest ...)` を解析し、リンク先の範囲(山括弧の内側)を返す。
fn parse_image_destination(bytes: &[u8], open_bracket: usize) -> Option<std::ops::Range<usize>> {
    // alt テキストの対応する `]` を探す(ネスト・バックスラッシュエスケープ対応)
    let mut depth = 0usize;
    let mut i = open_bracket;
    let close = loop {
        match *bytes.get(i)? {
            b'\\' => i += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break i;
                }
            }
            _ => {}
        }
        i += 1;
    };
    if bytes.get(close + 1) != Some(&b'(') {
        return None;
    }
    let mut i = close + 2;
    while matches!(bytes.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    if bytes.get(i) == Some(&b'<') {
        let start = i + 1;
        let mut j = start;
        loop {
            match *bytes.get(j)? {
                b'>' => return Some(start..j),
                b'\n' | b'<' => return None,
                b'\\' => j += 2,
                _ => j += 1,
            }
        }
    }
    let start = i;
    let mut paren = 0usize;
    let mut j = start;
    while let Some(&c) = bytes.get(j) {
        match c {
            b'\\' => {
                j += 2;
                continue;
            }
            b'(' => paren += 1,
            b')' if paren == 0 => break,
            b')' => paren -= 1,
            c if c.is_ascii_whitespace() => break,
            _ => {}
        }
        j += 1;
    }
    let end = j.min(bytes.len());
    (end > start).then_some(start..end)
}

/// `dest` が `old_prefix` で始まるか判定する。生の形・パーセントエンコード形(大小文字不問、
/// 部分エンコード混在も可)の両方を受け付け、(リンク先上での接頭辞バイト長, エンコード形か) を返す。
fn match_prefix(dest: &str, old_prefix: &str) -> Option<(usize, bool)> {
    if dest.starts_with(old_prefix) {
        return Some((old_prefix.len(), false));
    }
    let want = old_prefix.as_bytes();
    let src = dest.as_bytes();
    let mut decoded = Vec::with_capacity(want.len());
    let mut i = 0;
    let mut saw_escape = false;
    while decoded.len() < want.len() {
        let b = *src.get(i)?;
        if b == b'%' {
            let hi = hex_val(*src.get(i + 1)?)?;
            let lo = hex_val(*src.get(i + 2)?)?;
            decoded.push(hi << 4 | lo);
            i += 3;
            saw_escape = true;
        } else {
            decoded.push(b);
            i += 1;
        }
        if decoded[decoded.len() - 1] != want[decoded.len() - 1] {
            return None;
        }
    }
    saw_escape.then_some((i, true))
}

/// リンク先文字列をファイルパスとして解釈できる形に戻す(GitHub issue #265)。CommonMark の
/// バックスラッシュエスケープを外してからパーセントデコードする。デコード結果が UTF-8 として
/// 不正なら `None`。
pub fn decode_destination(dest: &str) -> Option<String> {
    let mut unescaped = Vec::with_capacity(dest.len());
    let src = dest.as_bytes();
    let mut i = 0;
    while i < src.len() {
        let b = src[i];
        if b == b'\\' && src.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) {
            unescaped.push(src[i + 1]);
            i += 2;
        } else {
            unescaped.push(b);
            i += 1;
        }
    }
    let mut decoded = Vec::with_capacity(unescaped.len());
    let mut i = 0;
    while i < unescaped.len() {
        let b = unescaped[i];
        if b == b'%' {
            if let (Some(hi), Some(lo)) = (unescaped.get(i + 1).and_then(|&h| hex_val(h)), unescaped.get(i + 2).and_then(|&l| hex_val(l))) {
                decoded.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        decoded.push(b);
        i += 1;
    }
    String::from_utf8(decoded).ok()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// パス1セグメント分のパーセントエンコード(RFC 3986 pchar 以外を UTF-8 バイト単位で `%XX` 化)。
fn percent_encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for &b in s.as_bytes() {
        let keep = b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@".contains(&b);
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_raw_japanese_image_link() {
        let body = "前文\n![](日記/001.png)\n![図](<日記/002.png>)\n";
        let got = rewrite_image_links_for_rename(body, "日記", "日記2").unwrap();
        assert_eq!(got, "前文\n![](日記2/001.png)\n![図](<日記2/002.png>)\n");
    }

    #[test]
    fn rewrites_percent_encoded_link_keeping_encoded_form() {
        // 日記 = E6 97 A5 / E8 A8 98 (小文字16進も受け付ける)
        let body = "![](%E6%97%A5%e8%a8%98/001.png)\r\n";
        let got = rewrite_image_links_for_rename(body, "日記", "日記2").unwrap();
        assert_eq!(got, "![](%E6%97%A5%E8%A8%982/001.png)\r\n");
    }

    #[test]
    fn leaves_non_matching_and_normal_links_intact() {
        // 別フォルダ・前方一致だけの名前(日記帳)・通常リンクは対象外
        let body = "![](日記帳/001.png) ![](他/日記/001.png) [リンク](日記/001.png)\n";
        assert_eq!(rewrite_image_links_for_rename(body, "日記", "日記2"), None);
    }

    #[test]
    fn leaves_plain_text_and_code_intact() {
        let body = "日記/001.png はテキスト\n`![](日記/001.png)`\n```\n![](日記/001.png)\n```\n![](日記/003.png)\n";
        let got = rewrite_image_links_for_rename(body, "日記", "日記2").unwrap();
        assert_eq!(
            got,
            "日記/001.png はテキスト\n`![](日記/001.png)`\n```\n![](日記/001.png)\n```\n![](日記2/003.png)\n"
        );
    }

    #[test]
    fn decode_destination_unescapes_and_percent_decodes() {
        assert_eq!(decode_destination("%E7%94%BB%E5%83%8F/a%20b\\(1\\).png").as_deref(), Some("画像/a b(1).png"));
        // 不完全な %XX はそのまま残す
        assert_eq!(decode_destination("100%.png").as_deref(), Some("100%.png"));
        assert_eq!(decode_destination("%FF.png"), None);
    }

    #[test]
    fn handles_parentheses_in_name_and_multiple_links_per_line() {
        let body = "![a](メモ(1)/001.png \"題\") と ![b](メモ(1)/002.png)";
        let got = rewrite_image_links_for_rename(body, "メモ(1)", "新 メモ").unwrap();
        assert_eq!(got, "![a](新 メモ/001.png \"題\") と ![b](新 メモ/002.png)");
    }
}
