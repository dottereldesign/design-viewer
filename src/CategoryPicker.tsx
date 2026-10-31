import { useState } from "react";
import { Folder, X } from "lucide-react";
import { categoryLabel } from "./model";

export default function CategoryPicker({categories,count,choose,close}: {categories:string[];count:number;choose:(name:string)=>Promise<void>;close:()=>void}) {
  const [query,setQuery]=useState(""),[busy,setBusy]=useState(false),[error,setError]=useState("");
  const commit=async(name:string)=>{if(busy)return;setBusy(true);setError("");try{await choose(name);}catch(e){setError(String(e));setBusy(false);}};
  const shown=categories.filter(c=>categoryLabel(c).toLocaleLowerCase().includes(query.trim().replace(/_/g," ").toLocaleLowerCase()));
  const isNew=!!query.trim()&&!categories.some(c=>categoryLabel(c).toLocaleLowerCase()===categoryLabel(query).toLocaleLowerCase());
  return <div className="modal-backdrop" onMouseDown={e=>{if(e.target===e.currentTarget&&!busy)close();}}><div className="modal category-picker" role="dialog" aria-modal="true" aria-label={count?"Change category":"New category"} onKeyDown={e=>{e.stopPropagation();if(e.key==='Escape'&&!busy)close();}}>
    <div className="modal-heading"><h2>{count?`Change category · ${count}`:"New category"}</h2><button className="icon-button" aria-label="Close category picker" disabled={busy} onClick={close}><X size={17}/></button></div>
    <form onSubmit={e=>{e.preventDefault();const name=isNew?query.trim():count?shown[0]:undefined;if(name)void commit(name);}}>
      <input autoFocus aria-label="Find or create category" placeholder={count?"Find or create a category…":"Category name"} maxLength={100} value={query} disabled={busy} onChange={e=>setQuery(e.target.value)}/>
      {count>0&&<div className="category-choices">{shown.map(name=><button type="button" disabled={busy} key={name} onClick={()=>void commit(name)}><Folder size={14}/>{categoryLabel(name)}</button>)}<button type="button" disabled={busy} onClick={()=>void commit("")}>Uncategorised</button></div>}
      {isNew&&<button className="primary" disabled={busy}>{count?`Create “${query.trim()}” & move`:`Create “${query.trim()}”`}</button>}
      {error&&<div className="form-error" role="alert">{error}</div>}
    </form>
  </div></div>;
}
