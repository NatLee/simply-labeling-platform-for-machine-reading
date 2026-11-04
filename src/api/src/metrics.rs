//! 答案的一致程度：SQuAD 官方評估用的完全一致（EM）和 token F1。
//!
//! 中文、日文、韓文沒有空白分詞，每個字當一個 token（和 CMRC 2018 評估一樣的混合切法），
//! 其他文字用空白切。比對前先轉小寫、去掉標點和英文冠詞（a、an、the）。

use std::collections::HashMap;

use serde::Serialize;

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF     // 平假名、片假名
        | 0x3400..=0x4DBF   // CJK 擴充 A
        | 0x4E00..=0x9FFF   // CJK 基本區
        | 0xAC00..=0xD7AF   // 韓文音節
        | 0xF900..=0xFAFF   // CJK 相容字
        | 0x20000..=0x2FA1F // CJK 擴充 B 之後
    )
}

/// 切成 token：中日韓文字一字一個，其他照空白切；標點和英文冠詞不算。
pub fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Vec<String>| {
        if !word.is_empty() && !matches!(word.as_str(), "a" | "an" | "the") {
            out.push(std::mem::take(word));
        }
        word.clear();
    };
    for c in text.chars().flat_map(char::to_lowercase) {
        if is_cjk(c) {
            flush(&mut word, &mut out);
            out.push(c.to_string());
        } else if c.is_alphanumeric() {
            word.push(c);
        } else {
            // 空白、標點和符號都當成分隔
            flush(&mut word, &mut out);
        }
    }
    flush(&mut word, &mut out);
    out
}

pub fn exact_match(a: &str, b: &str) -> bool {
    tokens(a) == tokens(b)
}

/// SQuAD 的 token F1。兩邊都沒有 token 時，一樣就是 1，不一樣就是 0。
pub fn f1(a: &str, b: &str) -> f64 {
    let ta = tokens(a);
    let tb = tokens(b);
    if ta.is_empty() || tb.is_empty() {
        return f64::from(u8::from(ta == tb));
    }
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for t in &ta {
        *counts.entry(t.as_str()).or_default() += 1;
    }
    let mut common = 0;
    for t in &tb {
        if let Some(n) = counts.get_mut(t.as_str()) {
            if *n > 0 {
                *n -= 1;
                common += 1;
            }
        }
    }
    if common == 0 {
        return 0.0;
    }
    let precision = common as f64 / tb.len() as f64;
    let recall = common as f64 / ta.len() as f64;
    2.0 * precision * recall / (precision + recall)
}

/// 一個問題上，不同標註者之間的一致程度。
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Agreement {
    /// 有幾組不同標註者的答案。
    pub annotators: usize,
    /// 兩兩比較時完全一致的比例（0–1）。
    pub exact: f64,
    /// 兩兩比較的平均 F1（0–1）。
    pub f1: f64,
}

/// 每一組是一位標註者的所有答案。兩組之間取最像的那一對（和 SQuAD 對多個標準答案取最大值一樣）。
/// 少於兩組時沒有一致程度可算。
pub fn agreement(groups: &[Vec<&str>]) -> Option<Agreement> {
    let groups: Vec<&Vec<&str>> = groups.iter().filter(|g| !g.is_empty()).collect();
    if groups.len() < 2 {
        return None;
    }
    let (mut em_sum, mut f1_sum, mut pairs) = (0.0, 0.0, 0usize);
    for i in 0..groups.len() {
        for j in i + 1..groups.len() {
            let mut best_em = 0.0f64;
            let mut best_f1 = 0.0f64;
            for a in groups[i] {
                for b in groups[j] {
                    best_em = best_em.max(f64::from(u8::from(exact_match(a, b))));
                    best_f1 = best_f1.max(f1(a, b));
                }
            }
            em_sum += best_em;
            f1_sum += best_f1;
            pairs += 1;
        }
    }
    Some(Agreement {
        annotators: groups.len(),
        exact: em_sum / pairs as f64,
        f1: f1_sum / pairs as f64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn tokens_split_cjk_per_character_and_drop_articles_and_punctuation() {
        assert_eq!(tokens("The Eiffel Tower!"), vec!["eiffel", "tower"]);
        assert_eq!(
            tokens("台北 101，高 508 公尺"),
            vec!["台", "北", "101", "高", "508", "公", "尺"]
        );
        assert!(tokens("。，!?").is_empty());
    }

    #[test]
    fn exact_match_ignores_case_articles_and_punctuation() {
        assert!(exact_match("the Eiffel Tower.", "Eiffel tower"));
        assert!(!exact_match("Eiffel Tower", "Tower"));
        assert!(exact_match("台北市", "台北市。"));
    }

    #[test]
    fn f1_matches_the_squad_definition() {
        // 共同 token：eiffel、tower → precision 2/2、recall 2/3 → F1 = 0.8
        assert!(close(f1("Eiffel Tower Paris", "the Eiffel Tower"), 0.8));
        // 中文一字一 token：「台北市」對「台北」→ precision 1、recall 2/3 → F1 = 0.8
        assert!(close(f1("台北市", "台北"), 0.8));
        assert!(close(f1("abc", "xyz"), 0.0));
        assert!(close(f1("", ""), 1.0));
        assert!(close(f1("the", "abc"), 0.0));
    }

    #[test]
    fn agreement_takes_the_best_pair_between_annotators() {
        // 甲標了兩個答案，乙的答案和其中一個完全一樣 → 這一對 EM 1、F1 1
        let a = agreement(&[vec!["台北", "台北市"], vec!["台北市"]]).unwrap();
        assert_eq!(a.annotators, 2);
        assert!(close(a.exact, 1.0));
        assert!(close(a.f1, 1.0));

        // 三個人：甲乙一樣，丙不一樣 → 三對裡一對 EM 1、F1 平均 (1 + 0 + 0) / 3
        let b = agreement(&[vec!["Paris"], vec!["paris"], vec!["London"]]).unwrap();
        assert!(close(b.exact, 1.0 / 3.0));
        assert!(close(b.f1, 1.0 / 3.0));

        assert!(agreement(&[vec!["only one"]]).is_none());
        assert!(agreement(&[vec!["x"], vec![]]).is_none());
    }
}
