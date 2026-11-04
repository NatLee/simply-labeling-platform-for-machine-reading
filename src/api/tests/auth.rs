//! 整合測試：啟用內建登入（設了 ADMIN_PASSWORD）之後的行為。沒啟用時的行為由 `api.rs` 的測試確認。

mod common;

use std::collections::HashMap;

use mrc_label::{auth::AuthSettings, build_state};
use reqwest::{header, Method, StatusCode};
use serde_json::{json, Value};
use tempfile::TempDir;

const ADMIN_PASSWORD: &str = "admin-password-1";
const CONTEXT: &str = "台北 101 高 508 公尺，在 2004 年完工。";

fn auth_settings(vars: &[(&str, &str)]) -> AuthSettings {
    let map: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    AuthSettings::from_env(|k| map.get(k).cloned())
        .unwrap()
        .unwrap()
}

struct App {
    base: String,
    http: reqwest::Client,
    _dir: TempDir,
}

/// 一個登入的人：帶著 session cookie 送請求。
#[derive(Clone)]
struct Session {
    cookie: Option<String>,
}

const ANON: Session = Session { cookie: None };

impl App {
    async fn spawn(extra: &[(&str, &str)]) -> Self {
        let mut vars = vec![("ADMIN_PASSWORD", ADMIN_PASSWORD)];
        vars.extend_from_slice(extra);
        let dir = TempDir::new().unwrap();
        let base = common::serve(common::config(
            dir.path(),
            common::ai_off(),
            Some(auth_settings(&vars)),
        ))
        .await;
        Self {
            base,
            http: reqwest::Client::new(),
            _dir: dir,
        }
    }

    async fn send(
        &self,
        who: &Session,
        method: Method,
        path: &str,
        body: Option<Value>,
        origin: Option<&str>,
    ) -> (StatusCode, Value, reqwest::header::HeaderMap) {
        let mut req = self.http.request(method, format!("{}{path}", self.base));
        if let Some(c) = &who.cookie {
            req = req.header(header::COOKIE, c);
        }
        if let Some(o) = origin {
            req = req.header(header::ORIGIN, o);
        }
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let text = resp.text().await.unwrap();
        assert!(!text.contains(ADMIN_PASSWORD), "password leaked in {path}");
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
            headers,
        )
    }

    async fn call(
        &self,
        who: &Session,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (s, v, _) = self.send(who, method, path, body, None).await;
        (s, v)
    }

    async fn login(&self, username: &str, password: &str) -> (StatusCode, Session, Option<String>) {
        let (s, _, headers) = self
            .send(
                &ANON,
                Method::POST,
                "/api/auth/login",
                Some(json!({"username": username, "password": password})),
                None,
            )
            .await;
        let set_cookie = headers
            .get(header::SET_COOKIE)
            .map(|v| v.to_str().unwrap().to_string());
        let cookie = set_cookie
            .as_ref()
            .map(|c| c.split(';').next().unwrap().to_string());
        (s, Session { cookie }, set_cookie)
    }

    async fn admin(&self) -> Session {
        let (s, who, _) = self.login("admin", ADMIN_PASSWORD).await;
        assert_eq!(s, StatusCode::OK);
        who
    }

    async fn ok(&self, who: &Session, method: Method, path: &str, body: Option<Value>) -> Value {
        let (s, v) = self.call(who, method.clone(), path, body).await;
        assert!(s.is_success(), "{method} {path} → {s}: {v}");
        v
    }
}

#[tokio::test]
async fn everything_but_health_meta_and_the_login_page_needs_a_session() {
    let app = App::spawn(&[]).await;
    assert_eq!(
        app.call(&ANON, Method::GET, "/api/health", None).await.0,
        StatusCode::OK
    );
    let (_, meta) = app.call(&ANON, Method::GET, "/api/meta", None).await;
    assert_eq!(meta["auth"]["enabled"], true);
    let (_, state) = app.call(&ANON, Method::GET, "/api/auth/state", None).await;
    assert_eq!(state, json!({"enabled": true, "user": null}));
    // 前端檔案不用登入（登入頁在裡面）
    assert_eq!(
        app.call(&ANON, Method::GET, "/p/1/annotate", None).await.0,
        StatusCode::OK
    );

    for (method, path) in [
        (Method::GET, "/api/projects"),
        (Method::POST, "/api/projects"),
        (Method::GET, "/api/annotators"),
        (Method::POST, "/api/articles/1/suggestions"),
        (Method::POST, "/api/projects/1/import"),
        (Method::GET, "/api/projects/1/export"),
        (Method::GET, "/api/users"),
        // 舊版 API 也要登入
        (Method::GET, "/article"),
        (Method::POST, "/question-answer"),
    ] {
        let (s, v) = app.call(&ANON, method.clone(), path, Some(json!({}))).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{method} {path}: {v}");
    }
}

