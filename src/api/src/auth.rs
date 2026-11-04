//! 內建登入（選用）。設了 `ADMIN_PASSWORD` 才啟用；沒設定時這裡的檢查全部跳過，和舊版一樣不用登入。
//!
//! - 密碼用 argon2 雜湊；session 是 HttpOnly、SameSite=Lax 的 cookie，資料庫只存 token 的 SHA-256。
//! - 每個 API 要什麼權限只寫在 [`required_level`] 這一張表，由 [`guard`] 這個 middleware 統一檢查，
//!   同時擋下不是同源送來的寫入請求（CSRF）。
//! - 登入後標註者就是帳號本人（[`Actor::annotator`]），不能用別人的名字寫入。

use std::{
    collections::HashMap,
    convert::Infallible,
    sync::Mutex,
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Result};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{FromRequestParts, MatchedPath, Request, State},
    http::{header, request::Parts, HeaderMap, Method},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    config::Secret,
    db,
    error::{AppError, AppResult},
    state::AppState,
    store,
    text::sha256_hex,
};

pub const COOKIE_NAME: &str = "mrc_session";
pub const SESSION_TTL_DAYS: i64 = 30;
pub const DEFAULT_ADMIN_USERNAME: &str = "admin";
pub const MIN_PASSWORD_CHARS: usize = 8;
const MAX_PASSWORD_BYTES: usize = 256;
const MAX_USERNAME_CHARS: usize = 40;
/// 同一個帳號在這段時間內失敗太多次，就暫時不讓它登入。
const FAILURE_WINDOW: Duration = Duration::from_secs(15 * 60);
const MAX_FAILURES: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Annotator,
    Reviewer,
}

/// 一個 API 需要的權限。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// 不用登入（健康檢查、版本、登入本身）。
    Public,
    /// 任何登入的人：讀取和標註。
    Member,
    /// 審核者和管理員：審核、匯出資料集。
    Review,
    /// 只有管理員：專案、刪文章、匯入、帳號。
    Admin,
}

impl Role {
    fn level(self) -> Level {
        match self {
            Role::Admin => Level::Admin,
            Role::Reviewer => Level::Review,
            Role::Annotator => Level::Member,
        }
    }
}

/// 每個 API 要什麼權限，全部寫在這裡。`path` 是路由的樣板（例如 `/api/projects/{id}`）。
/// 沒列到的讀取是 Member，沒列到的寫入也是 Member：漏寫的 API 不會變成公開。
pub fn required_level(method: &Method, path: &str) -> Level {
    match (method.as_str(), path) {
        (
            _,
            "/api/health" | "/api/meta" | "/api/auth/state" | "/api/auth/login"
            | "/api/auth/logout",
        ) => Level::Public,
        (_, p) if p == "/api/users" || p.starts_with("/api/users/") => Level::Admin,
        ("POST", "/api/projects")
        | ("PUT" | "DELETE", "/api/projects/{id}")
        | ("POST", "/api/projects/{id}/import")
        | ("DELETE", "/api/articles/{id}") => Level::Admin,
        ("POST", "/api/questions/{id}/review") | ("GET", "/api/projects/{id}/export") => {
            Level::Review
        }
        _ => Level::Member,
    }
}

// ───────────────────────────── 設定 ─────────────────────────────

#[derive(Clone, Debug)]
pub struct AuthSettings {
    pub admin_username: String,
    pub admin_password: Secret,
    /// `PUBLIC_URL` 的 origin（例如 `https://label.example.com`），同源檢查用。
    pub public_origin: Option<String>,
    /// cookie 加不加 Secure：`COOKIE_SECURE` 有設定就照它，否則看 `PUBLIC_URL` 是不是 https。
    pub cookie_secure: bool,
}

