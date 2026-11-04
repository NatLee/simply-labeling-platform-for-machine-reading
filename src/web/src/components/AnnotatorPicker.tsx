import { Check, UserRound } from "lucide-react";
import { useState } from "react";
import { useApp } from "../context";
import { Dialog } from "./ui";

/** 選擇或新增標註者。標註者只有名字（和舊版一樣不需要登入），記在這個瀏覽器裡。 */
export function AnnotatorDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { annotators, annotator, setAnnotatorId, addAnnotator } = useApp();
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    if (!name.trim()) return;
    try {
      await addAnnotator(name.trim());
      setName("");
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <Dialog open={open} title="你是哪位標註者？" onClose={onClose}>
      <p className="mb-3 text-sm text-zinc-500 dark:text-zinc-400">
        每個問題和答案都會記下是誰標的，用來計算標註者之間的一致程度，也讓審核者知道要找誰。
      </p>
      {annotators.length > 0 && (
        <div className="mb-4 flex flex-wrap gap-2">
          {annotators.map((a) => (
            <button
              key={a.id}
              className={`btn ${a.id === annotator?.id ? "btn-primary" : "btn-secondary"}`}
              onClick={() => {
                setAnnotatorId(a.id);
                onClose();
              }}
            >
              {a.id === annotator?.id ? <Check className="size-3.5" /> : <UserRound className="size-3.5" />}
              {a.name}
            </button>
          ))}
        </div>
      )}
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <input
          className="input"
          placeholder="輸入你的名字，例如：小明"
          value={name}
          onChange={(e) => setName(e.target.value)}
          aria-label="標註者名字"
        />
        <button className="btn btn-primary" type="submit" disabled={!name.trim()}>
          新增
        </button>
      </form>
      {error && <p className="mt-2 text-sm text-rose-600">{error}</p>}
    </Dialog>
  );
}
