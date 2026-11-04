import { Download, FileUp, UploadCloud } from "lucide-react";
import { useRef, useState } from "react";
import { useParams } from "react-router";
import { api } from "../api";
import { PageBody } from "../components/Layout";
import { ErrorBanner, PageHeader, Spinner } from "../components/ui";
import { useApp } from "../context";
import { EXPORT_FORMAT_INFO } from "../lib/format";
import type { ImportSummary } from "../types";

function Summary({ s }: { s: ImportSummary }) {
  const rows: [string, number][] = [
    ["新增文章", s.articles_created],
    ["合併到既有文章", s.articles_merged],
    ["新增問題", s.questions_created],
    ["重複而略過的問題", s.questions_skipped],
    ["位置正確的答案", s.answers_exact],
    ["位置修正過的答案", s.answers_moved],
    ["找不到而沒匯入的答案", s.answers_dropped],
  ];
  return (
    <div className="mt-4 rounded-xl border border-emerald-200 bg-emerald-50 p-4 text-sm dark:border-emerald-900 dark:bg-emerald-950/40">
      <p className="mb-2 font-medium text-emerald-800 dark:text-emerald-200">匯入完成（{s.format}）</p>
      <dl className="grid grid-cols-2 gap-x-6 gap-y-1 sm:grid-cols-3">
        {rows.map(([k, v]) => (
          <div key={k} className="flex justify-between gap-2">
            <dt className="text-zinc-600 dark:text-zinc-300">{k}</dt>
            <dd className="font-semibold tabular-nums">{v}</dd>
          </div>
        ))}
      </dl>
      {s.warnings.length > 0 && (
        <details className="mt-3">
          <summary className="cursor-pointer text-zinc-600 dark:text-zinc-300">提醒（{s.warnings.length}）</summary>
          <ul className="mt-1 list-disc space-y-0.5 pl-5 text-xs text-zinc-600 dark:text-zinc-400">
            {s.warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

export function DataPage() {
  const projectId = Number(useParams().pid);
  const { meta, refreshProjects } = useApp();
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [drag, setDrag] = useState(false);
  const [summary, setSummary] = useState<ImportSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [format, setFormat] = useState("squad-v2");
  const [onlyApproved, setOnlyApproved] = useState(false);

  const upload = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true);
    setError(null);
    setSummary(null);
    try {
      setSummary(await api.importFile(projectId, file));
      await refreshProjects();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      if (input.current) input.current.value = "";
    }
  };

  const formats = meta?.export_formats ?? Object.keys(EXPORT_FORMAT_INFO);

  return (
    <PageBody>
      <PageHeader title="匯入／匯出" subtitle="檔案格式會自動判斷。同一個檔案匯入兩次不會重複。" />
      <div className="grid gap-4 lg:grid-cols-2">
        <section className="card p-5">
          <h2 className="mb-1 flex items-center gap-2 font-semibold">
            <FileUp className="size-4 text-brand-600" /> 匯入
          </h2>
          <p className="mb-4 text-sm text-zinc-500 dark:text-zinc-400">
            支援 SQuAD v1.1／v2.0 JSON、JSONL（Hugging Face squad 格式，或一行一篇 {"{"}"context": …{"}"}），以及舊版平台的資料庫
            <code className="mx-1 rounded bg-zinc-100 px-1 dark:bg-zinc-800">mrc.label</code>。
            答案位置會逐一檢查，對不上的會在文章裡找回正確位置。
          </p>
          <button
            type="button"
            onClick={() => input.current?.click()}
            onDragOver={(e) => {
              e.preventDefault();
              setDrag(true);
            }}
            onDragLeave={() => setDrag(false)}
            onDrop={(e) => {
              e.preventDefault();
              setDrag(false);
              void upload(e.dataTransfer.files[0]);
            }}
            disabled={busy}
            className={`flex w-full flex-col items-center gap-2 rounded-xl border-2 border-dashed px-4 py-10 text-sm transition-colors ${
              drag
                ? "border-brand-500 bg-brand-50 dark:bg-brand-950/40"
                : "border-zinc-300 hover:border-brand-400 hover:bg-zinc-50 dark:border-zinc-700 dark:hover:bg-zinc-800/40"
            }`}
          >
            {busy ? <Spinner className="size-6" /> : <UploadCloud className="size-7 text-brand-500" />}
            <span className="font-medium">{busy ? "匯入中…" : "拖曳檔案到這裡，或點一下選擇檔案"}</span>
            <span className="text-xs text-zinc-500">最大 {meta?.max_upload_mb ?? "—"} MB</span>
          </button>
          <input
            ref={input}
            type="file"
            className="hidden"
            accept=".json,.jsonl,.label,.db,.sqlite,application/json"
            onChange={(e) => void upload(e.target.files?.[0])}
            data-testid="import-file"
          />
          {error && (
            <div className="mt-4">
              <ErrorBanner message={error} />
            </div>
          )}
          {summary && <Summary s={summary} />}
        </section>

        <section className="card p-5">
          <h2 className="mb-1 flex items-center gap-2 font-semibold">
            <Download className="size-4 text-brand-600" /> 匯出
          </h2>
          <p className="mb-4 text-sm text-zinc-500 dark:text-zinc-400">
            退回的題目不會匯出；可以回答但還沒有答案的題目也會略過。
          </p>
          <fieldset className="flex flex-col gap-2">
            <legend className="sr-only">格式</legend>
            {formats.map((id) => {
              const info = EXPORT_FORMAT_INFO[id] ?? { title: id, detail: "", ext: "" };
              return (
                <label
                  key={id}
                  className={`flex cursor-pointer gap-3 rounded-xl border px-3 py-2.5 transition-colors ${
                    format === id
                      ? "border-brand-500 bg-brand-50/70 dark:border-brand-400 dark:bg-brand-950/40"
                      : "border-zinc-200 hover:bg-zinc-50 dark:border-zinc-800 dark:hover:bg-zinc-800/40"
                  }`}
                >
                  <input
                    type="radio"
                    name="format"
                    className="mt-1 accent-brand-600"
                    checked={format === id}
                    onChange={() => setFormat(id)}
                  />
                  <span>
                    <span className="block text-sm font-medium">
                      {info.title} <span className="font-mono text-xs text-zinc-400">{info.ext}</span>
                    </span>
                    <span className="block text-xs text-zinc-500 dark:text-zinc-400">{info.detail}</span>
                  </span>
                </label>
              );
            })}
          </fieldset>
          <label className="mt-4 flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              className="size-4 accent-brand-600"
              checked={onlyApproved}
              onChange={(e) => setOnlyApproved(e.target.checked)}
            />
            只匯出審核通過的題目
          </label>
          <a className="btn btn-primary mt-4 w-full" href={api.exportUrl(projectId, format, onlyApproved)} download>
            <Download className="size-4" /> 下載
          </a>
        </section>
      </div>
    </PageBody>
  );
}
