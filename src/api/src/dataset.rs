//! 資料集格式：匯入 SQuAD v1／v2 JSON、JSONL，匯出成可以直接訓練或評估的格式。
//!
//! 匯入和匯出都先轉成同一個中間格式（[`DatasetArticle`]），格式之間不直接互轉。

use std::{collections::HashMap, fmt, str::FromStr};

use serde::Deserialize;
use serde_json::{json, Value};

/// 一篇文章和它的問題。匯入時位置還沒檢查過；匯出時都是檢查過的字元位置。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DatasetArticle {
    pub title: String,
    pub context: String,
    pub description: String,
    pub created_at: Option<String>,
    pub qas: Vec<DatasetQa>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DatasetQa {
    pub id: Option<String>,
    pub question: String,
    pub is_impossible: bool,
    /// 可以回答時是答案；無法回答時是 plausible answers（看似答案的片段）。
    pub answers: Vec<Span>,
    pub reference_answer: String,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub text: String,
}

// ───────────────────────────── 匯入 ─────────────────────────────

/// 匯入檔案的格式，自動判斷。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFormat {
    Squad,
    Jsonl,
    /// 舊版 Flask 平台的 SQLite 資料庫（`mrc.label`）。
    LegacyDb,
}

impl ImportFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Squad => "SQuAD JSON",
            Self::Jsonl => "JSONL",
            Self::LegacyDb => "舊版資料庫（mrc.label）",
        }
    }
}

const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

pub fn detect(bytes: &[u8]) -> Result<ImportFormat, String> {
    if bytes.starts_with(SQLITE_MAGIC) {
        return Ok(ImportFormat::LegacyDb);
    }
    let text = decode(bytes)?;
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return Err("檔案是空的".into());
    }
    // 整個檔案是一個有 data 陣列的 JSON 物件 → SQuAD；否則當成一行一筆的 JSONL。
    if let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(trimmed) {
        if obj.get("data").is_some_and(Value::is_array) {
            return Ok(ImportFormat::Squad);
        }
    }
    Ok(ImportFormat::Jsonl)
}

fn decode(bytes: &[u8]) -> Result<&str, String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    std::str::from_utf8(bytes).map_err(|_| "檔案不是 UTF-8 文字".to_string())
}

#[derive(Deserialize)]
struct SquadFile {
    data: Vec<SquadEntry>,
}

#[derive(Deserialize)]
struct SquadEntry {
    #[serde(default)]
    title: String,
    paragraphs: Vec<SquadParagraph>,
}

#[derive(Deserialize)]
struct SquadParagraph {
    context: String,
    #[serde(default)]
    qas: Vec<SquadQa>,
}

#[derive(Deserialize)]
struct SquadQa {
    #[serde(default)]
    id: Option<Value>,
    question: String,
    #[serde(default)]
    answers: Vec<SquadAnswer>,
    #[serde(default)]
    is_impossible: bool,
    #[serde(default)]
    plausible_answers: Vec<SquadAnswer>,
    #[serde(default)]
    reference_answer: String,
}

#[derive(Deserialize)]
struct SquadAnswer {
    text: String,
    answer_start: i64,
}

