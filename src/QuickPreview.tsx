import { useEffect, useRef, useState } from "react";
import { X, ExternalLink } from "lucide-react";
import { media, openUrl } from "./api";
import type { Item } from "./model";

export default function QuickPreview({item,close}:{item:Item;close:()=>void}) {
  const panel=useRef<HTMLElement>(null);
  const [url,setUrl]=useState("");
  const [failed,setFailed]=useState(false);
  useEffect(()=>{const previous=document.activeElement as HTMLElement|null;panel.current?.focus();return()=>previous?.focus({preventScroll:true});},[]);
  useEffect(()=>{
    const abort=new AbortController();setUrl("");setFailed(false);
    if(item.kind==="link"&&!item.thumbnail)return;
    void media(item,item.kind==="link"?"thumbnail":item.kind==="video"?"preview":"original",abort.signal).then(value=>{if(!abort.signal.aborted)setUrl(value);}).catch(()=>{if(!abort.signal.aborted)setFailed(true);});
    return()=>abort.abort();
  },[item]);
  return <div className="quick-preview-backdrop" onClick={close}>
    <section className="quick-preview" role="dialog" aria-modal="true" aria-label="Quick preview" tabIndex={-1} ref={panel} onClick={e=>e.stopPropagation()} onKeyDown={e=>{
      if(e.key!=="Tab")return;
      const controls=Array.from(panel.current?.querySelectorAll<HTMLElement>('button,a[href],video[controls]')??[]);
      const first=controls[0],last=controls.at(-1);
      if(e.shiftKey&&(document.activeElement===first||document.activeElement===panel.current)){e.preventDefault();last?.focus();}
      else if(!e.shiftKey&&document.activeElement===last){e.preventDefault();first?.focus();}
    }}>
      <header><strong>{item.name}</strong><button className="icon-button" aria-label="Close quick preview" onClick={close}><X size={18}/></button></header>
      <div className="quick-preview-media">
        {url&&!failed ? item.kind==="video"?<video src={url} autoPlay muted loop controls onError={()=>setFailed(true)}/>:<img src={url} alt={item.name} onError={()=>setFailed(true)}/> : <span>{item.kind==="link"?new URL(item.path).hostname:failed?"Preview unavailable":"Loading preview…"}</span>}
      </div>
      {item.kind==="link"&&<div className="quick-preview-link"><span>{item.path}</span><button onClick={()=>void openUrl(item.path).catch(()=>setFailed(true))}>Open website <ExternalLink size={14}/></button></div>}
      <footer>Space or Esc to close · ← → browse{item.kind==="video"?" · Muted preview":""}</footer>
    </section>
  </div>;
}
