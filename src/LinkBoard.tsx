import { convertFileSrc } from "@tauri-apps/api/core";
import { MoreHorizontal, Plus } from "lucide-react";
import { openUrl } from "./api";
import type { Item } from "./model";

export default function LinkBoard({ items, size, report, menu, add, selected, openItem }: {
  items: Item[]; size: number; report: (error: unknown) => void;
  menu: (id: number, event: React.MouseEvent<HTMLButtonElement>) => void;
  add?: () => void;
  selected?: Set<number>;
  openItem?: (item: Item, event: React.MouseEvent) => void;
}) {
  return <div className="link-board" style={{ gridTemplateColumns: `repeat(auto-fill, minmax(min(${size}px, 100%), 1fr))` }}>
    {items.map(item => <article className={`link-card ${selected?.has(item.id) ? "selected" : ""}`} key={item.id} data-item-id={item.id}>
      <button className="link-card-open" onClick={event => openItem ? openItem(item, event) : void openUrl(item.path).catch(report)} aria-label={`Open ${item.name}`}>
        {item.thumbnail ? <img loading="lazy" src={convertFileSrc(item.thumbnail)} alt="" /> : <div className="link-card-placeholder" />}
        <strong>{item.name}</strong><small>{new URL(item.path).hostname}</small>
      </button>
      <button className="icon-button link-card-menu" aria-label={`Actions for ${item.name}`} onClick={event => menu(item.id,event)}><MoreHorizontal size={16}/></button>
    </article>)}
    {!items.length && add && <div className="empty-state"><button className="subtle-button" onClick={add}><Plus size={14}/> Add link</button><small>Ctrl+V</small></div>}
  </div>;
}
