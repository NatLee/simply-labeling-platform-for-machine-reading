//! 整合測試：真的啟動伺服器，模型服務換成本機的假伺服器（這些測試不需要任何 API key）。

use std::{
    collections::VecDeque,
    path::Path,
    sync::{Arc, Mutex},
};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use mrc_label::{
    app, build_state, dataset,
    llm::{LlmSettings, ProviderConfig, ProviderKind},
    Config,
};
use reqwest::Method;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::net::TcpListener;

// 「😀」在 UTF-16 佔 2 個單位、字元位置只佔 1 個，用來確認位置的單位。
const CONTEXT: &str = "台北😀101 高 508 公尺，在 2004 年完工。設計師是李祖原。";
// 「508 公尺」的字元位置：台 北 😀 1 0 1 空白 高 空白 → 第 9 個字元；UTF-16 位置是 10。
const ANSWER_START: usize = 9;
const ANSWER_UTF16: usize = 10;
const FAKE_KEY: &str = "sk-test-not-a-real-key";

struct TestApp {
    base: String,
    http: reqwest::Client,
    _dir: TempDir,
}

impl TestApp {
    async fn spawn(llm: LlmSettings) -> Self {
        let dir = TempDir::new().unwrap();
        let static_dir = dir.path().join("web");
        std::fs::create_dir_all(&static_dir).unwrap();
        std::fs::write(static_dir.join("index.html"), "<div id=\"root\"></div>").unwrap();
        let config = Config {
            bind: "127.0.0.1:0".into(),
            data_dir: dir.path().to_path_buf(),
            database_path: dir.path().join("test.db"),
            static_dir,
            max_upload_bytes: 10 * 1024 * 1024,
            llm,
        };
        let state = build_state(config).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app(state)).await.unwrap() });
        Self {
            base,
            http: reqwest::Client::new(),
            _dir: dir,
        }
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut req = self.http.request(method, format!("{}{path}", self.base));
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await.unwrap();
        let status = StatusCode::from_u16(resp.status().as_u16()).unwrap();
        let text = resp.text().await.unwrap();
        // 任何回應都不能帶出 API key
        assert!(!text.contains(FAKE_KEY), "API key leaked in {path}: {text}");
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    async fn get(&self, path: &str) -> Value {
        let (s, v) = self.call(Method::GET, path, None).await;
        assert!(s.is_success(), "GET {path} → {s}: {v}");
        v
    }

    async fn post(&self, path: &str, body: Value) -> Value {
        let (s, v) = self.call(Method::POST, path, Some(body)).await;
        assert!(s.is_success(), "POST {path} → {s}: {v}");
        v
    }

    async fn upload(&self, path: &str, bytes: Vec<u8>) -> (StatusCode, Value) {
        let resp = self
            .http
            .post(format!("{}{path}", self.base))
            .header("content-type", "application/octet-stream")
            .body(bytes)
            .send()
            .await
            .unwrap();
        let status = StatusCode::from_u16(resp.status().as_u16()).unwrap();
        (status, resp.json().await.unwrap())
    }

    /// 建立專案、標註者和一篇文章，回傳 (project_id, annotator_id, article_id)。
    async fn seed(&self) -> (i64, i64, i64) {
        let p = self
            .post("/api/projects", json!({"name": "測試專案"}))
            .await;
        let a = self.post("/api/annotators", json!({"name": "小明"})).await;
        let art = self
            .post(
                &format!("/api/projects/{}/articles", p["id"]),
                json!({"title": "台北 101", "context": CONTEXT}),
            )
            .await;
        (
            p["id"].as_i64().unwrap(),
            a["id"].as_i64().unwrap(),
            art["id"].as_i64().unwrap(),
        )
    }
}

fn disabled() -> LlmSettings {
    LlmSettings::Disabled {
        reason: "沒有設定 LLM_PROVIDER".into(),
    }
}

// ───────────────────────────── 假的模型服務 ─────────────────────────────

#[derive(Default)]
struct FakeLlm {
    /// 依序回應；用完之後一直回最後一個。
    replies: Mutex<VecDeque<(u16, Value)>>,
    last: Mutex<Option<(u16, Value)>>,
    requests: Mutex<Vec<(HeaderMap, Value)>>,
}