fn id_string(v: Option<Value>) -> Option<String> {
    match v? {
        Value::String(s) if !s.is_empty() => Some(s),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn spans(answers: Vec<SquadAnswer>) -> Vec<Span> {
    answers
        .into_iter()
        .filter(|a| a.answer_start >= 0 && !a.text.is_empty())
        .map(|a| Span {
            start: a.answer_start as usize,
            text: a.text,
        })
        .collect()
}

/// SQuAD v1.1 和 v2.0 都用這個讀：沒有 `is_impossible` 欄位就是可以回答。
pub fn parse_squad(bytes: &[u8]) -> Result<Vec<DatasetArticle>, String> {
    let file: SquadFile =
        serde_json::from_str(decode(bytes)?).map_err(|e| format!("SQuAD JSON 格式不對：{e}"))?;
    let mut out = Vec::new();
    for entry in file.data {
        for p in entry.paragraphs {
            out.push(DatasetArticle {
                title: entry.title.clone(),
                context: p.context,
                qas: p
                    .qas
                    .into_iter()
                    .map(|q| {
                        let answers = if q.is_impossible {
                            spans(q.plausible_answers)
                        } else {
                            spans(q.answers)
                        };
                        DatasetQa {
                            id: id_string(q.id),
                            question: q.question,
                            is_impossible: q.is_impossible,
                            answers,
                            reference_answer: q.reference_answer,
                            created_at: None,
                        }
                    })
                    .collect(),
                ..Default::default()
            });
        }
    }
    Ok(out)
}

/// JSONL 的 `answers` 欄位接受兩種寫法：
/// Hugging Face 的 `{"text": [...], "answer_start": [...]}`，或 SQuAD 的 `[{"text", "answer_start"}]`。
#[derive(Deserialize)]
#[serde(untagged)]
enum JsonlAnswers {
    Columns {
        text: Vec<String>,
        answer_start: Vec<i64>,
    },
    Rows(Vec<SquadAnswer>),
}

#[derive(Deserialize)]
struct JsonlLine {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    title: String,
    context: String,
    #[serde(default)]
    question: Option<String>,
    #[serde(default)]
    answers: Option<JsonlAnswers>,
    #[serde(default)]
    is_impossible: Option<bool>,
    #[serde(default)]
    reference_answer: String,
}

/// 一行一筆。同一篇文章（標題和內容都一樣）的多行合併成一篇；沒有 `question` 的行只匯入文章。
/// 沒有 `is_impossible` 欄位時，有問題但沒有答案就當成無法回答（和 Hugging Face 的 squad_v2 一樣）。
pub fn parse_jsonl(bytes: &[u8]) -> Result<Vec<DatasetArticle>, String> {
    let text = decode(bytes)?;
    let mut out: Vec<DatasetArticle> = Vec::new();
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: JsonlLine =
            serde_json::from_str(line).map_err(|e| format!("第 {} 行格式不對：{e}", n + 1))?;
        let key = (row.title.clone(), row.context.clone());
        let idx = *index.entry(key).or_insert_with(|| {
            out.push(DatasetArticle {
                title: row.title.clone(),
                context: row.context.clone(),
                ..Default::default()
            });
            out.len() - 1
        });
        let Some(question) = row.question.filter(|q| !q.trim().is_empty()) else {
            continue;
        };
        let answers = match row.answers {
            Some(JsonlAnswers::Columns { text, answer_start }) => {
                if text.len() != answer_start.len() {
                    return Err(format!(
                        "第 {} 行的 answers.text 和 answers.answer_start 數量不一樣",
                        n + 1
                    ));
                }
                spans(
                    text.into_iter()
                        .zip(answer_start)
                        .map(|(text, answer_start)| SquadAnswer { text, answer_start })
                        .collect(),
                )
            }
            Some(JsonlAnswers::Rows(rows)) => spans(rows),
            None => Vec::new(),
        };
        let is_impossible = row.is_impossible.unwrap_or(answers.is_empty());
        out[idx].qas.push(DatasetQa {
            id: id_string(row.id),
            question,
            answers,
            is_impossible,
            reference_answer: row.reference_answer,
            created_at: None,
        });
    }
    Ok(out)
}

// ───────────────────────────── 匯出 ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    /// SQuAD 2.0 JSON，包含無法回答的題目；這個平台所有欄位都保留，可以再匯入。
    SquadV2,
    /// SQuAD 1.1 JSON，不含無法回答的題目。
    SquadV1,
    /// Hugging Face `datasets` 的 squad／squad_v2 格式，一行一題。
    Jsonl,
    /// RAG 評估資料集（Ragas 的欄位名稱：user_input、reference、reference_contexts）。
    RagJsonl,
    /// 對話格式的微調資料（messages：system、user、assistant）。
    ChatJsonl,
}

pub const EXPORT_FORMATS: [ExportFormat; 5] = [
    ExportFormat::SquadV2,
    ExportFormat::SquadV1,
    ExportFormat::Jsonl,
    ExportFormat::RagJsonl,
    ExportFormat::ChatJsonl,
];

impl ExportFormat {
    pub fn id(self) -> &'static str {
        match self {
            Self::SquadV2 => "squad-v2",
            Self::SquadV1 => "squad-v1",
            Self::Jsonl => "jsonl",
            Self::RagJsonl => "rag-jsonl",
            Self::ChatJsonl => "chat-jsonl",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::SquadV2 | Self::SquadV1 => "json",
            _ => "jsonl",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Self::SquadV2 | Self::SquadV1 => "application/json",
            _ => "application/x-ndjson",
        }
    }
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

