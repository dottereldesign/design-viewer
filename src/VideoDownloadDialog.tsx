import { useState } from "react";
import { Check, Plus, X } from "lucide-react";
import type { Library } from "./model";

export default function VideoDownloadDialog({ initialUrl, lib, project, close, add }: {
  initialUrl: string; lib: Library; project: number | null; close: () => void;
  add: (text: string, projects: number[], tags: number[]) => void;
}) {
  const [url,setUrl]=useState(initialUrl), [projects,setProjects]=useState<number[]>(project ? [project] : []), [tags,setTags]=useState<number[]>([]), [error,setError]=useState("");
  const toggle=(ids:number[],id:number)=>ids.includes(id)?ids.filter(n=>n!==id):[...ids,id];
  return <div className="modal-backdrop" onMouseDown={e=>{if(e.target===e.currentTarget)close();}}>
    <div className="modal capture-modal video-download-modal" role="dialog" aria-modal="true" aria-label="Add videos" onKeyDown={e=>{
      if(e.key==="Escape"){e.preventDefault();e.stopPropagation();close();}
      if(e.key==="Enter"&&!e.shiftKey&&((e.target instanceof HTMLTextAreaElement)||(e.target as HTMLElement).closest('.capture-assign'))){e.preventDefault();e.currentTarget.querySelector('form')?.requestSubmit();}
    }}>
      <div className="modal-heading"><h2>Add videos</h2><button className="icon-button" aria-label="Close video download" onClick={close}><X size={17}/></button></div>
      <form onSubmit={e=>{e.preventDefault();try{add(url,projects,tags);}catch(error){setError(String(error));}}}>
        <label className="field">Links<textarea autoFocus required rows={3} value={url} onChange={e=>setUrl(e.target.value)} placeholder="One link per line" aria-label="Video links"/></label>
        <div className="capture-assign">
          <span>Projects</span><div>{lib.projects.map(p=><button type="button" key={p.id} className={projects.includes(p.id)?"chosen":""} aria-pressed={projects.includes(p.id)} onClick={()=>setProjects(toggle(projects,p.id))}>{projects.includes(p.id)&&<Check size={11}/>} {p.name}</button>)}</div>
          <span>Tags</span><div>{lib.tags.map(t=><button type="button" key={t.id} className={tags.includes(t.id)?"chosen":""} aria-pressed={tags.includes(t.id)} onClick={()=>setTags(toggle(tags,t.id))}>{tags.includes(t.id)&&<Check size={11}/>} {t.name}</button>)}</div>
        </div>
        {error&&<div className="form-error" role="alert">{error}</div>}
        <div className="modal-footer"><button type="button" className="subtle-button" onClick={close}>Cancel</button><button className="primary"><Plus size={14}/>Add to queue</button></div>
      </form>
    </div>
  </div>;
}