impl AuthSettings {
    /// | 變數 | 說明 |
    /// |---|---|
    /// | `ADMIN_PASSWORD` | 設了才啟用登入；至少 8 個字元。沒有預設密碼 |
    /// | `ADMIN_USERNAME` | 預設 `admin` |
    /// | `PUBLIC_URL` | 外部看到的網址，例如 `https://label.example.com` |
    /// | `COOKIE_SECURE` | `true`／`false`；沒設定時 `PUBLIC_URL` 是 https 就是 true |
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Option<Self>> {
        let Some(password) = get("ADMIN_PASSWORD") else {
            return Ok(None);
        };
        if password.chars().count() < MIN_PASSWORD_CHARS {
            bail!("ADMIN_PASSWORD 至少要 {MIN_PASSWORD_CHARS} 個字元");
        }
        let admin_username = get("ADMIN_USERNAME")
            .map(|u| u.trim().to_string())
            .unwrap_or_else(|| DEFAULT_ADMIN_USERNAME.into());
        validate_username(&admin_username).map_err(|e| anyhow!("ADMIN_USERNAME 不合法：{e}"))?;
        let public_origin =
            match get("PUBLIC_URL") {
                Some(url) => Some(origin_of(&url).ok_or_else(|| {
                    anyhow!("PUBLIC_URL 要是 http:// 或 https:// 開頭的網址：{url}")
                })?),
                None => None,
            };
        let cookie_secure = match get("COOKIE_SECURE") {
            Some(v) => match v.trim().to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => true,
                "0" | "false" | "no" | "off" => false,
                other => bail!("COOKIE_SECURE 只能是 true 或 false：{other}"),
            },
            None => public_origin
                .as_deref()
                .is_some_and(|o| o.starts_with("https://")),
        };
        Ok(Some(Self {
            admin_username,
            admin_password: Secret::new(password),
            public_origin,
            cookie_secure,
        }))
    }
}

/// `https://Example.com:8443/path` → `https://example.com:8443`。
fn origin_of(url: &str) -> Option<String> {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let host = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
    (!host.is_empty()).then(|| format!("{scheme}://{host}"))
}

/// 執行時的登入狀態：設定、登入失敗紀錄，和「帳號不存在時」用來比對的雜湊（讓回應時間差不多，猜不出帳號在不在）。
pub struct AuthRuntime {
    pub settings: AuthSettings,
    failures: Mutex<HashMap<String, Vec<Instant>>>,
    dummy_hash: String,
}

impl AuthRuntime {
    pub async fn new(settings: AuthSettings) -> Result<Self> {
        let dummy_hash = hash_password(random_hex(16)).await?;
        Ok(Self {
            settings,
            failures: Mutex::new(HashMap::new()),
            dummy_hash,
        })
    }

    fn check_rate_limit(&self, username: &str) -> AppResult<()> {
        let mut map = self.failures.lock().unwrap();
        let entry = map.entry(username.trim().to_lowercase()).or_default();
        entry.retain(|t| t.elapsed() < FAILURE_WINDOW);
        if entry.len() >= MAX_FAILURES {
            return Err(AppError::TooManyRequests(format!(
                "登入失敗太多次，請 {} 分鐘後再試",
                FAILURE_WINDOW.as_secs() / 60
            )));
        }
        Ok(())
    }

    fn record_failure(&self, username: &str) {
        self.failures
            .lock()
            .unwrap()
            .entry(username.trim().to_lowercase())
            .or_default()
            .push(Instant::now());
    }

    fn clear_failures(&self, username: &str) {
        self.failures
            .lock()
            .unwrap()
            .remove(&username.trim().to_lowercase());
    }
}

// ───────────────────────────── 密碼和 session ─────────────────────────────

pub async fn hash_password(password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| anyhow!("密碼雜湊失敗：{e}"))
    })
    .await?
}

pub async fn verify_password(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|parsed| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

pub fn validate_password(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(AppError::bad(format!(
            "密碼至少要 {MIN_PASSWORD_CHARS} 個字元"
        )));
    }
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(AppError::bad("密碼太長了"));
    }
    Ok(())
}

pub fn validate_username(name: &str) -> std::result::Result<(), String> {
    let n = name.chars().count();
    if n == 0 || n > MAX_USERNAME_CHARS {
        return Err(format!("帳號要 1 到 {MAX_USERNAME_CHARS} 個字元"));
    }
    if name.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("帳號不能有空白或控制字元".into());
    }
    Ok(())
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// 登入的人。`annotator_id` 是同名的標註者，寫入時用它。
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
    pub role: Role,
    pub annotator_id: i64,
}

impl CurrentUser {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }
}

async fn user_for_token(db: &SqlitePool, token: &str) -> Result<Option<CurrentUser>> {
    // 被停用的帳號，已經登入的 session 也立刻失效
    Ok(sqlx::query_as::<_, CurrentUser>(
        "SELECT u.id, u.username, u.role, u.annotator_id FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.expires_at > ? AND u.disabled = 0",
    )
    .bind(sha256_hex(token))
    .bind(db::now())
    .fetch_optional(db)
    .await?)
}

