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
//! Angle brackets (GitHub issue #266): a destination that already used `<dest>` keeps it; a bare
//! destination whose rewritten form contains a space / ASCII control character or unbalanced
//! parentheses (e.g. a memo renamed to `新 メモ`) is wrapped as `<dest>`, since CommonMark rejects
//! those in a bare destination. Otherwise the bare form is kept.
//!
//! Limitation: processing is per line, so an image whose `![alt](dest)` syntax is split across
//! lines, or an inline code span spanning lines, is not recognised (the app itself always emits
//! single-line `![](relativePath)` markup — `editor/imagePaste.ts`).
//!
//! Legacy repair (GitHub issue #266): links written before the angle-bracket fix as a bare
//! destination containing a half-width space (`![](新 メモ/001.png)`, not a link per CommonMark)
//! are also recognised, but only when the caller confirms the referenced file exists; they are
//! then rewritten in bracketed form. Indented code blocks are not excluded (same as before).

use std::path::{Component, Path};

use crate::fs::path_safety::{is_link_or_reparse_point, resolve_relative_within_base, AbsolutePathPolicy};

/// Returns the rewritten body, or `None` when no image link needed rewriting.
///
/// `legacy_exists`: GitHub issue #266 — called for a legacy broken link (`![](<old> 名/001.png)`
/// style bare destination with a half-width space, see `rewrite_image_destinations_with_legacy`)
/// whose path starts with `<old>/`, with the decoded remainder after that prefix (e.g. `001.png`).
/// Only when it returns `true` (the image file really exists in the memo's image folder at this
/// moment) is the link rewritten (to `<new>/…`, bracketed when needed).
pub fn rewrite_image_links_for_rename<L: FnMut(&str) -> bool>(body: &str, old_name: &str, new_name: &str, mut legacy_exists: L) -> Option<String> {
    if old_name.is_empty() || old_name == new_name {
        return None;
    }
    let old_prefix = format!("{old_name}/");
    let new_raw = format!("{new_name}/");
    let new_encoded = format!("{}/", percent_encode_segment(new_name));

    rewrite_image_destinations_with_legacy(
        body,
        |dest| {
            let (prefix_len, encoded) = match_prefix(dest, &old_prefix)?;
            let new_prefix = if encoded { &new_encoded } else { &new_raw };
            Some(format!("{new_prefix}{}", &dest[prefix_len..]))
        },
        |path| {
            // 旧フォルダ配下を指すレガシーリンクのみ、実在確認できたら対象にする
            let Some((prefix_len, _)) = match_prefix(path, &old_prefix) else { return false };
            decode_destination(&path[prefix_len..]).is_some_and(|rest| !rest.is_empty() && legacy_exists(&rest))
        },
    )
}

/// `base`(タブフォルダ等)からの相対パス `rel` が、リンク/リパースポイントを一切経由せずに到達できる
/// 通常ファイルか(GitHub issue #266 のレガシーリンク修復の実在確認)。絶対パス・`..`・ルート付きは
/// `resolve_relative_within_base`(#263 と同じガード)で拒否する。`base` 自体は検査しない。
pub fn regular_file_without_links(base: &Path, rel: &str) -> bool {
    if resolve_relative_within_base(rel, Some(base), AbsolutePathPolicy::Reject).is_err() {
        return false;
    }
    let names: Vec<_> = Path::new(rel).components().filter(|c| !matches!(c, Component::CurDir)).collect();
    let mut cur = base.to_path_buf();
    for (idx, comp) in names.iter().enumerate() {
        let Component::Normal(name) = comp else { return false };
        cur.push(name);
        let Ok(meta) = std::fs::symlink_metadata(&cur) else { return false };
        if is_link_or_reparse_point(&meta) {
            return false;
        }
        if idx + 1 == names.len() {
            return meta.is_file();
        }
        if !meta.is_dir() {
            return false;
        }
    }
    false
}

