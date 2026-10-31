import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RotateCcw, X } from "lucide-react";

type RemovedItem = { id: number; name: string; path: string };
export default function RemovedDialog({ close, restore }: { close: () => void; restore: (ids: number[]) => Promise<unknown> }) {
  const [items, setItems] = useState<RemovedItem[] | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const reload = () => invoke<RemovedItem[]>("removed_items").then(setItems);
  useEffect(() => { void reload().catch((e) => setError(String(e))); }, []);
  return <div className="modal-backdrop" onMouseDown={(e) => { if (e.target === e.currentTarget && !busy) close(); }}>
    <div className="modal removed-modal" role="dialog" aria-modal="true" aria-label="Removed items">
      <div className="modal-heading"><h2>Removed items</h2><button className="icon-button" aria-label="Close removed items" onClick={close}><X size={17} /></button></div>
      <div className="removed-items">
        {items?.map((item) => <div key={item.id} className="removed-item"><span><strong>{item.name}</strong><small title={item.path}>{item.path}</small></span><button disabled={busy} onClick={async () => {
          setBusy(true); setError("");
          try { await restore([item.id]); await reload(); } catch (e) { setError(String(e)); } finally { setBusy(false); }
        }}><RotateCcw size={13} /> Restore</button></div>)}
        {items && !items.length && <div className="modal-empty">No removed items</div>}
        {!items && !error && <div className="modal-empty">Loading…</div>}
      </div>
      {error && <div className="form-error" role="alert">{error}</div>}
    </div>
  </div>;
}