impl FakeLlm {
    fn new(replies: Vec<(u16, Value)>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into()),
            ..Default::default()
        })
    }
}

async fn fake_handler(
    State(f): State<Arc<FakeLlm>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    f.requests.lock().unwrap().push((headers, body));
    let next = f.replies.lock().unwrap().pop_front();
    let (status, reply) = match next {
        Some(r) => {
            *f.last.lock().unwrap() = Some(r.clone());
            r
        }
        None => f.last.lock().unwrap().clone().unwrap(),
    };
    let mut h = HeaderMap::new();
    if status == 429 {
        h.insert("retry-after", "0".parse().unwrap());
    }
    (StatusCode::from_u16(status).unwrap(), h, Json(reply))
}

async fn spawn_fake(path: &str, fake: Arc<FakeLlm>) -> String {
    let router = Router::new()
        .route(path, post(fake_handler))
        .with_state(fake);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    base
}

/// 模型回答的 JSON：一題答案是原文片段，一題不是（要被丟掉）。
fn model_output() -> Value {
    json!({"suggestions": [
        {"question": "台北 101 有多高？", "answerable": true, "answer": "508 公尺",
         "evidence": "台北😀101 高 508 公尺，在 2004 年完工。", "reference_answer": "508 公尺。"},
        {"question": "誰設計的？", "answerable": true, "answer": "李祖原建築師",
         "evidence": "", "reference_answer": ""}
    ]})
}

fn anthropic_reply(output: &Value) -> Value {
    json!({
        "id": "msg_test", "type": "message", "role": "assistant", "model": "claude-opus-5-5",
        "stop_reason": "end_turn",
        "content": [
            {"type": "thinking", "thinking": "", "signature": "sig"},
            {"type": "text", "text": output.to_string()}
        ],
        "usage": {"input_tokens": 10, "output_tokens": 20}
    })
}

fn openai_reply(text: &str) -> Value {
    json!({"choices": [{"index": 0, "finish_reason": "stop",
                        "message": {"role": "assistant", "content": text}}]})
}

// ───────────────────────────── 測試 ─────────────────────────────

