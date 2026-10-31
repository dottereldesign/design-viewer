import { useEffect, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Download, FolderOpen, LoaderCircle, X } from "lucide-react";

type Progress = { phase: string; copied_bytes: number; total_bytes: number; completed_files: number; total_files: number; current: string };
type Result = { path: string; files: number; links: number; missing: string[]; cancelled: boolean };
const bytes = (value: number) => value >= 1e9 ? `${(value / 1e9).toFixed(2)} GB` : `${(value / 1e6).toFixed(1)} MB`;

export default function LibraryExport({ close }: { close: () => void }) {
  const [busy, setBusy] = useState(false), [choosing, setChoosing] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null), [result, setResult] = useState<Result | null>(null);
  const [error, setError] = useState(""), [cancelling, setCancelling] = useState(false);
  const dialog = useRef<HTMLDivElement>(null), pending = useRef(false);
  useEffect(() => { dialog.current?.focus(); }, []);
  const start = async () => {
    if (pending.current) return;
    pending.current = true; setChoosing(true); setError("");
    try {
      const path = await open({ directory: true, multiple: false, title: "Choose where to save your library backup" });
      if (typeof path !== "string") return;
      setBusy(true); setResult(null); setProgress(null); setCancelling(false);
      const onProgress = new Channel<Progress>(); onProgress.onmessage = setProgress;
      const today = new Date();
      const name = `Design Backup ${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`;
      setResult(await invoke<Result>("export_library", { path, name, onProgress }));
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); setChoosing(false); pending.current = false; }
  };
  const cancel = async () => {
    setCancelling(true);
    try { await invoke("cancel_library_export"); }
    catch (e) { setError(String(e)); setCancelling(false); }
  };
  const percent = progress?.total_bytes ? Math.min(100, progress.copied_bytes / progress.total_bytes * 100) : 0;
  return <div className="modal-backdrop">
    <div className="modal library-export" role="dialog" aria-modal="true" aria-labelledby="export-title" tabIndex={-1} ref={dialog}
      onKeyDown={e => {
        if (e.key === "Escape") { e.stopPropagation(); if (!busy && !choosing) close(); }
        if (e.key === "Tab") {
          const controls = [...(dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled),summary') ?? [])];
          const first = controls[0], last = controls.at(-1);
          if (e.shiftKey && (document.activeElement === first || document.activeElement === dialog.current)) { e.preventDefault(); last?.focus(); }
          else if (!e.shiftKey && (document.activeElement === last || document.activeElement === dialog.current)) { e.preventDefault(); first?.focus(); }
        }
      }}>
      <div className="modal-heading"><h2 id="export-title">Export full library</h2><button className="icon-button" aria-label="Close library export" disabled={busy || choosing} onClick={close}><X size={17}/></button></div>
      <p className="export-description">Save your images, videos, links and organisation in one portable folder. Choose a folder on your PC or an external drive.</p>
      <small>Includes original media, link thumbnails, projects, tags, categories and pins. Removed references are excluded.</small>
      {busy && <div className="export-progress" role="status" aria-live="polite">
        <div><span><LoaderCircle size={14} className="spin"/> {cancelling ? "Cancelling…" : progress?.phase === "finishing" ? "Finishing backup…" : progress?.phase === "copying" ? "Copying files…" : "Preparing library…"}</span><strong>{Math.round(percent)}%</strong></div>
        <progress aria-label="Library export progress" max={100} value={progress?.phase === "preparing" || !progress ? undefined : percent}/>
        {progress && <><small>{bytes(progress.copied_bytes)} of {bytes(progress.total_bytes)} · {progress.completed_files} of {progress.total_files} files</small><p className="export-current" title={progress.current}>{progress.current}</p></>}
      </div>}
      {result && <div className="export-result" role="status">
        <strong>{result.cancelled ? "Export cancelled" : "Backup ready"}</strong>
        <p>{result.cancelled ? "The partial folder is marked incomplete. Your originals are unchanged." : `${result.files} files and ${result.links} links exported. You can copy this entire folder to an external drive.`}</p>
        <code>{result.path}</code>
        {!!result.missing.length && <details><summary>{result.missing.length} unavailable files — review before relying on this backup</summary><ul>{result.missing.map((value, index) => <li key={index}>{value}</li>)}</ul></details>}
        <button className="text-button" onClick={() => void revealItemInDir(result.path).catch(e => setError(String(e)))}><FolderOpen size={14}/> Show backup folder</button>
      </div>}
      {error && <div className="form-error" role="alert">{error}</div>}
      <div className="modal-footer">{busy ? <button className="subtle-button" disabled={cancelling} onClick={() => void cancel()}>Cancel export</button> : <button className="primary" disabled={choosing} onClick={() => void start()}><Download size={14}/> {choosing ? "Choosing folder…" : result ? "Export again" : "Choose backup folder"}</button>}</div>
    </div>
  </div>;
}