async fn create_session(db: &SqlitePool, user_id: i64) -> Result<String> {
    let token = random_hex(32);
    let expires = chrono::Utc::now() + chrono::Duration::days(SESSION_TTL_DAYS);
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(sha256_hex(&token))
    .bind(user_id)
    .bind(db::now())
    .bind(expires.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
    .execute(db)
    .await?;
    Ok(token)
}

pub async fn delete_sessions_of(db: &SqlitePool, user_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}

pub fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
        .filter(|v| !v.is_empty())
}

pub fn session_cookie(token: &str, secure: bool) -> String {
    format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        SESSION_TTL_DAYS * 24 * 3600,
        if secure { "; Secure" } else { "" }
    )
}

pub fn clear_cookie(secure: bool) -> String {
    format!(
        "{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

/// 帳號密碼對的話回傳使用者並建立 session（回傳 token）；失敗太多次回 429。
pub async fn login(
    db: &SqlitePool,
    rt: &AuthRuntime,
    username: &str,
    password: &str,
) -> AppResult<(CurrentUser, String)> {
    rt.check_rate_limit(username)?;
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE username = ? AND disabled = 0")
            .bind(username.trim())
            .fetch_optional(db)
            .await?;
    let ok = match &row {
        Some((_, hash)) => verify_password(password.to_string(), hash.clone()).await,
        None => {
            verify_password(password.to_string(), rt.dummy_hash.clone()).await;
            false
        }
    };
    let Some((id, _)) = row.filter(|_| ok) else {
        rt.record_failure(username);
        return Err(AppError::Unauthorized("帳號或密碼錯誤".into()));
    };
    rt.clear_failures(username);
    let token = create_session(db, id).await?;
    let user = sqlx::query_as::<_, CurrentUser>(
        "SELECT id, username, role, annotator_id FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await?;
    Ok((user, token))
}

pub async fn logout(db: &SqlitePool, headers: &HeaderMap) -> Result<()> {
    if let Some(token) = read_cookie(headers, COOKIE_NAME) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(sha256_hex(&token))
            .execute(db)
            .await?;
    }
    Ok(())
}

/// 建立帳號，連到同名的標註者（沒有就建立）。
pub async fn create_user(
    db: &SqlitePool,
    username: &str,
    password: &str,
    role: Role,
) -> AppResult<i64> {
    let username = username.trim();
    validate_username(username).map_err(AppError::BadRequest)?;
    validate_password(password)?;
    let annotator = store::annotator_by_name_or_create(db, username).await?;
    let hash = hash_password(password.to_string()).await?;
    let res = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, role, annotator_id, created_at) VALUES (?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(username)
    .bind(hash)
    .bind(role)
    .bind(annotator.id)
    .bind(db::now())
    .fetch_one(db)
    .await;
    match res {
        Ok(id) => Ok(id),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Err(AppError::conflict(format!("已經有叫「{username}」的帳號")))
        }
        Err(e) => Err(e.into()),
    }
}

/// 啟動時確認管理員帳號：沒有就建立；有的話以環境變數的密碼為準（忘記密碼時重設），
/// 並確保它是啟用中的管理員，避免把自己鎖在外面。密碼換了就登出它所有的裝置。
pub async fn bootstrap_admin(db: &SqlitePool, settings: &AuthSettings) -> Result<()> {
    let name = &settings.admin_username;
    let password = settings.admin_password.expose();
    let existing: Option<(i64, String)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE username = ?")
            .bind(name)
            .fetch_optional(db)
            .await?;
    match existing {
        None => {
            create_user(db, name, password, Role::Admin)
                .await
                .map_err(|e| anyhow!("建立管理員帳號失敗：{e}"))?;
            tracing::info!("已建立管理員帳號 {name}");
        }
        Some((id, hash)) => {
            if !verify_password(password.to_string(), hash).await {
                sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
                    .bind(hash_password(password.to_string()).await?)
                    .bind(id)
                    .execute(db)
                    .await?;
                delete_sessions_of(db, id).await?;
                tracing::info!("管理員 {name} 的密碼已改成 ADMIN_PASSWORD 的值，並登出所有裝置");
            }
            sqlx::query("UPDATE users SET role = 'admin', disabled = 0 WHERE id = ?")
                .bind(id)
                .execute(db)
                .await?;
        }
    }
    Ok(())
}

// ───────────────────────────── middleware 和 extractor ─────────────────────────────

fn is_safe(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// 寫入請求是不是同源送來的。瀏覽器送跨站請求時一定會帶 `Origin`（或 `Sec-Fetch-Site: cross-site`），
/// 這兩個都沒有的是命令列工具之類的客戶端，它們要自己帶 session cookie，不會被別的網站借用。
fn same_origin(headers: &HeaderMap, settings: &AuthSettings) -> bool {
    if headers
        .get("sec-fetch-site")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("cross-site"))
    {
        return false;
    }
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return true;
    };
    let origin = origin.trim().to_ascii_lowercase();
    if settings.public_origin.as_deref() == Some(origin.as_str()) {
        return true;
    }
    let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let host = host.trim().to_ascii_lowercase();
    origin == format!("http://{host}") || origin == format!("https://{host}")
}

