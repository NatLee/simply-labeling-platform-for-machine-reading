use std::path::Path;

use anyhow::{bail, Result};
use mrc_label::{app, build_state, routes::import_file, store, Config};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "用法：
  mrc-label                                啟動伺服器
  mrc-label import <檔案> [--project <名稱>]  匯入檔案到專案（沒有就建立）
                                           檔案可以是舊版資料庫 mrc.label、SQuAD JSON 或 JSONL；
                                           沒指定專案時用檔名當專案名稱

設定用環境變數，見 README。";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => serve(Config::from_env()?).await,
        Some("import") => {
            let Some(file) = args.get(1) else {
                bail!("{USAGE}");
            };
            let project = match args.get(2).map(String::as_str) {
                Some("--project") => match args.get(3) {
                    Some(name) => name.clone(),
                    None => bail!("{USAGE}"),
                },
                Some(other) => bail!("不認得的參數：{other}\n\n{USAGE}"),
                None => Path::new(file)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "匯入".into()),
            };
            import(Config::from_env()?, file, &project).await
        }
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => bail!("不認得的指令：{other}\n\n{USAGE}"),
    }
}

async fn serve(config: Config) -> Result<()> {
    tracing::info!(
        "MRC 標注平台 v{}｜資料庫 {}",
        env!("CARGO_PKG_VERSION"),
        config.database_path.display()
    );
    let bind = config.bind.clone();
    let state = build_state(config).await?;
    let listener = TcpListener::bind(&bind).await?;
    tracing::info!("網頁介面：http://{}", listener.local_addr()?);
    axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("已停止");
    Ok(())
}

async fn import(config: Config, file: &str, project: &str) -> Result<()> {
    let state = build_state(config).await?;
    let bytes = std::fs::read(file)?;
    let project = store::project_by_name_or_create(&state.db, project)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let summary = import_file(&state, project.id, &bytes)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("匯入到專案「{}」（id {}）", project.name, project.id);
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
