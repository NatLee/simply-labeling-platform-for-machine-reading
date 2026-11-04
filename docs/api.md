# HTTP API

網頁介面用的就是這些 API，腳本也可以直接呼叫。全部在 `/api` 底下，送出和回傳都是 JSON（匯入和匯出除外）。錯誤回 `{"error": "訊息"}`，訊息是給人看的中文；只有網址裡的 id 不是數字、或 JSON 格式不對時，回的是框架（axum）產生的純文字說明。

- 答案位置 `start` 是**字元位置**（Unicode code point），和 SQuAD、Python 的字串索引一樣。瀏覽器的 `selectionStart` 是 UTF-16 位置，文章裡有 emoji 時兩者不同，送出前要換算。
- 後端會檢查「文章從 `start` 開始的文字」是否正好等於答案，對不上就回 400。
- 寫入的 API 可以帶 `annotator_id`（標註者），沒帶就是匿名。
- 列表 API 用 `limit`（預設 50，最多 200）和 `offset` 分頁。

## 系統

| 方法 | 路徑 | 說明 |
|---|---|---|
| GET | `/api/health` | 回 `ok` |
| GET | `/api/meta` | 版本、AI 狀態（`provider`、`model`，或關閉的原因）、支援的匯出格式、上傳大小上限。不含 API key |
| GET／POST | `/api/annotators` | 列出／新增標註者 `{"name"}`；同名的直接回傳已存在的那一位 |

## 專案和文章

| 方法 | 路徑 | 說明 |
|---|---|---|
| GET／POST | `/api/projects` | 列出（附文章數、問題數、通過數）／新增 `{"name", "description"}` |
| GET／PUT／DELETE | `/api/projects/{id}` | 讀取／改名稱和說明／刪除（連同裡面的所有資料） |
| GET | `/api/projects/{id}/stats` | 進度統計：文章、問題、各審核狀態、無法回答、還沒答案、一致性、AI 建議接受率、每位標註者的數量 |
| GET | `/api/projects/{id}/articles` | 文章列表。`q` 搜尋標題、內容、說明；`filter` 是 `unlabeled`、`pending` 或 `reviewed` |
| POST | `/api/projects/{id}/articles` | 新增 `{"title", "context", "description"}`。專案裡已經有一樣的文章時回 200 `{"id", "created": false}`，不重複建立 |
| GET | `/api/projects/{id}/articles/random` | 隨機一篇。`exclude=<id>` 排除目前這篇，`unlabeled=true` 只挑還沒有問題的 |
| GET | `/api/articles/{id}` | 文章、所有問題和答案、在專案裡的位置（`position`／`total`）、上一篇和下一篇的 id |
| PUT | `/api/articles/{id}` | 改 `title`、`description`。文章內容不能改，因為答案位置是照原文算的 |
| DELETE | `/api/articles/{id}` | 刪除 |

## 問題、答案和審核

| 方法 | 路徑 | 說明 |
|---|---|---|
| POST | `/api/articles/{id}/questions` | 新增問題 `{"text", "is_impossible", "reference_answer", "answers": [{"start", "text"}], "annotator_id"}`。任何一個答案位置對不上，整題都不存 |
| GET／PUT／DELETE | `/api/questions/{id}` | 讀取／改 `text`、`is_impossible`、`reference_answer`／刪除 |
| POST | `/api/questions/{id}/answers` | 加一個答案 `{"start", "text", "annotator_id"}`，例如第二位標註者作答 |
| DELETE | `/api/answers/{id}` | 刪除一個答案 |
| POST | `/api/questions/{id}/review` | 審核 `{"status": "approved" \| "rejected" \| "pending", "annotator_id", "note"}` |
| GET | `/api/projects/{id}/review` | 審核清單。`status` 篩選；回傳 `items`、`total`，和這一頁用到的文章全文 `contexts` |

問題的回傳格式包含 `answers` 和 `agreement`：有兩位以上標註者作答時，`agreement` 是 `{"annotators", "exact", "f1"}`，算法和 SQuAD 官方評估一樣（中文一字一個 token）。

`origin` 欄位記錄來源：`human`（人工）、`ai`（AI 建議照原樣接受）、`ai_edited`（AI 建議改過才接受）、`import`（匯入）。

## 匯入和匯出

| 方法 | 路徑 | 說明 |
|---|---|---|
| POST | `/api/projects/{id}/import` | request body 直接放檔案（`Content-Type: application/octet-stream`）。格式自動判斷：SQuAD v1／v2 JSON、JSONL、舊版資料庫 `mrc.label`。回傳匯入了幾篇文章、幾題、幾個答案位置被修正或丟掉 |
| GET | `/api/projects/{id}/export` | `format` 是 `squad-v2`（預設）、`squad-v1`、`jsonl`、`rag-jsonl`、`chat-jsonl`；`only_approved=true` 只匯出審核通過的。回應標頭 `X-Exported-Questions`、`X-Skipped-Questions` 是匯出和略過的題數 |

同一個檔案匯入兩次不會重複：一樣的文章會合併，同一篇文章裡一樣的問題會略過。

## AI 建議

AI 功能沒開時，這些 API 回 503 和原因。

| 方法 | 路徑 | 說明 |
|---|---|---|
| GET | `/api/articles/{id}/suggestions` | 這篇文章還沒處理的建議 |
| POST | `/api/articles/{id}/suggestions` | 請模型出題 `{"count": 1–10, "annotator_id"}`。回傳 `suggestions`（通過檢查的）和 `dropped`（沒通過的和原因） |
| POST | `/api/articles/{id}/suggestions/answer` | 請模型替問題找答案 `{"question", "annotator_id"}` |
| POST | `/api/suggestions/{id}/accept` | 接受。可以先修改：`question`、`is_impossible`、`reference_answer`、`answers`；`question_id` 表示把答案加到已經存在的問題上 |
| POST | `/api/suggestions/{id}/reject` | 拒絕 |

模型給的答案一定要是文章裡一字不差的片段。模型算不準位置，所以後端請它照抄答案和包含答案的那一句，再自己在文章裡找位置；找不到的建議會丟掉，不會存進資料庫。

## 舊版相容

舊版 Flask 平台的 3 個 API 還在，寫入最早建立的專案（沒有專案時自動建立「預設專案」）：

| 方法 | 路徑 | 說明 |
|---|---|---|
| GET | `/article` | 隨機一篇：`{"article_id", "article", "timestamp"}` |
| POST | `/article` | 表單欄位 `article`、`description` |
| POST | `/question-answer` | JSON 陣列，每筆 `article_id`、`question`、`answer_start`（UTF-16 位置，和舊版前端一樣）、`answer_string` |
