import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { Check, FileImage, LoaderCircle, X } from "lucide-react";
import { boardNames, isBoardDestination, isTableCollection, type Library, type LinkDestination } from "./model";
import { RatioSelect, ThumbnailCrop } from "./ThumbnailEditor";

export type CaptureFile = { path: string; name: string };
export type CaptureDraft = { files: CaptureFile[]; url: string; thumbnail?: CaptureFile; destination?: LinkDestination };
export type CaptureResult = { ids: number[]; added: number; reused: number; destination?: LinkDestination | null };
export async function stageFiles(files: File[]): Promise<CaptureDraft> {
  const staged: CaptureFile[] = [];
  try {
    for (const file of files) {
      const entry = await invoke<CaptureFile>("stage_begin", {
        name: file.name,
      });
      staged.push(entry);
      const token = entry.path.replace(/\\/g, "/").split("/").pop()!;
      for (let offset = 0; offset < file.size; offset += 2 * 1024 * 1024) {
        const bytes = new Uint8Array(
          await file.slice(offset, offset + 2 * 1024 * 1024).arrayBuffer(),
        );
        await invoke("stage_chunk", bytes, {
          headers: { "x-capture-token": token },
        });
      }
    }
    return { files: staged, url: "" };
  } catch (error) {
    await invoke("discard_capture", { paths: staged.map((f) => f.path) }).catch(
      () => {},
    );
    throw error;
  }
}
export default function CaptureDialog({
  draft,
  lib,
  project,
  source,
  close,
  complete,
}: {
  draft: CaptureDraft;
  lib: Library;
  project: number | null;
  source: string;
  close: () => void;
  complete: (result: CaptureResult) => Promise<void>;
}) {
  const [name, setName] = useState(
      draft.files.length === 1
        ? draft.files[0].name.replace(/\.[^.]+$/, "")
        : "",
    ),
    [url, setUrl] = useState(draft.url),
    [sourceUrl, setSourceUrl] = useState("");
  const [projects, setProjects] = useState<number[]>(project ? [project] : []),
    [linkDestination, setLinkDestination] = useState(draft.destination ?? "links"),
    [destination, setDestination] = useState(["Photography", "Videos"].includes(source.split("/")[0]) ? source.split("/")[0] : "Design"),
    [tags, setTags] = useState<number[]>([]),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const board = isBoardDestination(linkDestination);
  const [thumbnail, setThumbnail] = useState<CaptureFile | undefined>(draft.thumbnail);
  const [reading, setReading] = useState(false);
  const [ratio, setRatio] = useState(4/3);
  const [adjusting, setAdjusting] = useState(false);
  const pending = useRef(false);
  const saveAfterPaste = useRef(false);
  const form = useRef<HTMLFormElement>(null);
  useEffect(() => {
    if (!reading && saveAfterPaste.current && (thumbnail || isTableCollection(linkDestination)) && url) {
      saveAfterPaste.current = false; form.current?.requestSubmit();
    }
  }, [reading, thumbnail, url, linkDestination]);
  const staged = useRef<string[]>(draft.thumbnail ? [draft.thumbnail.path] : []);
  const cleanup = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => { clearTimeout(cleanup.current); return () => { cleanup.current = setTimeout(() => { void invoke("discard_capture", { paths: staged.current }).catch(() => {}); }, 0); }; }, []);
  const receive = (incoming: CaptureDraft) => {
    if (incoming.files.length) {
      const file = incoming.files[0];
      staged.current.push(...incoming.files.map(file => file.path));
      setThumbnail(file);
    }
    if (incoming.url) setUrl(incoming.url);
  };
  const paste = async () => {
    if (pending.current || busy) return;
    pending.current = true; setReading(true); setError("");
    try { receive(await invoke<CaptureDraft>("read_clipboard")); }
    catch (error) { saveAfterPaste.current = false; setError(String(error)); }
    finally { pending.current = false; setReading(false); }
  };
  const toggle = (ids: number[], id: number) =>
    ids.includes(id) ? ids.filter((n) => n !== id) : [...ids, id];
  const save = async (image = thumbnail) => {
    if (busy || pending.current) return;
    if (board && !isTableCollection(linkDestination) && !image) { setError("Capture or paste a thumbnail first."); return; }
    setBusy(true);
    setError("");
    try {
      const result = await invoke<CaptureResult>("capture_items", {
        input: {
          files: draft.files,
          thumbnail: image,
          destination: linkDestination,
          url,
          name,
          source_url: sourceUrl,
          projects,
          tags,
          source: destination,
          resource: draft.files.length === 0 && linkDestination === "resources",
        },
      });
      await complete(result);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };
  const submit = async (e: React.FormEvent) => { e.preventDefault(); await save(); };
  const captureScreen = async () => {
    if (busy || pending.current || !form.current?.reportValidity()) return;
    pending.current = true; setReading(true); setError("");
    let file: CaptureFile | null = null;
    try {
      file = await invoke<CaptureFile | null>("capture_screen", {ratio});
      if(file) { staged.current.push(file.path); setThumbnail(file); }
    } catch(e) { setError(String(e)); }
    finally { pending.current = false; setReading(false); }
    if(file) await save(file);
  };
  return (
    <><div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !busy && !reading) close();
      }}
    >
      <div
        className="modal capture-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Capture"
        onKeyDown={e=>{
          if(board && e.key==='Enter' && pending.current) { e.preventDefault(); e.stopPropagation(); saveAfterPaste.current=true; return; }
          if(board && e.key==='Enter' && ((e.target instanceof HTMLInputElement) || (e.target as HTMLElement).closest('.capture-assign'))) { e.preventDefault(); e.stopPropagation(); form.current?.requestSubmit(); return; }
          if(e.key==='Escape'&&(busy||reading)){e.preventDefault();e.stopPropagation();}
          if(board&&(e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='v') {e.preventDefault();e.stopPropagation();void paste();}
        }}
        onPaste={e=>{ if(board&&e.clipboardData.files.length){e.preventDefault(); if(pending.current||busy)return; pending.current=true;setReading(true);void stageFiles(Array.from(e.clipboardData.files)).then(receive).catch(e=>setError(String(e))).finally(()=>{pending.current=false;setReading(false);});} }}
      >
        <div className="modal-heading">
          <h2>{isBoardDestination(linkDestination) ? boardNames[linkDestination] : "Capture"}</h2>
          <button
            className="icon-button"
            aria-label="Close capture"
            disabled={busy || reading}
            onClick={close}
          >
            <X size={17} />
          </button>
        </div>
        <form ref={form} onSubmit={submit}>
          {draft.files.length === 0 && <label className="field">Save to
            <select aria-label="Save link to" value={linkDestination} onChange={(e) => setLinkDestination(e.target.value as LinkDestination)}>
              <option value="links">Links</option><option value="resources">Resources</option>{Object.entries(boardNames).map(([key,name])=><option key={key} value={key}>{name}</option>)}
            </select>
          </label>}
          {draft.files.length > 0 ? (
            <div className="capture-files">
              {draft.files.map((file, n) => (
                <div key={`${file.path}-${n}`}>
                  <FileImage size={14} />
                  <span>{file.name}</span>
                </div>
              ))}
            </div>
          ) : (
            <label className="field">
              Link
              <input
                autoFocus
                type="url"
                required
                placeholder="https://"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
              />
            </label>
          )}
          {draft.files.length === 0 && !isTableCollection(linkDestination) && <div className="thumbnail-capture">
            {thumbnail && <img src={convertFileSrc(thumbnail.path)} alt="Thumbnail" />}
            <RatioSelect value={ratio} change={setRatio} disabled={busy||reading}/>
            <button type="button" className="primary" disabled={busy || reading || !url.trim()} onClick={() => void captureScreen()}>{reading ? "Working…" : "Capture & save"}</button>
            <button type="button" className="subtle-button" disabled={busy || reading} onClick={() => void paste()}>{reading ? "Reading…" : thumbnail ? "Replace thumbnail" : "Paste thumbnail"}</button>
            {thumbnail&&<button type="button" className="subtle-button" disabled={busy||reading} onClick={()=>setAdjusting(true)}>Adjust thumbnail</button>}
          </div>}
          {draft.files.length > 0 && <label className="field">Save files to
            <select value={destination} onChange={(e) => setDestination(e.target.value)}>
              {["Design", "Photography", "Videos"].map((name) => <option key={name}>{name}</option>)}
            </select>
          </label>}
          {draft.files.length < 2 && (
            <label className="field">
              Name
              <input
                autoFocus={draft.files.length > 0}
                value={name}
                maxLength={300}
                onChange={(e) => setName(e.target.value)}
                placeholder="Name"
              />
            </label>
          )}
          {draft.files.length > 0 && (
            <label className="field">
              Source link <span className="optional">optional</span>
              <input
                type="url"
                value={sourceUrl}
                onChange={(e) => setSourceUrl(e.target.value)}
                placeholder="https://"
              />
            </label>
          )}
          <div className="capture-assign">
            <span>Projects</span>
            <div>
              {lib.projects.length ? (
                lib.projects.map((p) => (
                  <button
                    type="button"
                    key={p.id}
                    className={projects.includes(p.id) ? "chosen" : ""}
                    aria-pressed={projects.includes(p.id)}
                    onClick={() => setProjects(toggle(projects, p.id))}
                  >
                    {projects.includes(p.id) && <Check size={11} />} {p.name}
                  </button>
                ))
              ) : (
                <small>No projects yet</small>
              )}
            </div>
            <span>Tags</span>
            <div>
              {lib.tags.map((t) => (
                <button
                  type="button"
                  key={t.id}
                  className={tags.includes(t.id) ? "chosen" : ""}
                  aria-pressed={tags.includes(t.id)}
                  onClick={() => setTags(toggle(tags, t.id))}
                >
                  {tags.includes(t.id) && <Check size={11} />} {t.name}
                </button>
              ))}
            </div>
          </div>
          {error && (
            <div className="form-error" role="alert">
              {error}
            </div>
          )}
          <div className="modal-footer">
            <button
              type="button"
              className="subtle-button"
              disabled={busy || reading}
              onClick={close}
            >
              Cancel
            </button>
            <button type="submit" className="primary" disabled={busy || reading}>
              {busy ? (
                <>
                  <LoaderCircle size={14} className="spin" /> Checking & saving…
                </>
              ) : (
                "Save"
              )}
            </button>
          </div>
        </form>
      </div>
    </div>{adjusting&&thumbnail&&<ThumbnailCrop file={thumbnail} ratio={ratio} close={()=>setAdjusting(false)} save={async file=>{staged.current.push(file.path);setThumbnail(file);setAdjusting(false);}}/>}</>
  );
}
