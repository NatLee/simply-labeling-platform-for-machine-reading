use std::{path::Path, time::Duration};

use anyhow::{Context, Result};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};

pub async fn connect(path: &Path) -> Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("建立資料夾失敗：{}", parent.display()))?;
        }
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await
        .with_context(|| format!("開不了資料庫：{}", path.display()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("資料庫 migration 失敗")?;
    Ok(pool)
}

/// 現在的 UTC 時間（RFC 3339，到秒）。
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// 外部資料的時間轉成 UTC；看不懂的話用現在時間。
pub fn to_utc(ts: Option<&str>) -> String {
    ts.and_then(|t| chrono::DateTime::parse_from_rfc3339(t.trim()).ok())
        .map(|t| {
            t.with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
        .unwrap_or_else(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_timestamps_convert_to_utc() {
        // 舊版存的是 Python isoformat()，帶 +08:00 和微秒
        assert_eq!(
            to_utc(Some("2023-05-01T10:20:30.123456+08:00")),
            "2023-05-01T02:20:30Z"
        );
        assert!(to_utc(Some("not a time")).ends_with('Z'));
    }
}