#[tokio::test]
async fn health_meta_static_files_and_unknown_api() {
    let app = TestApp::spawn(disabled()).await;
    assert_eq!(app.get("/api/health").await, json!("ok"));
    let meta = app.get("/api/meta").await;
    assert_eq!(meta["ai"]["enabled"], false);
    assert!(meta["ai"]["reason"]
        .as_str()
        .unwrap()
        .contains("LLM_PROVIDER"));
    assert_eq!(meta["export_formats"][0], "squad-v2");
    // 前端路由交給 index.html
    assert!(app.get("/").await.as_str().unwrap().contains("root"));
    assert!(app
        .get("/p/1/annotate/3")
        .await
        .as_str()
        .unwrap()
        .contains("root"));
    let (s, v) = app.call(Method::GET, "/api/nope", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(v["error"].is_string());
}

#[tokio::test]
async fn answers_must_match_the_article_text_at_character_offsets() {
    let app = TestApp::spawn(disabled()).await;
    let (_, annotator, article) = app.seed().await;
    let path = format!("/api/articles/{article}/questions");

    // UTF-16 位置（瀏覽器的 selectionStart）不能直接送，要換成字元位置
    let (s, v) = app
        .call(
            Method::POST,
            &path,
            Some(json!({"text": "多高？", "annotator_id": annotator,
                        "answers": [{"start": ANSWER_UTF16, "text": "508 公尺"}]})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(v["error"].as_str().unwrap().contains("對不上"), "{v}");

    let q = app
        .post(
            &path,
            json!({"text": "台北 101 有多高？", "annotator_id": annotator,
                   "reference_answer": "508 公尺高",
                   "answers": [{"start": ANSWER_START, "text": "508 公尺"}]}),
        )
        .await;
    assert_eq!(q["answers"][0]["start"], ANSWER_START);
    assert_eq!(q["origin"], "human");
    assert_eq!(q["status"], "pending");
    assert!(q["agreement"].is_null(), "只有一位標註者");

    // 無法回答（SQuAD 2.0）
    let q2 = app
        .post(
            &path,
            json!({"text": "門票多少錢？", "is_impossible": true, "annotator_id": annotator}),
        )
        .await;
    assert_eq!(q2["is_impossible"], true);

    let detail = app.get(&format!("/api/articles/{article}")).await;
    assert_eq!(detail["questions"].as_array().unwrap().len(), 2);
    assert_eq!(detail["position"], 1);
    assert_eq!(detail["total"], 1);
}

#[tokio::test]
async fn second_annotator_review_and_stats() {
    let app = TestApp::spawn(disabled()).await;
    let (project, alice, article) = app.seed().await;
    let bob = app.post("/api/annotators", json!({"name": "小華"})).await["id"]
        .as_i64()
        .unwrap();
    // 同名的標註者不會重複建立
    let again = app.post("/api/annotators", json!({"name": "小華"})).await;
    assert_eq!(again["id"], bob);

    let q = app
        .post(
            &format!("/api/articles/{article}/questions"),
            json!({"text": "多高？", "annotator_id": alice,
                   "answers": [{"start": ANSWER_START, "text": "508 公尺"}]}),
        )
        .await;
    let qid = q["id"].as_i64().unwrap();
    // 小華只標「508」：和「508 公尺」（3 個 token：508、公、尺）共同 1 個 → precision 1、recall 1/3、F1 0.5
    let viewed = app
        .post(
            &format!("/api/questions/{qid}/answers"),
            json!({"start": ANSWER_START, "text": "508", "annotator_id": bob}),
        )
        .await;
    let agreement = &viewed["agreement"];
    assert_eq!(agreement["annotators"], 2);
    assert_eq!(agreement["exact"], 0.0);
    assert!(
        (agreement["f1"].as_f64().unwrap() - 0.5).abs() < 1e-9,
        "{agreement}"
    );

    let reviewed = app
        .post(
            &format!("/api/questions/{qid}/review"),
            json!({"status": "approved", "annotator_id": bob, "note": "OK"}),
        )
        .await;
    assert_eq!(reviewed["status"], "approved");
    assert_eq!(reviewed["reviewed_by"], bob);

    let stats = app.get(&format!("/api/projects/{project}/stats")).await;
    assert_eq!(stats["articles"], 1);
    assert_eq!(stats["labeled_articles"], 1);
    assert_eq!(stats["questions"], 1);
    assert_eq!(stats["approved"], 1);
    assert_eq!(stats["answers"], 2);
    assert_eq!(stats["multi_annotated"], 1);
    assert_eq!(stats["annotators"].as_array().unwrap().len(), 2);

    let queue = app
        .get(&format!("/api/projects/{project}/review?status=approved"))
        .await;
    assert_eq!(queue["total"], 1);
    assert_eq!(queue["contexts"][article.to_string()], CONTEXT);

    // 「只匯出審核通過」：退回後就不匯出
    app.post(
        &format!("/api/questions/{qid}/review"),
        json!({"status": "rejected"}),
    )
    .await;
    let resp = app
        .http
        .get(format!(
            "{}/api/projects/{project}/export?format=jsonl",
            app.base
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.headers()["x-exported-questions"], "0");
}

#[tokio::test]
async fn duplicate_articles_point_to_the_existing_one_and_lists_filter() {
    let app = TestApp::spawn(disabled()).await;
    let (project, _, article) = app.seed().await;
    let (s, v) = app
        .call(
            Method::POST,
            &format!("/api/projects/{project}/articles"),
            // 換行不同也算同一篇（\r\n 會統一成 \n）
            Some(json!({"context": CONTEXT})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v, json!({"id": article, "created": false}));

    app.post(
        &format!("/api/projects/{project}/articles"),
        json!({"context": "第二篇\r\n文章"}),
    )
    .await;
    let page = app
        .get(&format!(
            "/api/projects/{project}/articles?filter=unlabeled&limit=10"
        ))
        .await;
    assert_eq!(page["total"], 2);
    let page = app
        .get(&format!(
            "/api/projects/{project}/articles?q=%E7%AC%AC%E4%BA%8C"
        ))
        .await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["preview"], "第二篇\n文章");

    let random = app
        .get(&format!(
            "/api/projects/{project}/articles/random?exclude={article}"
        ))
        .await;
    assert_ne!(random["id"], article);
}

const SQUAD_V2: &str = r#"{
  "version": "v2.0",
  "data": [{
    "title": "Taipei",
    "paragraphs": [{
      "context": "Taipei 101 is 508 metres tall. It was completed in 2004.",
      "qas": [
        {"id": "a1", "question": "How tall is Taipei 101?", "is_impossible": false,
         "answers": [{"text": "508 metres", "answer_start": 14}]},
        {"id": "a2", "question": "When was it completed?", "is_impossible": false,
         "answers": [{"text": "2004", "answer_start": 49}]},
        {"id": "a3", "question": "Who owns it?", "is_impossible": true, "answers": [],
         "plausible_answers": [{"text": "Taipei 101", "answer_start": 0}]}
      ]
    }]
  }]
}"#;

#[tokio::test]
async fn squad_import_is_idempotent_and_exports_round_trip() {
    let app = TestApp::spawn(disabled()).await;
    let p = app.post("/api/projects", json!({"name": "squad"})).await["id"]
        .as_i64()
        .unwrap();
    let (s, summary) = app
        .upload(&format!("/api/projects/{p}/import"), SQUAD_V2.into())
        .await;
    assert_eq!(s, StatusCode::OK, "{summary}");
    assert_eq!(summary["format"], "SQuAD JSON");
    assert_eq!(summary["articles_created"], 1);
    assert_eq!(summary["questions_created"], 3);
    // 「2004」的位置給錯了（正確是 51），匯入時找回來
    assert_eq!(summary["answers_exact"], 2);
    assert_eq!(summary["answers_moved"], 1);

    let (_, again) = app
        .upload(&format!("/api/projects/{p}/import"), SQUAD_V2.into())
        .await;
    assert_eq!(again["articles_merged"], 1);
    assert_eq!(again["questions_created"], 0);
    assert_eq!(again["questions_skipped"], 3);

    let resp = app
        .http
        .get(format!(
            "{}/api/projects/{p}/export?format=squad-v2",
            app.base
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.headers()["content-type"], "application/json");
    assert!(resp.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .contains("squad-v2.json"));
    let exported = resp.text().await.unwrap();
    let mut expected = dataset::parse_squad(SQUAD_V2.as_bytes()).unwrap();
    expected[0].qas[1].answers[0].start = 51;
    let got = dataset::parse_squad(exported.as_bytes()).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].context, expected[0].context);
    assert_eq!(got[0].qas.len(), 3);
    for (g, e) in got[0].qas.iter().zip(&expected[0].qas) {
        assert_eq!(g.id, e.id, "SQuAD 的 id 會保留");
        assert_eq!(g.question, e.question);
        assert_eq!(g.is_impossible, e.is_impossible);
        assert_eq!(g.answers, e.answers);
    }

    // 其他格式：JSONL 一行一題、RAG 評估和對話格式略過沒有參考答案的無法回答題
    for (format, lines) in [
        ("jsonl", 3),
        ("rag-jsonl", 2),
        ("chat-jsonl", 2),
        ("squad-v1", 0),
    ] {
        let body = app
            .http
            .get(format!(
                "{}/api/projects/{p}/export?format={format}",
                app.base
            ))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        if format == "squad-v1" {
            let v: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(
                v["data"][0]["paragraphs"][0]["qas"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
        } else {
            assert_eq!(body.lines().count(), lines, "{format}: {body}");
        }
    }
    let (s, v) = app
        .call(
            Method::GET,
            &format!("/api/projects/{p}/export?format=csv"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(v["error"].as_str().unwrap().contains("squad-v2"));
}

/// 照舊版 `utils/databaseTool.py` 的 schema 建一個資料庫，放進舊版前端真的會產生的資料。
async fn legacy_database(path: &Path) {
    use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions};
    let mut conn = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .connect()
        .await
        .unwrap();
    for sql in [
        r#"CREATE TABLE "article" ("id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT UNIQUE,
             "context" TEXT NOT NULL, "timestamp" TEXT NOT NULL, "description" TEXT)"#,
        r#"CREATE TABLE "question_answer" ("id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT UNIQUE,
             "article_id" INTEGER NOT NULL, "question" TEXT NOT NULL, "answer_start" INTEGER NOT NULL,
             "answer_string" TEXT NOT NULL, "timestamp" TEXT NOT NULL)"#,
    ] {
        sqlx::query(sql).execute(&mut conn).await.unwrap();
    }
    sqlx::query("INSERT INTO article VALUES (NULL, ?, '2023-05-01T10:20:30.123456+08:00', '新聞')")
        .bind(CONTEXT)
        .execute(&mut conn)
        .await
        .unwrap();
    for (q, start, ans) in [
        // 舊版前端存的是 UTF-16 位置
        ("台北 101 有多高？", ANSWER_UTF16 as i64, "508 公尺"),
        // 沒選答案就送出：答案 0、起點 0（舊版 bug）
        ("在哪一年完工？", 0, "0"),
        // 讀不到問題時送出空白（舊版 bug）
        ("", 3, "101"),
    ] {
        sqlx::query(
            "INSERT INTO question_answer VALUES (NULL, 1, ?, ?, ?, '2023-05-01T10:21:00+08:00')",
        )
        .bind(q)
        .bind(start)
        .bind(ans)
        .execute(&mut conn)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn legacy_database_upload_converts_offsets_and_keeps_unanswered_questions() {
    let app = TestApp::spawn(disabled()).await;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("mrc.label");
    legacy_database(&path).await;
    let p = app.post("/api/projects", json!({"name": "舊資料"})).await["id"]
        .as_i64()
        .unwrap();

    let bytes = std::fs::read(&path).unwrap();
    let (s, summary) = app
        .upload(&format!("/api/projects/{p}/import"), bytes.clone())
        .await;
    assert_eq!(s, StatusCode::OK, "{summary}");
    assert_eq!(summary["format"], "舊版資料庫（mrc.label）");
    assert_eq!(summary["articles_created"], 1);
    assert_eq!(summary["questions_created"], 2);
    assert_eq!(
        summary["answers_exact"], 1,
        "UTF-16 位置換成字元位置後完全對上"
    );

    let list = app.get(&format!("/api/projects/{p}/articles")).await;
    let article = list["items"][0]["id"].as_i64().unwrap();
    assert_eq!(list["items"][0]["description"], "新聞");
    let detail = app.get(&format!("/api/articles/{article}")).await;
    assert_eq!(detail["article"]["created_at"], "2023-05-01T02:20:30Z");
    let qs = detail["questions"].as_array().unwrap();
    assert_eq!(qs[0]["answers"][0]["start"], ANSWER_START);
    assert_eq!(qs[0]["origin"], "import");
    assert!(qs[1]["answers"].as_array().unwrap().is_empty());

    let stats = app.get(&format!("/api/projects/{p}/stats")).await;
    assert_eq!(stats["needs_answer"], 1);

    let (_, again) = app
        .upload(&format!("/api/projects/{p}/import"), bytes)
        .await;
    assert_eq!(again["questions_skipped"], 2);
    assert_eq!(again["questions_created"], 0);
}

#[tokio::test]
async fn legacy_http_api_still_works() {
    let app = TestApp::spawn(disabled()).await;
    let (s, v) = app.call(Method::GET, "/article", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["article_id"].is_null());

    let resp = app
        .http
        .post(format!("{}/article", app.base))
        .form(&[("article", CONTEXT), ("description", "說明")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.json::<Value>().await.unwrap()["status"], "success");
    let resp = app
        .http
        .post(format!("{}/article", app.base))
        .form(&[("description", "沒有文章")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let got = app.get("/article").await;
    assert_eq!(got["article"], CONTEXT);
    let article_id = got["article_id"].clone();
    assert!(got["timestamp"].as_str().unwrap().ends_with('Z'));

    let ok = app
        .post(
            "/question-answer",
            json!([
                {"article_id": article_id, "question": "多高？", "answer_start": ANSWER_UTF16, "answer_string": "508 公尺"},
                {"article_id": article_id, "answer_start": 0, "answer_string": 0}
            ]),
        )
        .await;
    assert_eq!(ok, json!({"success": true}));
    let detail = app.get(&format!("/api/articles/{article_id}")).await;
    let qs = detail["questions"].as_array().unwrap();
    assert_eq!(qs.len(), 1);
    assert_eq!(qs[0]["answers"][0]["start"], ANSWER_START);
    assert_eq!(qs[0]["origin"], "human");
    // 舊版 API 寫進最早的專案（沒有就建立「預設專案」）
    assert_eq!(app.get("/api/projects").await[0]["name"], "預設專案");
}

#[tokio::test]
async fn ai_endpoints_explain_why_they_are_off() {
    let app = TestApp::spawn(disabled()).await;
    let (_, _, article) = app.seed().await;
    let (s, v) = app
        .call(
            Method::POST,
            &format!("/api/articles/{article}/suggestions"),
            Some(json!({})),
        )
        .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert!(v["error"].as_str().unwrap().contains("LLM_PROVIDER"));
}

#[tokio::test]
async fn anthropic_provider_sends_a_native_messages_request_and_validates_spans() {
    // 第一次回 429（要重試），第二次才是正常回答
    let fake = FakeLlm::new(vec![
        (
            429,
            json!({"type": "error", "error": {"type": "rate_limit_error", "message": "slow down"}}),
        ),
        (200, anthropic_reply(&model_output())),
    ]);
    let base = spawn_fake("/v1/messages", fake.clone()).await;
    let app = TestApp::spawn(LlmSettings::from_env(|k| match k {
        "LLM_PROVIDER" => Some("anthropic".into()),
        "ANTHROPIC_API_KEY" => Some(FAKE_KEY.into()),
        "ANTHROPIC_BASE_URL" => Some(base.clone()),
        _ => None,
    }))
    .await;
    let meta = app.get("/api/meta").await;
    assert_eq!(
        meta["ai"],
        json!({"enabled": true, "provider": "anthropic", "model": "claude-opus-5-5"})
    );

    let (_, annotator, article) = app.seed().await;
    let res = app
        .post(
            &format!("/api/articles/{article}/suggestions"),
            json!({"count": 3, "annotator_id": annotator}),
        )
        .await;
    let suggestions = res["suggestions"].as_array().unwrap();
    assert_eq!(suggestions.len(), 1, "{res}");
    assert_eq!(suggestions[0]["answer_start"], ANSWER_START);
    assert_eq!(suggestions[0]["provider"], "anthropic");
    assert_eq!(res["dropped"][0]["reason"], "答案不是文章裡一字不差的片段");

    let requests = fake.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 2, "429 之後重試一次");
    let (headers, body) = &requests[1];
    assert_eq!(headers["x-api-key"], FAKE_KEY);
    assert_eq!(headers["anthropic-version"], "2026-10-21");
    assert_eq!(headers["anthropic-beta"], "server-side-fallback-2026-07-01");
    assert_eq!(body["model"], "claude-opus-5-5");
    assert_eq!(body["fallbacks"], "default");
    assert_eq!(body["output_config"]["format"]["type"], "json_schema");
    assert!(body["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains(CONTEXT));
    assert!(body["system"]
        .as_str()
        .unwrap()
        .contains("write 3 diverse questions"));

    // 修改後接受：問題改了 → ai_edited；答案沒改 → ai
    let sid = suggestions[0]["id"].as_i64().unwrap();
    let q = app
        .post(
            &format!("/api/suggestions/{sid}/accept"),
            json!({"annotator_id": annotator, "question": "台北 101 的高度是多少？"}),
        )
        .await;
    assert_eq!(q["origin"], "ai_edited");
    assert_eq!(q["answers"][0]["origin"], "ai");
    assert_eq!(q["created_by"], annotator);
    let (s, _) = app
        .call(
            Method::POST,
            &format!("/api/suggestions/{sid}/accept"),
            Some(json!({})),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "同一個建議不能接受兩次");
    assert!(app
        .get(&format!("/api/articles/{article}/suggestions"))
        .await
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn anthropic_refusals_become_a_readable_error() {
    let fake = FakeLlm::new(vec![(
        200,
        json!({"stop_reason": "refusal", "stop_details": {"type": "refusal", "category": "cyber"}, "content": []}),
    )]);
    let base = spawn_fake("/v1/messages", fake).await;
    let app = TestApp::spawn(LlmSettings::Enabled(ProviderConfig {
        kind: ProviderKind::Anthropic,
        model: "claude-opus-5-5".into(),
        base_url: base,
        api_key: None,
    }))
    .await;
    let (_, _, article) = app.seed().await;
    let (s, v) = app
        .call(
            Method::POST,
            &format!("/api/articles/{article}/suggestions"),
            Some(json!({})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    assert!(v["error"].as_str().unwrap().contains("拒絕"), "{v}");
}

#[tokio::test]
async fn openai_compatible_provider_answers_a_human_question() {
    // 小模型常把 JSON 包在 ```json 區塊裡
    let output = json!({"suggestions": [{"question": "How tall?", "answerable": true, "answer": "508 公尺",
                                         "evidence": "", "reference_answer": "508 公尺"}]});
    let fake = FakeLlm::new(vec![(
        200,
        openai_reply(&format!("```json\n{output}\n```")),
    )]);
    let base = spawn_fake("/v1/chat/completions", fake.clone()).await;
    let app = TestApp::spawn(LlmSettings::from_env(|k| match k {
        "LLM_PROVIDER" => Some("openai".into()),
        "OPENAI_MODEL" => Some("qwen2.5:0.5b".into()),
        "OPENAI_BASE_URL" => Some(format!("{base}/v1")),
        "OPENAI_API_KEY" => Some(FAKE_KEY.into()),
        _ => None,
    }))
    .await;
    let (_, annotator, article) = app.seed().await;
    // 人先寫好問題（還沒有答案），再請 AI 找答案
    let q = app
        .post(
            &format!("/api/articles/{article}/questions"),
            json!({"text": "台北 101 有多高？", "annotator_id": annotator}),
        )
        .await;
    let res = app
        .post(
            &format!("/api/articles/{article}/suggestions/answer"),
            json!({"question": "台北 101 有多高？", "annotator_id": annotator}),
        )
        .await;
    let s = &res["suggestions"][0];
    assert_eq!(s["question"], "台北 101 有多高？", "建議掛在人寫的問題上");
    assert_eq!(s["answer_start"], ANSWER_START);
    assert_eq!(s["provider"], "openai");
    assert_eq!(s["model"], "qwen2.5:0.5b");

    let (headers, body) = fake.requests.lock().unwrap()[0].clone();
    assert_eq!(headers["authorization"], format!("Bearer {FAKE_KEY}"));
    assert_eq!(body["model"], "qwen2.5:0.5b");
    assert_eq!(body["response_format"]["type"], "json_schema");

    // 答案加到原本的問題上，不另外建立問題
    let accepted = app
        .post(
            &format!("/api/suggestions/{}/accept", s["id"]),
            json!({"annotator_id": annotator, "question_id": q["id"]}),
        )
        .await;
    assert_eq!(accepted["id"], q["id"]);
    assert_eq!(accepted["answers"][0]["origin"], "ai");
    let detail = app.get(&format!("/api/articles/{article}")).await;
    assert_eq!(detail["questions"].as_array().unwrap().len(), 1);

    // 拒絕
    let res = app
        .post(
            &format!("/api/articles/{article}/suggestions/answer"),
            json!({"question": "台北 101 有多高？"}),
        )
        .await;
    let rejected = app
        .post(
            &format!("/api/suggestions/{}/reject", res["suggestions"][0]["id"]),
            json!({}),
        )
        .await;
    assert_eq!(rejected["status"], "rejected");
}