/// 所有路由前面的檢查：同源、登入、角色。沒啟用登入時什麼都不做。
/// 登入的人會放進 request 的 extensions，handler 用 [`Actor`] 拿。
pub async fn guard(
    State(state): State<AppState>,
    matched: Option<MatchedPath>,
    mut req: Request,
    next: Next,
) -> Response {
    let Some(rt) = &state.auth else {
        return next.run(req).await;
    };
    if !is_safe(req.method()) && !same_origin(req.headers(), &rt.settings) {
        return AppError::Forbidden("這個請求不是從本站送出的，已拒絕".into()).into_response();
    }
    let path = matched
        .as_ref()
        .map_or_else(|| req.uri().path().to_string(), |m| m.as_str().to_string());
    let level = required_level(req.method(), &path);
    let user = match read_cookie(req.headers(), COOKIE_NAME) {
        Some(token) => match user_for_token(&state.db, &token).await {
            Ok(u) => u,
            Err(e) => return AppError::Internal(e).into_response(),
        },
        None => None,
    };
    if level > Level::Public {
        let Some(u) = &user else {
            return AppError::Unauthorized("請先登入".into()).into_response();
        };
        if u.role.level() < level {
            let need = if level == Level::Admin {
                "管理員"
            } else {
                "審核者或管理員"
            };
            return AppError::Forbidden(format!("需要{need}權限")).into_response();
        }
    }
    if let Some(u) = user {
        req.extensions_mut().insert(u);
    }
    next.run(req).await
}

/// 這次請求是誰送的。沒啟用登入時是 `None`。
#[derive(Clone, Debug)]
pub struct Actor(pub Option<CurrentUser>);

impl<S: Send + Sync> FromRequestParts<S> for Actor {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        Ok(Self(parts.extensions.get::<CurrentUser>().cloned()))
    }
}

