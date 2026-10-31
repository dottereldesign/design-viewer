import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Check, ClipboardCopy, ExternalLink, Tag, Trash2 } from "lucide-react";
import { canCopyImage, isResource, isTableLink, orderedProjects, type Item, type Library } from "./model";

export type ItemMenuTarget = { x: number; y: number; ids: number[] };
export default function ItemMenu({ target, lib, close, add, tags, reveal, remove, copy, copying, moveLink, editLink, editThumbnail, category }: {
  target: ItemMenuTarget; lib: Library; close: () => void;
  add: (project: number) => void; tags: () => void;
  reveal: (item: Item) => void; remove: () => void;
  copy: (id: number) => void; copying: boolean;
  moveLink: (item: Item) => void;
  editLink: (item: Item) => void;
  editThumbnail: (item: Item) => void;
  category: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: target.x, top: target.y });
  const items = lib.items.filter((item) => target.ids.includes(item.id));
  const projects = orderedProjects(lib.projects, lib.recent_projects);
  useLayoutEffect(() => {
    const rect = ref.current!.getBoundingClientRect();
    setPosition({ left: Math.max(8, Math.min(target.x, window.innerWidth - rect.width - 8)), top: Math.max(8, Math.min(target.y, window.innerHeight - rect.height - 8)) });
    ref.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [target]);
  useEffect(() => {
    const outside = (e: PointerEvent) => { if (!ref.current?.contains(e.target as Node)) close(); };
    const dismiss = () => close();
    document.addEventListener("pointerdown", outside, true);
    window.addEventListener("blur", dismiss);
    window.addEventListener("resize", dismiss);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("blur", dismiss);
      window.removeEventListener("resize", dismiss);
    };
  }, [close]);
  return createPortal(<div ref={ref} className="item-context-menu" style={position} role="menu" aria-label="Reference actions"
    onContextMenu={(e) => e.preventDefault()}
    onKeyDown={(e) => {
      if (e.key === "Escape" || e.key === "Tab") { e.preventDefault(); e.stopPropagation(); close(); }
      if (["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) {
        e.preventDefault(); e.stopPropagation();
        const buttons = [...ref.current!.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
        const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
        buttons[e.key === "Home" ? 0 : e.key === "End" ? buttons.length - 1 : (index + (e.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length]?.focus();
      }
    }}>
    {items.length === 1 && canCopyImage(items[0]) && <>
      <button role="menuitem" disabled={copying} onClick={() => { close(); copy(items[0].id); }}><ClipboardCopy size={13} /> Copy image</button><hr />
    </>}
    {projects.length ? projects.map((project, index) => {
      const added = items.length > 0 && items.every((item) => item.projects.includes(project.id));
      const recent = lib.recent_projects?.includes(project.id);
      const previousRecent = index > 0 && lib.recent_projects?.includes(projects[index - 1].id);
      return <div key={project.id}>
        {index === 0 && recent && <div className="context-section">Recent projects</div>}
        {previousRecent && !recent && <hr />}
        <button role="menuitem" disabled={added} onClick={() => { close(); add(project.id); }}>
          <span>Add to {project.name}</span>{added && <Check size={13} />}
        </button>
      </div>;
    }) : <div className="context-section">No projects yet</div>}
    <hr />
    {items.some(i=>i.kind!=="link")&&<button role="menuitem" onClick={()=>{close();category();}}>Change category…</button>}
    <button role="menuitem" onClick={() => { close(); tags(); }}><Tag size={13} /> Tags</button>
    {items.length === 1 && items[0].kind === "link" && <button role="menuitem" onClick={() => { close(); editLink(items[0]); }}>Edit link</button>}
    {items.length === 1 && items[0].kind === "link" && !isTableLink(items[0]) && <button role="menuitem" onClick={() => { close(); editThumbnail(items[0]); }}>Edit thumbnail</button>}
    {items.length === 1 && items[0].kind === "link" && <button role="menuitem" onClick={() => { close(); moveLink(items[0]); }}>Move to {isResource(items[0]) ? "Links" : "Resources"}</button>}
    {items.length === 1 && <button role="menuitem" onClick={() => { close(); reveal(items[0]); }}><ExternalLink size={13} />{items[0].kind === "link" ? "Open website" : "Reveal in Explorer"}</button>}
    <button role="menuitem" className="danger" onClick={() => { close(); remove(); }}><Trash2 size={13} /> Remove from library</button>
  </div>, document.body);
}