#[tokio::test]
async fn login_sets_an_http_only_cookie_and_logout_ends_the_session() {
    let app = App::spawn(&[("PUBLIC_URL", "https://label.example.com")]).await;
    let (s, who, set_cookie) = app.login("admin", ADMIN_PASSWORD).await;
    assert_eq!(s, StatusCode::OK);
    let set_cookie = set_cookie.unwrap();
    assert!(set_cookie.starts_with("mrc_session="), "{set_cookie}");
    for attr in ["HttpOnly", "SameSite=Lax", "Secure", "Path=/"] {
        assert!(set_cookie.contains(attr), "missing {attr}: {set_cookie}");
    }
    let (_, state) = app.call(&who, Method::GET, "/api/auth/state", None).await;
    assert_eq!(state["user"]["username"], "admin");
    assert_eq!(state["user"]["role"], "admin");
    app.ok(&who, Method::GET, "/api/projects", None).await;

    let (s, _, headers) = app
        .send(&who, Method::POST, "/api/auth/logout", None, None)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(headers[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .contains("Max-Age=0"));
    assert_eq!(
        app.call(&who, Method::GET, "/api/projects", None).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn wrong_passwords_are_rate_limited() {
    let app = App::spawn(&[]).await;
    for _ in 0..5 {
        let (s, _, _) = app.login("admin", "wrong-password").await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }
    // 第 6 次就算密碼對也先擋下
    let (s, _, cookie) = app.login("admin", ADMIN_PASSWORD).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert!(cookie.is_none());
    // 別的帳號不受影響；不存在的帳號回一樣的錯誤，猜不出帳號在不在
    let (s, _, _) = app.login("nobody", ADMIN_PASSWORD).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn roles_match_annotating_reviewing_and_managing() {
    let app = App::spawn(&[]).await;
    let admin = app.admin().await;
    for (name, role) in [("小明", "annotator"), ("小華", "reviewer")] {
        let u = app
            .ok(
                &admin,
                Method::POST,
                "/api/users",
                Some(json!({"username": name, "password": "password-123", "role": role})),
            )
            .await;
        assert_eq!(u["role"], role);
    }
    let project = app
        .ok(
            &admin,
            Method::POST,
            "/api/projects",
            Some(json!({"name": "p"})),
        )
        .await["id"]
        .clone();
    let article = app
        .ok(
            &admin,
            Method::POST,
            &format!("/api/projects/{project}/articles"),
            Some(json!({"context": CONTEXT})),
        )
        .await["id"]
        .clone();
    let (_, ann, _) = app.login("小明", "password-123").await;
    let (_, rev, _) = app.login("小華", "password-123").await;
    let me = app.ok(&ann, Method::GET, "/api/auth/state", None).await["user"].clone();

    // 標註者：可以標註，標註者自動是本人
    let q = app
        .ok(
            &ann,
            Method::POST,
            &format!("/api/articles/{article}/questions"),
            Some(json!({"text": "多高？", "answers": [{"start": 9, "text": "508 公尺"}]})),
        )
        .await;
    assert_eq!(q["created_by"], me["annotator_id"]);
    assert_eq!(q["answers"][0]["annotator_id"], me["annotator_id"]);
    let annotators = app.ok(&ann, Method::GET, "/api/annotators", None).await;
    assert!(annotators
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["name"] == "小明"));
    // 不能用別人的名字
    let other = annotators
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "小華")
        .unwrap()["id"]
        .clone();
    let (s, v) = app
        .call(
            &ann,
            Method::POST,
            &format!("/api/articles/{article}/questions"),
            Some(json!({"text": "冒名", "annotator_id": other})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{v}");
    // 登入後不能自己新增標註者名字
    assert_eq!(
        app.call(
            &admin,
            Method::POST,
            "/api/annotators",
            Some(json!({"name": "路人"}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    let qid = q["id"].clone();
    let review = Some(json!({"status": "approved"}));
    let forbidden_for_annotator = [
        (
            Method::POST,
            format!("/api/questions/{qid}/review"),
            review.clone(),
        ),
        (Method::GET, format!("/api/projects/{project}/export"), None),
        (
            Method::POST,
            format!("/api/projects/{project}/import"),
            Some(json!({})),
        ),
        (
            Method::POST,
            "/api/projects".to_string(),
            Some(json!({"name": "x"})),
        ),
        (Method::DELETE, format!("/api/articles/{article}"), None),
        (Method::GET, "/api/users".to_string(), None),
    ];
    for (method, path, body) in &forbidden_for_annotator {
        let (s, v) = app.call(&ann, method.clone(), path, body.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "annotator {method} {path}: {v}");
    }

    // 審核者：可以審核和匯出，不能匯入或管理
    let reviewed = app
        .ok(
            &rev,
            Method::POST,
            &format!("/api/questions/{qid}/review"),
            review,
        )
        .await;
    assert_eq!(reviewed["status"], "approved");
    app.ok(
        &rev,
        Method::GET,
        &format!("/api/projects/{project}/export?format=jsonl"),
        None,
    )
    .await;
    assert_eq!(
        app.call(&rev, Method::GET, "/api/users", None).await.0,
        StatusCode::FORBIDDEN
    );

    // 管理員：帳號列表
    let users = app.ok(&admin, Method::GET, "/api/users", None).await;
    assert_eq!(users.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn cross_site_writes_are_rejected() {
    let app = App::spawn(&[("PUBLIC_URL", "https://label.example.com")]).await;
    let admin = app.admin().await;
    let body = Some(json!({"name": "csrf"}));
    let (s, _, _) = app
        .send(
            &admin,
            Method::POST,
            "/api/projects",
            body.clone(),
            Some("https://evil.example"),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // 同源（PUBLIC_URL 或請求的 Host）可以
    let (s, _, _) = app
        .send(
            &admin,
            Method::POST,
            "/api/projects",
            body.clone(),
            Some("https://label.example.com"),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let own = app.base.clone();
    let (s, _, _) = app
        .send(
            &admin,
            Method::POST,
            "/api/projects",
            Some(json!({"name": "csrf2"})),
            Some(&own),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    // 別的網站也不能替人登入
    let (s, _, _) = app
        .send(
            &ANON,
            Method::POST,
            "/api/auth/login",
            Some(json!({"username": "admin", "password": ADMIN_PASSWORD})),
            Some("https://evil.example"),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn disabling_an_account_logs_it_out_and_admins_cannot_lock_themselves_out() {
    let app = App::spawn(&[]).await;
    let admin = app.admin().await;
    let u = app
        .ok(
            &admin,
            Method::POST,
            "/api/users",
            Some(json!({"username": "ann", "password": "password-123", "role": "annotator"})),
        )
        .await;
    let (_, ann, _) = app.login("ann", "password-123").await;
    app.ok(&ann, Method::GET, "/api/projects", None).await;

    app.ok(
        &admin,
        Method::PUT,
        &format!("/api/users/{}", u["id"]),
        Some(json!({"disabled": true})),
    )
    .await;
    assert_eq!(
        app.call(&ann, Method::GET, "/api/projects", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.login("ann", "password-123").await.0,
        StatusCode::UNAUTHORIZED
    );

    let me = app.ok(&admin, Method::GET, "/api/auth/state", None).await["user"]["id"].clone();
    let (s, _) = app
        .call(
            &admin,
            Method::PUT,
            &format!("/api/users/{me}"),
            Some(json!({"disabled": true})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // 重複的帳號和太短的密碼
    let (s, _) = app
        .call(
            &admin,
            Method::POST,
            "/api/users",
            Some(json!({"username": "ANN", "password": "password-123", "role": "annotator"})),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = app
        .call(
            &admin,
            Method::POST,
            "/api/users",
            Some(json!({"username": "bob", "password": "short", "role": "annotator"})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn restarting_with_a_new_admin_password_resets_it() {
    let dir = TempDir::new().unwrap();
    let first = auth_settings(&[("ADMIN_PASSWORD", "first-password")]);
    build_state(common::config(dir.path(), common::ai_off(), Some(first)))
        .await
        .unwrap();
    let second = auth_settings(&[("ADMIN_PASSWORD", "second-password")]);
    let base = common::serve(common::config(dir.path(), common::ai_off(), Some(second))).await;
    let app = App {
        base,
        http: reqwest::Client::new(),
        _dir: dir,
    };
    assert_eq!(
        app.login("admin", "first-password").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.login("admin", "second-password").await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn legacy_api_works_with_a_session_and_records_who_wrote_it() {
    let app = App::spawn(&[]).await;
    let admin = app.admin().await;
    let resp = app
        .http
        .post(format!("{}/article", app.base))
        .header(header::COOKIE, admin.cookie.clone().unwrap())
        .form(&[("article", CONTEXT)])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let got = app.ok(&admin, Method::GET, "/article", None).await;
    let article_id = got["article_id"].clone();
    // 舊版 API 的位置是 UTF-16；這段文字沒有 emoji，兩種單位一樣
    app.ok(
        &admin,
        Method::POST,
        "/question-answer",
        Some(json!([{"article_id": article_id, "question": "多高？", "answer_start": 9, "answer_string": "508 公尺"}])),
    )
    .await;
    let me =
        app.ok(&admin, Method::GET, "/api/auth/state", None).await["user"]["annotator_id"].clone();
    let detail = app
        .ok(
            &admin,
            Method::GET,
            &format!("/api/articles/{article_id}"),
            None,
        )
        .await;
    assert_eq!(detail["questions"][0]["created_by"], me);
    assert_eq!(detail["questions"][0]["answers"][0]["annotator_id"], me);
}
