import { useEffect, useRef, useState } from "react";
import { Check, LoaderCircle, Plus, RotateCcw, X } from "lucide-react";
import type useVideoQueue from "./useVideoQueue";

const size=(n:number)=>n>=1048576?`${(n/1048576).toFixed(1)} MB`:`${Math.ceil(n/1024)} KB`;
const time=(n:number)=>n<60?`${Math.ceil(n)}s`:`${Math.floor(n/60)}m ${Math.ceil(n%60)}s`;
export default function VideoDownloadQueue({queue,project,close}: {
  queue: ReturnType<typeof useVideoQueue>; project: number|null; close:()=>void;
}) {
  const [text,setText]=useState(''), [error,setError]=useState(''), [now,setNow]=useState(Date.now());
  const input=useRef<HTMLTextAreaElement>(null);
  useEffect(()=>{input.current?.focus();},[]);
  useEffect(()=>{if(!queue.jobs.some(job=>job.status==='active'))return; const timer=setInterval(()=>setNow(Date.now()),1000);return()=>clearInterval(timer);},[queue.jobs.some(job=>job.status==='active')]);
  const submit=()=>{
    try{queue.add(text,project?[project]:[]);setText('');setError('');input.current?.focus();}
    catch(error){setError(String(error));}
  };
  const shown=[...queue.jobs.filter(job=>job.status==='active'),...queue.jobs.filter(job=>job.status==='queued'),...queue.jobs.filter(job=>job.status==='error'),...queue.jobs.filter(job=>job.status==='done'||job.status==='cancelled').slice(-20).reverse()];
  return <section className="download-queue" aria-label="Video download queue" onKeyDown={e=>{if(e.key==='Escape'){e.preventDefault();e.stopPropagation();close();}}}>
    <header><strong>Downloads</strong><small>{queue.jobs.filter(job=>job.status==='queued').length} waiting</small><button className="icon-button" aria-label="Hide downloads" onClick={close}><X size={15}/></button></header>
    <form className="download-queue-input" onSubmit={e=>{e.preventDefault();submit();}}>
      <textarea ref={input} rows={2} value={text} placeholder="Paste video links" aria-label="Queue video links" onChange={e=>setText(e.target.value)} onKeyDown={e=>{if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();submit();}}}/>
      <button type="submit" className="icon-button" aria-label="Add video links to queue"><Plus size={16}/></button>
    </form>
    {error&&<div className="form-error" role="alert">{error}</div>}
    <div className="download-queue-list">
      {shown.map((job)=>{
        const active=job.status==='active', p=job.progress;
        const label=job.cancelling?'Cancelling…':active ? p?.phase==='downloading'?'Downloading…':p?.phase==='processing'?'Preparing video…':p?.phase==='saving'?'Adding to Videos…':'Connecting…' : job.status==='queued'?'Waiting':job.status==='done'?(job.reused?'Already saved':'Saved'):job.status==='cancelled'?'Cancelled':'Failed';
        return <div className={`download-queue-item ${job.status}`} key={job.id} data-job-id={job.id}>
          <div className="download-queue-row"><span title={job.url}>{job.url.replace(/^https?:\/\//,'')}</span>
            {(active||job.status==='queued')&&<button className="icon-button" disabled={job.cancelling} aria-label={active?'Cancel download':'Remove queued video'} onClick={()=>queue.cancel(job.id)}><X size={13}/></button>}
            {(job.status==='error'||job.status==='cancelled')&&<button className="icon-button" aria-label="Retry video" onClick={()=>queue.retry(job.id)}><RotateCcw size={13}/></button>}
            {job.status==='done'&&<Check size={13}/>}
          </div>
          <div className="download-queue-row"><small>{active&&<LoaderCircle size={11} className="spin"/>}{label}</small><small>{active&&job.started?`${time(Math.max(0,(now-job.started)/1000))} elapsed`:''}</small></div>
          {active&&<div className="video-download-progress" role="status">
            <progress aria-label="Video download progress" max="100" value={p?.phase==='downloading'&&p.percent!=null?p.percent:undefined}/>
            <div><small>{p?.phase==='downloading'?`${p.percent==null?'':`${Math.floor(p.percent)}% · `}${size(p.downloaded)}${p.total?` / ${size(p.total)}`:''}${p.speed?` · ${size(p.speed)}/s`:''}`:''}</small><small>{p?.phase==='downloading'&&p.eta!=null?`${time(p.eta)} remaining`:''}</small></div>
          </div>}
          {job.error&&<div className="download-queue-error" role="alert">{job.error}</div>}
        </div>;
      })}
      {!queue.jobs.length&&<div className="modal-empty">No downloads</div>}
    </div>
    {queue.jobs.some(job=>!['active','queued'].includes(job.status))&&<footer><button onClick={queue.clear}>Clear finished</button></footer>}
  </section>;
}