impl FromStr for ExportFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        EXPORT_FORMATS
            .into_iter()
            .find(|f| f.id() == s)
            .ok_or_else(|| {
                let ids: Vec<_> = EXPORT_FORMATS.iter().map(|f| f.id()).collect();
                format!("不支援的匯出格式：{s}（可以用 {}）", ids.join("、"))
            })
    }
}

/// 對話格式微調資料的提示詞。要改成別的語言或寫法，改這裡就好。
pub const CHAT_SYSTEM_PROMPT: &str =
    "Answer the question using only the passage. If the passage does not contain the answer, say so.";

fn chat_user_message(context: &str, question: &str) -> String {
    format!("Passage:\n{context}\n\nQuestion: {question}")
}

/// 給 LLM 用的參考答案：有自由作答就用它，否則用第一個答案片段。
/// 無法回答又沒有寫參考答案的題目沒有東西可以當標準答案，回 `None`。
fn reference_for(q: &DatasetQa) -> Option<String> {
    let reference = q.reference_answer.trim();
    if !reference.is_empty() {
        return Some(reference.to_string());
    }
    if q.is_impossible {
        return None;
    }
    q.answers.first().map(|a| a.text.clone())
}

fn answer_objects(spans: &[Span]) -> Value {
    Value::Array(
        spans
            .iter()
            .map(|s| json!({ "text": s.text, "answer_start": s.start }))
            .collect(),
    )
}

/// 匯出時的一題，連同它的文章和唯一的 id。
struct ExportItem<'a> {
    article: &'a DatasetArticle,
    qa: &'a DatasetQa,
    id: String,
}

/// 依格式輸出。`articles` 裡的題目已經是要匯出的（審核狀態等篩選在外面做），id 也已經唯一。
pub fn render(format: ExportFormat, articles: &[DatasetArticle], ids: &[Vec<String>]) -> String {
    let items = || {
        articles.iter().zip(ids).flat_map(|(article, ids)| {
            article.qas.iter().zip(ids).map(move |(qa, id)| ExportItem {
                article,
                qa,
                id: id.clone(),
            })
        })
    };
    match format {
        ExportFormat::SquadV2 | ExportFormat::SquadV1 => render_squad(format, articles, ids),
        ExportFormat::Jsonl => lines(items().map(|it| {
            let answers = if it.qa.is_impossible { &[][..] } else { &it.qa.answers[..] };
            json!({
                "id": it.id,
                "title": it.article.title,
                "context": it.article.context,
                "question": it.qa.question,
                "answers": {
                    "text": answers.iter().map(|a| a.text.clone()).collect::<Vec<_>>(),
                    "answer_start": answers.iter().map(|a| a.start).collect::<Vec<_>>(),
                },
            })
        })),
        ExportFormat::RagJsonl => lines(items().filter_map(|it| {
            let reference = reference_for(it.qa)?;
            Some(json!({
                "id": it.id,
                "user_input": it.qa.question,
                "reference": reference,
                "reference_contexts": [it.article.context],
            }))
        })),
        ExportFormat::ChatJsonl => lines(items().filter_map(|it| {
            let reference = reference_for(it.qa)?;
            Some(json!({
                "messages": [
                    { "role": "system", "content": CHAT_SYSTEM_PROMPT },
                    { "role": "user", "content": chat_user_message(&it.article.context, &it.qa.question) },
                    { "role": "assistant", "content": reference },
                ]
            }))
        })),
    }
}

fn lines(values: impl Iterator<Item = Value>) -> String {
    let mut out = String::new();
    for v in values {
        out.push_str(&v.to_string());
        out.push('\n');
    }
    out
}

