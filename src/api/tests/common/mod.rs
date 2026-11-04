//! 整合測試共用：在暫存資料夾裡用指定的設定啟動伺服器。

use std::path::Path;

use mrc_label::{app, auth::AuthSettings, build_state, llm::LlmSettings, Config};
use tokio::net::TcpListener;

pub fn config(dir: &Path, llm: LlmSettings, auth: Option<AuthSettings>) -> Config {
    let static_dir = dir.join("web");
    std::fs::create_dir_all(&static_dir).unwrap();
    std::fs::write(static_dir.join("index.html"), "<div id=\"root\"></div>").unwrap();
    Config {
        bind: "127.0.0.1:0".into(),
        data_dir: dir.to_path_buf(),
        database_path: dir.join("test.db"),
        static_dir,
        max_upload_bytes: 10 * 1024 * 1024,
        llm,
        auth,
    }
}

/// 啟動伺服器，回傳網址（`http://127.0.0.1:<port>`）。
pub async fn serve(config: Config) -> String {
    let state = build_state(config).await.unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app(state)).await.unwrap() });
    base
}

pub fn ai_off() -> LlmSettings {
    LlmSettings::Disabled {
        reason: "沒有設定 LLM_PROVIDER".into(),
    }
}
