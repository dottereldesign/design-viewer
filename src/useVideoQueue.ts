import { useCallback, useEffect, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import { enqueueVideos, persistVideoQueue, restoreVideoQueue, videoRequests, type VideoJob, type VideoProgress } from "./videoQueue";

const storageKey = 'video-download-queue-v1';
export default function useVideoQueue(enabled: boolean, saved: () => Promise<unknown>, report: (error:unknown)=>void) {
  const [jobs,setJobs]=useState<VideoJob[]>(()=>{try{return restoreVideoQueue(localStorage.getItem(storageKey));}catch{return [];}});
  const jobsRef=useRef(jobs), running=useRef<string|null>(null), persisted=useRef('');
  const savedRef=useRef(saved), reportRef=useRef(report);
  savedRef.current=saved;reportRef.current=report;
  const change=useCallback((update:(jobs:VideoJob[])=>VideoJob[])=>{
    const next=update(jobsRef.current);jobsRef.current=next;setJobs(next);
  },[]);
  useEffect(()=>{
    const text=persistVideoQueue(jobs);
    if(text===persisted.current)return;
    try{localStorage.setItem(storageKey,text);persisted.current=text;}catch{/* Downloads still work if local storage is unavailable. */}
  },[jobs]);
  useEffect(()=>{
    if(!enabled || running.current)return;
    const next=jobsRef.current.find(job=>job.status==='queued');if(!next)return;
    running.current=next.id;
    change(jobs=>jobs.map(job=>job.id===next.id?{...job,status:'active',started:Date.now(),error:undefined,progress:undefined,cancelling:false}:job));
    const onProgress=new Channel<VideoProgress>();
    onProgress.onmessage=progress=>change(jobs=>jobs.map(job=>job.id===next.id&&job.status==='active'?{...job,progress}:job));
    void (async()=>{
      try {
        const result=await invoke<{id:number;reused:boolean}>("download_video",{input:{url:next.url,projects:next.projects,tags:next.tags},onProgress});
        change(jobs=>jobs.map(job=>job.id===next.id?{...job,status:'done',reused:result.reused,cancelling:false,progress:undefined}:job));
        try{await savedRef.current();}catch(error){reportRef.current(error);}
      }catch(error){
        change(jobs=>jobs.map(job=>job.id===next.id?{...job,status:job.cancelling?'cancelled':'error',error:job.cancelling?undefined:String(error),progress:undefined,cancelling:false}:job));
      }finally{
        running.current=null;
        // A new state reference wakes the FIFO worker even after an immediate error.
        change(jobs=>[...jobs]);
      }
    })();
  },[enabled,jobs,change]);
  const add=useCallback((text:string,projects:number[]=[],tags:number[]=[])=>{
    const requests=videoRequests(text,projects,tags);
    const result=enqueueVideos(jobsRef.current,requests,()=>crypto.randomUUID());
    change(()=>result.jobs);return result;
  },[change]);
  const cancel=useCallback((id:string)=>{
    const job=jobsRef.current.find(job=>job.id===id);if(!job || job.cancelling)return;
    if(job.status==='queued'){change(jobs=>jobs.map(job=>job.id===id?{...job,status:'cancelled'}:job));return;}
    if(job.status!=='active')return;
    change(jobs=>jobs.map(job=>job.id===id?{...job,cancelling:true,error:undefined}:job));
    void invoke("cancel_video_download").catch(error=>{
      change(jobs=>jobs.map(job=>job.id===id&&job.status==='active'?{...job,cancelling:false,error:String(error)}:job));
    });
  },[change]);
  const retry=useCallback((id:string)=>change(jobs=>{
    const job=jobs.find(job=>job.id===id && (job.status==='error'||job.status==='cancelled'));
    return job?[...jobs.filter(job=>job.id!==id),{...job,status:'queued',progress:undefined,error:undefined,cancelling:false}]:jobs;
  }),[change]);
  const clear=useCallback(()=>change(jobs=>jobs.filter(job=>job.status==='active'||job.status==='queued')),[change]);
  return {jobs,add,cancel,retry,clear,pending:jobs.filter(job=>job.status==='active'||job.status==='queued').length};
}