/// 本文中の画像リンク `![alt](dest)` / `![alt](<dest>)` のリンク先を `replace` で置換する
/// (GitHub issue #265 でインポート時の画像コピーと共用するため #261 の走査部分を汎用化)。
/// `replace` にはリンク先の生テキスト(山括弧形なら括弧の内側、エスケープ・パーセントエンコードは
/// 未処理のまま)を渡し、`Some(新リンク先)` ならその範囲だけを置き換える。新リンク先は山括弧なしで
/// 返す: 元が山括弧形ならそのまま括弧内に、元が裸形で空白等を含むなら `<...>` で囲んで書く
/// (GitHub issue #266)。フェンス・インラインコード内は対象外。置換が1件も無ければ `None`。
/// レガシーリンク(#266 以前の空白入り裸形)は扱わない — `rewrite_image_destinations_with_legacy` 参照。
pub fn rewrite_image_destinations<F: FnMut(&str) -> Option<String>>(body: &str, replace: F) -> Option<String> {
    rewrite_image_destinations_with_legacy(body, replace, |_| false)
}

/// `rewrite_image_destinations` に加え、#266 以前に書かれた「半角スペース入りの裸形リンク先」
/// (`![](新 メモ/001.png)` — CommonMark ではリンクにならない)も認識する(GitHub issue #266)。
/// 同じ行の `![alt](` から `)` までを、リンク先パス(+ 任意の `"…"` / `'…'` / `(…)` タイトル)と
/// みなし、半角スペースを含むパス候補を `legacy_exists` に渡す(生テキスト、未デコード)。
/// 最初に `true` となった候補を採用し、`replace` の結果で置き換える(必要なら山括弧形)。
/// どの候補も実在しなければ触らない。
pub fn rewrite_image_destinations_with_legacy<F, L>(body: &str, mut replace: F, mut legacy_exists: L) -> Option<String>
where
    F: FnMut(&str) -> Option<String>,
    L: FnMut(&str) -> bool,
{
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
        if rewrite_line(line, &mut replace, &mut legacy_exists, &mut out) {
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
fn rewrite_line<F, L>(line: &str, replace: &mut F, legacy_exists: &mut L, out: &mut String) -> bool
where
    F: FnMut(&str) -> Option<String>,
    L: FnMut(&str) -> bool,
{
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
                let Some(parsed) = parse_image_destination(bytes, i + 1) else {
                    i += 1;
                    continue;
                };
                if !parsed.valid {
                    // #266: 空白入り裸形のレガシーリンク。実在するパス候補が見つかれば修復する
                    let found = legacy_candidates(line, parsed.dest.start).into_iter().find(|(range, _)| legacy_exists(&line[range.clone()]));
                    if let Some((range, close)) = found {
                        if let Some(new_dest) = replace(&line[range.clone()]) {
                            out.push_str(&line[copied..range.start]);
                            if needs_angle_brackets(&new_dest) {
                                out.push('<');
                                out.push_str(&new_dest);
                                out.push('>');
                            } else {
                                out.push_str(&new_dest);
                            }
                            copied = range.end;
                            changed = true;
                        }
                        i = close + 1;
                        continue;
                    }
                    if parsed.dest.is_empty() {
                        i += 1;
                        continue;
                    }
                }
                let ParsedImage { dest: range, bracketed, .. } = parsed;
                if let Some(new_dest) = replace(&line[range.start..range.end]) {
                    out.push_str(&line[copied..range.start]);
                    if !bracketed && needs_angle_brackets(&new_dest) {
                        // 裸形のままでは CommonMark がリンク先と認めないため山括弧形にする(#266)
                        out.push('<');
                        out.push_str(&new_dest);
                        out.push('>');
                    } else {
                        out.push_str(&new_dest);
                    }
                    copied = range.end;
                    changed = true;
                }
                i = range.end;
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

/// 裸形のリンク先として書けない文字列か(GitHub issue #266)。CommonMark の裸形リンク先は
/// 半角スペース・ASCII 制御文字を含めず、エスケープされていない丸括弧が対応している必要がある。
/// 全角スペース等の非 ASCII 空白は裸形のままで有効。
pub fn needs_angle_brackets(dest: &str) -> bool {
    let bytes = dest.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if bytes.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) => i += 1,
            b' ' | 0x00..=0x1f | 0x7f => return true,
            b'(' => depth += 1,
            b')' => match depth.checked_sub(1) {
                Some(d) => depth = d,
                None => return true,
            },
            _ => {}
        }
        i += 1;
    }
    depth != 0
}

