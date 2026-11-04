use std::io::Write;

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderName, HeaderValue},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::{
    dataset::{self, ExportFormat, ImportFormat},
    error::{AppError, AppResult},
    legacy,
    models::Origin,
    state::AppState,
    store::{self, ImportSummary},
    text::OffsetUnit,
};

/// 匯入一個檔案到專案，格式自動判斷。網頁的上傳和命令列的 `mrc-label import` 都走這裡。
pub async fn import_file(
    state: &AppState,
    project_id: i64,
    bytes: &[u8],
) -> AppResult<ImportSummary> {
    let format = dataset::detect(bytes).map_err(AppError::bad)?;
    let (articles, unit) = match format {
        ImportFormat::Squad => (
            dataset::parse_squad(bytes).map_err(AppError::bad)?,
            OffsetUnit::Chars,
        ),
        ImportFormat::Jsonl => (
            dataset::parse_jsonl(bytes).map_err(AppError::bad)?,
            OffsetUnit::Chars,
        ),
        ImportFormat::LegacyDb => {
            // sqlx 只能從檔案開 SQLite，先寫到暫存檔。
            let mut tmp = tempfile::NamedTempFile::new()?;
            tmp.write_all(bytes)?;
            tmp.flush()?;
            let articles = legacy::read_database(tmp.path())
                .await
                .map_err(|e| AppError::bad(format!("{e:#}")))?;
            (articles, legacy::LEGACY_OFFSET_UNIT)
        }
    };
    store::import_dataset(
        &state.db,
        project_id,
        articles,
        unit,
        Origin::Import,
        None,
        format.label(),
    )
    .await
}

/// 上傳的檔案直接放在 request body（`Content-Type: application/octet-stream`）。
pub async fn import(
    State(state): State<AppState>,
    Path(project_id): Path<i64>,
    body: Bytes,
) -> AppResult<Json<ImportSummary>> {
    store::project(&state.db, project_id).await?;
    Ok(Json(import_file(&state, project_id, &body).await?))
}

#[derive(Deserialize)]
pub struct ExportQuery {
    format: Option<String>,
    #[serde(default)]
    only_approved: bool,
}

/// 下載匯出檔。`X-Exported-Questions`、`X-Skipped-Questions` 回報匯出幾題、幾題因為沒有答案而略過。
pub async fn export(
    State(state): State<AppState>,
    Path(project_id): Path<i64>,
    Query(query): Query<ExportQuery>,
) -> AppResult<impl IntoResponse> {
    let project = store::project(&state.db, project_id).await?;
    let format: ExportFormat = query
        .format
        .as_deref()
        .unwrap_or(ExportFormat::SquadV2.id())
        .parse()
        .map_err(AppError::bad)?;
    let data = store::export_data(&state.db, project_id, query.only_approved).await?;
    let body = dataset::render(format, &data.articles, &data.ids);
    let exported: usize = data.ids.iter().map(Vec::len).sum();

    let file_name = format!("{}-{}.{}", project.name, format.id(), format.extension());
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(format.content_type()),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition(&file_name, project_id, format))
            .expect("content disposition is ASCII"),
    );
    headers.insert(
        HeaderName::from_static("x-exported-questions"),
        exported.into(),
    );
    headers.insert(
        HeaderName::from_static("x-skipped-questions"),
        data.skipped_incomplete.into(),
    );
    Ok((headers, body))
}

/// 檔名可能有中文：`filename` 放 ASCII 的備用名稱，`filename*` 放 UTF-8 的真正名稱（RFC 6266）。
fn content_disposition(file_name: &str, project_id: i64, format: ExportFormat) -> String {
    let fallback = format!(
        "project-{project_id}-{}.{}",
        format.id(),
        format.extension()
    );
    format!(
        "attachment; filename=\"{fallback}\"; filename*=UTF-8''{}",
        percent_encode(file_name)
    )
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_with_chinese_are_encoded() {
        let v = content_disposition("台北 v2-squad-v2.json", 3, ExportFormat::SquadV2);
        assert_eq!(
            v,
            "attachment; filename=\"project-3-squad-v2.json\"; \
             filename*=UTF-8''%E5%8F%B0%E5%8C%97%20v2-squad-v2.json"
        );
        assert!(v.is_ascii());
    }
}