impl Actor {
    /// 寫入時記在資料上的標註者。登入時一律是本人：沒帶就用本人，帶了別人的 id 就拒絕；
    /// 沒啟用登入時和以前一樣，用請求裡的 id（要存在），沒帶就是匿名。
    pub async fn annotator(
        &self,
        db: &SqlitePool,
        requested: Option<i64>,
    ) -> AppResult<Option<i64>> {
        match (&self.0, requested) {
            (Some(u), None) => Ok(Some(u.annotator_id)),
            (Some(u), Some(id)) if id == u.annotator_id => Ok(Some(id)),
            (Some(_), Some(_)) => Err(AppError::Forbidden("登入後只能用自己的名字標註".into())),
            (None, Some(id)) => Ok(Some(store::annotator(db, id).await?.id)),
            (None, None) => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use axum::http::HeaderValue;

    use super::*;

    fn settings(vars: &[(&str, &str)]) -> Result<Option<AuthSettings>> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        AuthSettings::from_env(|k| map.get(k).cloned())
    }

    #[test]
    fn login_is_off_without_a_password_and_has_no_default_password() {
        assert!(settings(&[]).unwrap().is_none());
        assert!(settings(&[("ADMIN_USERNAME", "boss")]).unwrap().is_none());
        assert!(settings(&[("ADMIN_PASSWORD", "short")]).is_err());
    }

    #[test]
    fn secure_cookie_follows_public_url_unless_set() {
        let s = settings(&[
            ("ADMIN_PASSWORD", "long enough"),
            ("PUBLIC_URL", "https://Label.Example.com/app/"),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(s.admin_username, "admin");
        assert_eq!(
            s.public_origin.as_deref(),
            Some("https://label.example.com")
        );
        assert!(s.cookie_secure);
        let s = settings(&[
            ("ADMIN_PASSWORD", "long enough"),
            ("PUBLIC_URL", "http://localhost:8787"),
        ])
        .unwrap()
        .unwrap();
        assert!(!s.cookie_secure);
        let s = settings(&[("ADMIN_PASSWORD", "long enough"), ("COOKIE_SECURE", "true")])
            .unwrap()
            .unwrap();
        assert!(s.cookie_secure);
        assert!(settings(&[("ADMIN_PASSWORD", "long enough"), ("PUBLIC_URL", "ftp://x")]).is_err());
        assert!(
            !format!("{s:?}").contains("long enough"),
            "密碼不會出現在 log"
        );
    }

    #[test]
    fn permission_table() {
        let get = Method::GET;
        let post = Method::POST;
        assert_eq!(required_level(&get, "/api/health"), Level::Public);
        assert_eq!(required_level(&post, "/api/auth/login"), Level::Public);
        assert_eq!(required_level(&get, "/api/projects"), Level::Member);
        assert_eq!(
            required_level(&post, "/api/articles/{id}/questions"),
            Level::Member
        );
        assert_eq!(
            required_level(&post, "/api/articles/{id}/suggestions"),
            Level::Member
        );
        assert_eq!(
            required_level(&post, "/api/questions/{id}/review"),
            Level::Review
        );
        assert_eq!(
            required_level(&get, "/api/projects/{id}/export"),
            Level::Review
        );
        assert_eq!(
            required_level(&post, "/api/projects/{id}/import"),
            Level::Admin
        );
        assert_eq!(required_level(&post, "/api/projects"), Level::Admin);
        assert_eq!(required_level(&get, "/api/users"), Level::Admin);
        assert_eq!(
            required_level(&Method::PUT, "/api/users/{id}"),
            Level::Admin
        );
        // 舊版 API 和沒列到的路徑都要登入
        assert_eq!(required_level(&get, "/article"), Level::Member);
        assert_eq!(required_level(&post, "/question-answer"), Level::Member);
        assert_eq!(
            required_level(&Method::PATCH, "/api/whatever"),
            Level::Member
        );
        assert!(Role::Reviewer.level() >= Level::Review && Role::Annotator.level() < Level::Review);
    }

    #[test]
    fn same_origin_check() {
        let s = settings(&[
            ("ADMIN_PASSWORD", "long enough"),
            ("PUBLIC_URL", "https://label.example.com"),
        ])
        .unwrap()
        .unwrap();
        let headers = |pairs: &[(&'static str, &'static str)]| {
            let mut h = HeaderMap::new();
            for (k, v) in pairs {
                h.insert(*k, HeaderValue::from_static(v));
            }
            h
        };
        assert!(same_origin(&headers(&[]), &s), "命令列工具沒有 Origin");
        assert!(same_origin(
            &headers(&[("origin", "https://label.example.com")]),
            &s
        ));
        assert!(same_origin(
            &headers(&[
                ("origin", "http://127.0.0.1:8787"),
                ("host", "127.0.0.1:8787")
            ]),
            &s
        ));
        assert!(!same_origin(
            &headers(&[
                ("origin", "https://evil.example"),
                ("host", "label.example.com")
            ]),
            &s
        ));
        assert!(!same_origin(&headers(&[("origin", "null")]), &s));
        assert!(!same_origin(
            &headers(&[("sec-fetch-site", "cross-site")]),
            &s
        ));
    }

    #[test]
    fn cookies_are_http_only_and_lax() {
        let c = session_cookie("abc", true);
        assert!(c.starts_with("mrc_session=abc;"));
        assert!(c.contains("HttpOnly") && c.contains("SameSite=Lax") && c.ends_with("; Secure"));
        assert!(!session_cookie("abc", false).contains("Secure"));
        assert!(clear_cookie(false).contains("Max-Age=0"));
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; mrc_session=tok; b=2"),
        );
        assert_eq!(read_cookie(&h, COOKIE_NAME).as_deref(), Some("tok"));
    }

    #[test]
    fn usernames_and_passwords_are_validated() {
        assert!(validate_username("小明").is_ok());
        assert!(validate_username("a b").is_err());
        assert!(validate_username("").is_err());
        assert!(validate_password("1234567").is_err());
        assert!(validate_password("12345678").is_ok());
    }

    #[tokio::test]
    async fn hash_and_verify() {
        let h = hash_password("correct horse".into()).await.unwrap();
        assert!(h.starts_with("$argon2"));
        assert!(verify_password("correct horse".into(), h.clone()).await);
        assert!(!verify_password("wrong".into(), h).await);
        assert!(!verify_password("x".into(), "not-a-hash".into()).await);
    }
}