/// `[` 位置から `[alt](dest ...)` を解析する。
fn parse_image_destination(bytes: &[u8], open_bracket: usize) -> Option<ParsedImage> {
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
                b'>' => {
                    return Some(ParsedImage {
                        dest: start..j,
                        bracketed: true,
                        valid: true,
                    })
                }
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
    Some(ParsedImage {
        dest: start..end,
        bracketed: false,
        valid: end > start && valid_link_tail(bytes, end),
    })
}

/// 裸形リンク先の直後が CommonMark のリンク終端(空白 + 任意のタイトル + `)`)になっているか。
/// なっていなければ CommonMark 上は画像リンクではない(#266 のレガシーリンク候補)。
fn valid_link_tail(bytes: &[u8], mut i: usize) -> bool {
    let skip_ws = |mut i: usize| {
        while matches!(bytes.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
        i
    };
    i = skip_ws(i);
    let close = match bytes.get(i) {
        Some(b')') => return true,
        Some(b'"') => b'"',
        Some(b'\'') => b'\'',
        Some(b'(') => b')',
        _ => return false,
    };
    let open = bytes[i];
    i += 1;
    loop {
        match bytes.get(i) {
            None | Some(b'\n') => return false,
            Some(b'\\') => i += 2,
            // `(…)` タイトル内の未エスケープ `(` は不可
            Some(&c) if open == b'(' && c == b'(' => return false,
            Some(&c) if c == close => break,
            _ => i += 1,
        }
    }
    bytes.get(skip_ws(i + 1)) == Some(&b')')
}

/// レガシーリンク(#266)のパス候補を返す: `start` 以降の各 `)` までの内側について、全体をパスと
/// みなす候補と、末尾の `"…"` / `'…'` / `(…)` をタイトルとして除いた候補(右から順)。
/// (パス範囲, その `)` の位置) の列。半角スペースを含まないパス・`<` で始まるパスは除外。
fn legacy_candidates(line: &str, start: usize) -> Vec<(std::ops::Range<usize>, usize)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    for k in start..bytes.len() {
        if bytes[k] != b')' || (k > start && bytes[k - 1] == b'\\') {
            continue;
        }
        // 後続のリンク・インラインコードにまたがる候補は除外(以降の `)` も同様なので打ち切る)
        if ["`", "](", "!["].iter().any(|s| line[start..k].contains(s)) {
            break;
        }
        let inner = line[start..k].trim_end_matches([' ', '\t']);
        let mut push = |len: usize| {
            let path = line[start..start + len].trim_end_matches([' ', '\t']);
            if !path.is_empty() && path.contains(' ') && !path.starts_with('<') && !path.contains('\n') {
                out.push((start..start + path.len(), k));
            }
        };
        push(inner.len());
        let ib = inner.as_bytes();
        let opener = match ib.last() {
            Some(b'"') => Some(b'"'),
            Some(b'\'') => Some(b'\''),
            Some(b')') => Some(b'('),
            _ => None,
        };
        if let Some(opener) = opener {
            for q in (1..ib.len().saturating_sub(1)).rev() {
                if ib[q] == opener && matches!(ib[q - 1], b' ' | b'\t') {
                    push(q);
                }
            }
        }
    }
    out
}

/// 画像リンクの解析結果。`dest` はリンク先の範囲(山括弧形なら括弧の内側)、`valid` は CommonMark
/// 上も画像リンクとして成立しているか(裸形の場合のみ偽になりうる)。
struct ParsedImage {
    dest: std::ops::Range<usize>,
    bracketed: bool,
    valid: bool,
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

    /// レガシーリンクの実在確認なし(常に偽)の改名書き換え
    fn rename_no_legacy(body: &str, old: &str, new: &str) -> Option<String> {
        rewrite_image_links_for_rename(body, old, new, |_| false)
    }