/// 同一個標題的文章放在同一個 `data` 項目底下，匯入再匯出時結構不變。
fn render_squad(format: ExportFormat, articles: &[DatasetArticle], ids: &[Vec<String>]) -> String {
    let v2 = format == ExportFormat::SquadV2;
    let mut data: Vec<(String, Vec<Value>)> = Vec::new();
    let mut by_title: HashMap<&str, usize> = HashMap::new();
    for (article, ids) in articles.iter().zip(ids) {
        let qas: Vec<Value> = article
            .qas
            .iter()
            .zip(ids)
            .filter(|(qa, _)| v2 || !qa.is_impossible)
            .map(|(qa, id)| {
                let mut obj = json!({ "id": id, "question": qa.question });
                if v2 {
                    obj["is_impossible"] = json!(qa.is_impossible);
                    if qa.is_impossible {
                        obj["answers"] = json!([]);
                        if !qa.answers.is_empty() {
                            obj["plausible_answers"] = answer_objects(&qa.answers);
                        }
                    } else {
                        obj["answers"] = answer_objects(&qa.answers);
                    }
                    if !qa.reference_answer.is_empty() {
                        obj["reference_answer"] = json!(qa.reference_answer);
                    }
                } else {
                    obj["answers"] = answer_objects(&qa.answers);
                }
                obj
            })
            .collect();
        let paragraph = json!({ "context": article.context, "qas": qas });
        let idx = *by_title.entry(article.title.as_str()).or_insert_with(|| {
            data.push((article.title.clone(), Vec::new()));
            data.len() - 1
        });
        data[idx].1.push(paragraph);
    }
    let data: Vec<Value> = data
        .into_iter()
        .map(|(title, paragraphs)| json!({ "title": title, "paragraphs": paragraphs }))
        .collect();
    let version = if v2 { "v2.0" } else { "1.1" };
    serde_json::to_string_pretty(&json!({ "version": version, "data": data }))
        .expect("serializing JSON values cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUAD_V2: &str = r#"{
      "version": "v2.0",
      "data": [{
        "title": "台北",
        "paragraphs": [{
          "context": "台北 101 高 508 公尺，2004 年完工。",
          "qas": [
            {"id": "q1", "question": "台北 101 有多高？", "is_impossible": false,
             "answers": [{"text": "508 公尺", "answer_start": 9}]},
            {"id": "q2", "question": "誰設計了台北 101？", "is_impossible": true,
             "answers": [], "plausible_answers": [{"text": "台北 101", "answer_start": 0}]}
          ]
        }, {
          "context": "台北是台灣的首都。",
          "qas": [{"id": 3, "question": "台灣的首都是哪裡？",
                   "answers": [{"text": "台北", "answer_start": 0}]}]
        }]
      }]
    }"#;

    #[test]
    fn detects_formats() {
        assert_eq!(detect(SQUAD_V2.as_bytes()), Ok(ImportFormat::Squad));
        assert_eq!(detect(b"{\"context\": \"x\"}\n"), Ok(ImportFormat::Jsonl));
        assert_eq!(
            detect(b"\xEF\xBB\xBF{\"data\": []}"),
            Ok(ImportFormat::Squad)
        );
        assert_eq!(detect(b"SQLite format 3\0rest"), Ok(ImportFormat::LegacyDb));
        assert!(detect(b"   ").is_err());
        assert!(detect(b"\xff\xfe").is_err());
    }

    #[test]
    fn parses_squad_v2_including_plausible_answers() {
        let arts = parse_squad(SQUAD_V2.as_bytes()).unwrap();
        assert_eq!(arts.len(), 2);
        assert_eq!(arts[0].title, "台北");
        assert_eq!(arts[1].title, "台北");
        let q2 = &arts[0].qas[1];
        assert!(q2.is_impossible);
        assert_eq!(
            q2.answers,
            vec![Span {
                start: 0,
                text: "台北 101".into()
            }]
        );
        // 數字 id 轉成字串
        assert_eq!(arts[1].qas[0].id.as_deref(), Some("3"));
        // v1 的題目沒有 is_impossible 欄位，就是可以回答
        assert!(!arts[1].qas[0].is_impossible);
    }

    #[test]
    fn parses_jsonl_in_both_answer_layouts_and_groups_articles() {
        let jsonl = concat!(
            r#"{"id":"a","title":"T","context":"C1","question":"Q1","answers":{"text":["C"],"answer_start":[0]}}"#,
            "\n\n",
            r#"{"id":"b","title":"T","context":"C1","question":"Q2","answers":[{"text":"1","answer_start":1}]}"#,
            "\n",
            r#"{"title":"T","context":"C1","question":"Q3","answers":{"text":[],"answer_start":[]}}"#,
            "\n",
            r#"{"context":"only an article"}"#,
            "\n",
        );
        let arts = parse_jsonl(jsonl.as_bytes()).unwrap();
        assert_eq!(arts.len(), 2);
        assert_eq!(arts[0].qas.len(), 3);
        assert_eq!(arts[0].qas[1].answers[0].start, 1);
        assert!(arts[0].qas[2].is_impossible, "沒有答案就是無法回答");
        assert!(arts[1].qas.is_empty());
    }

    #[test]
    fn jsonl_errors_name_the_line() {
        let err = parse_jsonl(b"{\"context\":\"x\"}\nnot json\n").unwrap_err();
        assert!(err.contains("第 2 行"), "{err}");
        let err = parse_jsonl(
            br#"{"context":"x","question":"q","answers":{"text":["a"],"answer_start":[]}}"#,
        )
        .unwrap_err();
        assert!(err.contains("數量不一樣"), "{err}");
    }

    fn ids_for(arts: &[DatasetArticle]) -> Vec<Vec<String>> {
        arts.iter()
            .map(|a| a.qas.iter().map(|q| q.id.clone().unwrap()).collect())
            .collect()
    }

    #[test]
    fn squad_v2_round_trips() {
        let arts = parse_squad(SQUAD_V2.as_bytes()).unwrap();
        let out = render(ExportFormat::SquadV2, &arts, &ids_for(&arts));
        let again = parse_squad(out.as_bytes()).unwrap();
        assert_eq!(arts, again);
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["version"], "v2.0");
        // 同標題的兩段放在同一個 data 項目
        assert_eq!(v["data"].as_array().unwrap().len(), 1);
        assert_eq!(v["data"][0]["paragraphs"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn squad_v1_drops_unanswerable_questions() {
        let arts = parse_squad(SQUAD_V2.as_bytes()).unwrap();
        let out = render(ExportFormat::SquadV1, &arts, &ids_for(&arts));
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["version"], "1.1");
        let qas = v["data"][0]["paragraphs"][0]["qas"].as_array().unwrap();
        assert_eq!(qas.len(), 1);
        assert!(qas[0].get("is_impossible").is_none());
    }

    #[test]
    fn jsonl_export_matches_the_hugging_face_squad_layout_and_round_trips() {
        let arts = parse_squad(SQUAD_V2.as_bytes()).unwrap();
        let out = render(ExportFormat::Jsonl, &arts, &ids_for(&arts));
        let rows: Vec<Value> = out
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[0],
            json!({"id": "q1", "title": "台北", "context": "台北 101 高 508 公尺，2004 年完工。",
                   "question": "台北 101 有多高？",
                   "answers": {"text": ["508 公尺"], "answer_start": [9]}})
        );
        // 無法回答 → 空答案
        assert_eq!(rows[1]["answers"], json!({"text": [], "answer_start": []}));

        let again = parse_jsonl(out.as_bytes()).unwrap();
        assert_eq!(again[0].qas[0], arts[0].qas[0]);
        assert!(again[0].qas[1].is_impossible);
    }

    #[test]
    fn llm_formats_use_the_reference_answer_or_the_first_span() {
        let mut arts = parse_squad(SQUAD_V2.as_bytes()).unwrap();
        arts[0].qas[0].reference_answer = "台北 101 高 508 公尺。".into();
        let ids = ids_for(&arts);

        let rag: Vec<Value> = render(ExportFormat::RagJsonl, &arts, &ids)
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        // 無法回答又沒有參考答案的 q2 不匯出
        assert_eq!(rag.len(), 2);
        assert_eq!(rag[0]["user_input"], "台北 101 有多高？");
        assert_eq!(rag[0]["reference"], "台北 101 高 508 公尺。");
        assert_eq!(rag[0]["reference_contexts"][0], arts[0].context);
        assert_eq!(rag[1]["reference"], "台北");

        let chat: Vec<Value> = render(ExportFormat::ChatJsonl, &arts, &ids)
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(chat.len(), 2);
        let msgs = chat[0]["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert!(msgs[1]["content"]
            .as_str()
            .unwrap()
            .contains("台北 101 有多高？"));
        assert_eq!(msgs[2]["content"], "台北 101 高 508 公尺。");
    }

    #[test]
    fn export_format_ids_parse_back() {
        for f in EXPORT_FORMATS {
            assert_eq!(f.id().parse::<ExportFormat>(), Ok(f));
        }
        assert!("csv"
            .parse::<ExportFormat>()
            .unwrap_err()
            .contains("squad-v2"));
    }
}
