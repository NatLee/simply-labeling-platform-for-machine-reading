//! 文章和答案範圍的處理。
//!
//! 答案位置一律用「字元」計算（Unicode code point），和 SQuAD、Python 的字串索引一樣。
//! 瀏覽器的選取範圍是 UTF-16 位置，舊版資料庫存的也是 UTF-16 位置，
//! 所以這裡集中處理兩種單位的換算，以及「位置和文字對不上」時怎麼找回正確位置。

use sha2::{Digest, Sha256};

/// 位置的單位。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffsetUnit {
    /// Unicode code point，SQuAD 和這個平台用的單位。
    Chars,
    /// UTF-16 code unit，瀏覽器 `selectionStart` 和舊版資料庫用的單位。
    Utf16,
}

/// 把換行統一成 `\n`。
///
/// 瀏覽器的 textarea 會把 `\r\n` 變成 `\n`，不先統一的話，畫面上選到的位置和存下來的文章會差幾個字元。
pub fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// 文章內容的雜湊，用來判斷同一個專案裡是不是已經有這篇文章。
pub fn context_hash(context: &str) -> String {
    sha256_hex(context)
}

/// SHA-256 的十六進位字串（也用在 session token）。
pub fn sha256_hex(s: &str) -> String {
    let digest = Sha256::digest(s.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn char_len(text: &str) -> usize {
    text.chars().count()
}

/// 第 `char_idx` 個字元在 UTF-8 字串裡的 byte 位置；超出範圍時回 `None`。
fn byte_index(text: &str, char_idx: usize) -> Option<usize> {
    if char_idx == 0 {
        return Some(0);
    }
    match text.char_indices().nth(char_idx) {
        Some((b, _)) => Some(b),
        None if char_len(text) == char_idx => Some(text.len()),
        None => None,
    }
}

/// UTF-16 位置換成字元位置。落在代理對（surrogate pair）中間時回 `None`。
pub fn utf16_to_char(text: &str, utf16_idx: usize) -> Option<usize> {
    let mut units = 0;
    for (i, c) in text.chars().enumerate() {
        if units == utf16_idx {
            return Some(i);
        }
        units += c.len_utf16();
        if units > utf16_idx {
            return None;
        }
    }
    (units == utf16_idx).then(|| char_len(text))
}

/// 文章從字元位置 `start` 開始的文字是不是正好等於 `answer`。
pub fn span_matches(context: &str, start: usize, answer: &str) -> bool {
    match byte_index(context, start) {
        Some(b) => context[b..].starts_with(answer),
        None => false,
    }
}

/// 找出 `needle` 在 `haystack` 裡所有出現的字元位置。
pub fn find_all(haystack: &str, needle: &str) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(pos) = haystack[from..].find(needle) {
        let b = from + pos;
        out.push(char_len(&haystack[..b]));
        // 往後移一個字元，允許重疊的出現位置。
        from = b + haystack[b..].chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// 答案範圍檢查的結果。
#[derive(Debug, PartialEq, Eq)]
pub enum Resolved {
    /// 位置和文字對得上。
    Exact(usize),
    /// 原本的位置對不上，但文章裡找得到這段文字；取離原位置最近的那一個。
    Moved { from: usize, to: usize },
    /// 文章裡沒有這段文字。
    NotFound,
}

/// 確認「文章從 `claimed` 開始是 `answer`」；對不上時找最近的出現位置。
///
/// 匯入外部資料時用：位置可能是另一種單位，文章的換行也可能被統一過。
pub fn resolve_span(context: &str, claimed: usize, unit: OffsetUnit, answer: &str) -> Resolved {
    let claimed_chars = match unit {
        OffsetUnit::Chars => Some(claimed),
        OffsetUnit::Utf16 => utf16_to_char(context, claimed),
    };
    if let Some(start) = claimed_chars {
        if span_matches(context, start, answer) {
            return Resolved::Exact(start);
        }
    }
    let target = claimed_chars.unwrap_or(claimed);
    match find_all(context, answer)
        .into_iter()
        .min_by_key(|&pos| pos.abs_diff(target))
    {
        Some(to) => Resolved::Moved { from: claimed, to },
        None => Resolved::NotFound,
    }
}

/// 找出模型給的答案在文章裡的位置。
///
/// 模型算不準位置，所以請它照抄答案和「包含答案的那一句」（evidence）。
/// 先找 evidence，再在 evidence 裡找答案；evidence 找不到時取答案第一次出現的位置。
/// 答案不是原文一字不差的片段時回 `None`。
pub fn locate_answer(context: &str, answer: &str, evidence: Option<&str>) -> Option<usize> {
    if answer.is_empty() {
        return None;
    }
    if let Some(ev) = evidence.map(str::trim).filter(|e| !e.is_empty()) {
        for ev_start in find_all(context, ev) {
            if let Some(inner) = find_all(ev, answer).first() {
                return Some(ev_start + inner);
            }
        }
    }
    find_all(context, answer).first().copied()
}

/// 去掉選取範圍前後的空白，回傳新的 (start, end)，都是字元位置、end 不含。
///
/// 前端也有一份同樣的邏輯；這裡用在檢查 API 送來的範圍。
pub fn trim_span(context: &str, start: usize, end: usize) -> (usize, usize) {
    let chars: Vec<char> = context.chars().collect();
    let end = end.min(chars.len());
    let mut s = start.min(end);
    let mut e = end;
    while s < e && chars[s].is_whitespace() {
        s += 1;
    }
    while e > s && chars[e - 1].is_whitespace() {
        e -= 1;
    }
    (s, e)
}

/// 取文章的第 `start` 到 `end` 個字元。
pub fn slice_chars(context: &str, start: usize, end: usize) -> String {
    context
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 「😀」在 UTF-16 佔 2 個單位、在字元位置只佔 1 個，用來分辨兩種單位。
    const MIXED: &str = "台北😀是首都，Taipei is the capital.";

    #[test]
    fn span_matches_counts_characters_not_bytes() {
        assert!(span_matches(MIXED, 0, "台北"));
        assert!(span_matches(MIXED, 3, "是首都"));
        assert!(!span_matches(MIXED, 4, "是首都"));
        assert!(span_matches(MIXED, 7, "Taipei"));
        assert!(!span_matches(MIXED, 999, "x"));
        assert!(span_matches("abc", 3, ""));
    }

    #[test]
    fn utf16_offsets_convert_to_char_offsets() {
        // 「是」在 UTF-16 是第 4 個單位（台、北、😀×2），字元位置是 3。
        assert_eq!(utf16_to_char(MIXED, 4), Some(3));
        assert_eq!(utf16_to_char(MIXED, 0), Some(0));
        // 落在 emoji 的兩個單位中間
        assert_eq!(utf16_to_char(MIXED, 3), None);
        assert_eq!(utf16_to_char("ab", 2), Some(2));
        assert_eq!(utf16_to_char("ab", 3), None);
    }

    #[test]
    fn resolve_span_accepts_exact_and_utf16_offsets() {
        assert_eq!(
            resolve_span(MIXED, 3, OffsetUnit::Chars, "是首都"),
            Resolved::Exact(3)
        );
        assert_eq!(
            resolve_span(MIXED, 4, OffsetUnit::Utf16, "是首都"),
            Resolved::Exact(3)
        );
    }

    #[test]
    fn resolve_span_moves_to_the_nearest_occurrence() {
        let ctx = "the cat and the dog and the bird";
        // 正確位置是 24，給 22 時取最近的 24，不是第一個 0
        assert_eq!(
            resolve_span(ctx, 22, OffsetUnit::Chars, "the"),
            Resolved::Moved { from: 22, to: 24 }
        );
        assert_eq!(
            resolve_span(ctx, 0, OffsetUnit::Chars, "fish"),
            Resolved::NotFound
        );
    }

    #[test]
    fn find_all_returns_char_positions_including_overlaps() {
        assert_eq!(find_all("aaa", "aa"), vec![0, 1]);
        assert_eq!(find_all("😀a😀a", "a"), vec![1, 3]);
        assert!(find_all("abc", "").is_empty());
    }

    #[test]
    fn locate_answer_uses_evidence_to_pick_the_right_occurrence() {
        let ctx = "2019 年開幕。新館在 2021 年完工，2019 年的舊館關閉。";
        // 答案「2019 年」出現兩次，evidence 指的是第二句
        let pos = locate_answer(ctx, "2019 年", Some("2019 年的舊館關閉。")).unwrap();
        assert_eq!(pos, 22);
        assert!(span_matches(ctx, pos, "2019 年"));
        // 沒有 evidence 或 evidence 不是原文時，取第一次出現
        assert_eq!(locate_answer(ctx, "2019 年", None), Some(0));
        assert_eq!(locate_answer(ctx, "2019 年", Some("改寫過的句子")), Some(0));
        // 答案不是原文片段
        assert_eq!(locate_answer(ctx, "二〇一九年", None), None);
        assert_eq!(locate_answer(ctx, "", None), None);
    }

    #[test]
    fn trim_span_drops_surrounding_whitespace() {
        let ctx = "say  hello  world";
        assert_eq!(trim_span(ctx, 3, 12), (5, 10));
        assert_eq!(slice_chars(ctx, 5, 10), "hello");
        assert_eq!(trim_span(ctx, 3, 5), (5, 5));
        assert_eq!(trim_span(ctx, 0, 999), (0, 17));
    }

    #[test]
    fn newlines_are_normalized() {
        assert_eq!(normalize_newlines("a\r\nb\rc\n"), "a\nb\nc\n");
    }

    #[test]
    fn context_hash_is_stable_sha256() {
        // echo -n abc | sha256sum
        assert_eq!(
            context_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