    #[test]
    fn rewrites_raw_japanese_image_link() {
        let body = "前文\n![](日記/001.png)\n![図](<日記/002.png>)\n";
        let got = rename_no_legacy(body, "日記", "日記2").unwrap();
        assert_eq!(got, "前文\n![](日記2/001.png)\n![図](<日記2/002.png>)\n");
    }

    #[test]
    fn rewrites_percent_encoded_link_keeping_encoded_form() {
        // 日記 = E6 97 A5 / E8 A8 98 (小文字16進も受け付ける)
        let body = "![](%E6%97%A5%e8%a8%98/001.png)\r\n";
        let got = rename_no_legacy(body, "日記", "日記2").unwrap();
        assert_eq!(got, "![](%E6%97%A5%E8%A8%982/001.png)\r\n");
    }

    #[test]
    fn leaves_non_matching_and_normal_links_intact() {
        // 別フォルダ・前方一致だけの名前(日記帳)・通常リンクは対象外
        let body = "![](日記帳/001.png) ![](他/日記/001.png) [リンク](日記/001.png)\n";
        assert_eq!(rename_no_legacy(body, "日記", "日記2"), None);
    }

    #[test]
    fn leaves_plain_text_and_code_intact() {
        let body = "日記/001.png はテキスト\n`![](日記/001.png)`\n```\n![](日記/001.png)\n```\n![](日記/003.png)\n";
        let got = rename_no_legacy(body, "日記", "日記2").unwrap();
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
        let got = rename_no_legacy(body, "メモ(1)", "新 メモ").unwrap();
        // 半角スペースを含む新名は山括弧形で書く(GitHub issue #266)
        assert_eq!(got, "![a](<新 メモ/001.png> \"題\") と ![b](<新 メモ/002.png>)");
    }

    #[test]
    fn spaced_name_uses_angle_brackets_and_normal_names_stay_bare() {
        let body = "![](日記/001.png)\n";
        assert_eq!(rename_no_legacy(body, "日記", "新 メモ").unwrap(), "![](<新 メモ/001.png>)\n");
        assert_eq!(rename_no_legacy(body, "日記", "日記2").unwrap(), "![](日記2/001.png)\n");
        // 全角スペースは裸形のままで有効
        assert_eq!(rename_no_legacy(body, "日記", "新\u{3000}メモ").unwrap(), "![](新\u{3000}メモ/001.png)\n");
        // 対応の取れない丸括弧も山括弧形
        assert_eq!(rename_no_legacy(body, "日記", "メモ(").unwrap(), "![](<メモ(/001.png>)\n");
    }

    #[test]
    fn matches_bracketed_spaced_links_and_keeps_bracket_form() {
        // 既に山括弧形で書かれた空白入りリンクも改名・ロック変換の対象になる
        let body = "![](<新 メモ/001.png>) ![x](<新 メモ/002.png> \"題\") ![](新 メモ/003.png)\n";
        let got = rename_no_legacy(body, "新 メモ", "a1b2c3").unwrap();
        // 裸形の空白入りリンク(既存の壊れたリンク)は CommonMark 上リンクでないため対象外
        assert_eq!(got, "![](<a1b2c3/001.png>) ![x](<a1b2c3/002.png> \"題\") ![](新 メモ/003.png)\n");
        // 逆方向(ランダム名→空白入りメモ名)は山括弧形で戻る
        let back = rename_no_legacy("![](a1b2c3/001.png)\n", "a1b2c3", "新 メモ").unwrap();
        assert_eq!(back, "![](<新 メモ/001.png>)\n");
        assert_eq!(rename_no_legacy(&back, "新 メモ", "a1b2c3").unwrap(), "![](<a1b2c3/001.png>)\n");
    }

    #[test]
    fn needs_angle_brackets_rules() {
        assert!(!needs_angle_brackets("メモ/001.png"));
        assert!(!needs_angle_brackets("メモ(1)/001.png"));
        assert!(!needs_angle_brackets("メモ\\(/001.png"));
        assert!(!needs_angle_brackets("新\u{3000}メモ/001.png"));
        assert!(needs_angle_brackets("新 メモ/001.png"));
        assert!(needs_angle_brackets("a\tb/001.png"));
        assert!(needs_angle_brackets("メモ)/001.png"));
    }

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-link-rewrite-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn legacy_spaced_link_is_rewritten_with_brackets_when_file_exists() {
        // 改名後の画像フォルダ(新名側)に実在するファイルで確認する
        let tab = tempdir();
        std::fs::create_dir_all(tab.join("別 メモ")).unwrap();
        std::fs::write(tab.join("別 メモ").join("001.png"), b"png").unwrap();
        let body = "前文 ![](新 メモ/001.png) 後文\n";
        let got = rewrite_image_links_for_rename(body, "新 メモ", "別 メモ", |rest| regular_file_without_links(&tab, &format!("別 メモ/{rest}"))).unwrap();
        assert_eq!(got, "前文 ![](<別 メモ/001.png>) 後文\n");
        // 新名に空白が無ければ裸形
        std::fs::create_dir_all(tab.join("日記")).unwrap();
        std::fs::write(tab.join("日記").join("001.png"), b"png").unwrap();
        let got = rewrite_image_links_for_rename(body, "新 メモ", "日記", |rest| regular_file_without_links(&tab, &format!("日記/{rest}"))).unwrap();
        assert_eq!(got, "前文 ![](日記/001.png) 後文\n");
        std::fs::remove_dir_all(&tab).ok();
    }

    #[test]
    fn legacy_link_is_left_untouched_when_file_missing() {
        let tab = tempdir();
        let body = "![](新 メモ/001.png)\n";
        assert_eq!(rewrite_image_links_for_rename(body, "新 メモ", "別 メモ", |rest| regular_file_without_links(&tab, &format!("別 メモ/{rest}"))), None);
        // `..` を含むパスは実在確認の時点で拒否
        assert!(!regular_file_without_links(&tab, "../x.png"));
        std::fs::remove_dir_all(&tab).ok();
    }

    #[test]
    fn legacy_link_with_title_and_parens_picks_resolving_split() {
        let exists = |rest: &str| rest == "001 (1).png";
        let body = "![a](新 メモ/001 (1).png \"題 名\") ![b](新 メモ/001 (1).png (題))\n";
        let got = rewrite_image_links_for_rename(body, "新 メモ", "a1b2c3", exists).unwrap();
        assert_eq!(got, "![a](<a1b2c3/001 (1).png> \"題 名\") ![b](<a1b2c3/001 (1).png> (題))\n");
        // ファイル名が `001 (1).png "x"` のような候補でも、実在する分割だけを採用する
        let only_title_split = |rest: &str| rest == "001.png";
        let got = rewrite_image_links_for_rename("![](新 メモ/001.png 'x y')\n", "新 メモ", "日記", only_title_split).unwrap();
        assert_eq!(got, "![](日記/001.png 'x y')\n");
    }

    #[test]
    fn legacy_candidate_does_not_span_following_link() {
        // 1つ目だけがレガシー。候補が2つ目のリンクや後続のコードスパンにまたがらないこと
        let mut seen = Vec::new();
        let body = "![](新 メモ/001.png) ![](新 メモ/002.png) `x)`\n";
        let got = rewrite_image_links_for_rename(body, "新 メモ", "日記", |rest| {
            seen.push(rest.to_string());
            rest == "001.png"
        })
        .unwrap();
        assert_eq!(got, "![](日記/001.png) ![](新 メモ/002.png) `x)`\n");
        assert!(seen.iter().all(|r| !r.contains("](") && !r.contains('`')), "{seen:?}");
    }

    #[test]
    fn legacy_link_in_code_is_untouched() {
        let body = "`![](新 メモ/001.png)`\n```\n![](新 メモ/001.png)\n```\n";
        assert_eq!(rewrite_image_links_for_rename(body, "新 メモ", "日記", |_| true), None);
    }
}
