export type VideoRequest = { url: string; projects: number[]; tags: number[] };
export type VideoProgress = { phase: "connecting" | "downloading" | "processing" | "saving"; percent: number | null; downloaded: number; total: number | null; speed: number | null; eta: number | null };
export type VideoJob = VideoRequest & {
  id: string; key: string; status: "queued" | "active" | "done" | "error" | "cancelled";
  progress?: VideoProgress; started?: number; error?: string; cancelling?: boolean; reused?: boolean;
};
export function videoKey(input: string) {
  let url: URL;
  try { url = new URL(input.trim()); } catch { throw Error("Enter a complete http:// or https:// video link."); }
  if (!['http:', 'https:'].includes(url.protocol) || !url.hostname || url.username || url.password || /\s/.test(input.trim())) throw Error("Enter a complete http:// or https:// video link.");
  url.hash = '';
  if (['facebook.com','www.facebook.com','m.facebook.com'].includes(url.hostname)) {
    const id = url.searchParams.get('v');
    if (id && /^\d+$/.test(id)) return `https://www.facebook.com/watch/?v=${id}`;
  }
  return url.toString();
}
export function videoRequests(text: string, projects: number[], tags: number[]): VideoRequest[] {
  const lines = text.split(/\r?\n/).map(line=>line.trim()).filter(Boolean);
  if (!lines.length) throw Error("Paste a video link first.");
  return lines.map(url=>({url:videoKey(url),projects:[...new Set(projects)],tags:[...new Set(tags)]}));
}
export function enqueueVideos(current: VideoJob[], requests: VideoRequest[], id: () => string) {
  const jobs = [...current]; let added = 0, duplicates = 0;
  for (const request of requests) {
    const key = videoKey(request.url);
    const queued = jobs.find(job=>job.key===key && job.status==='queued');
    if (queued) {
      const index=jobs.indexOf(queued);
      jobs[index]={...queued, projects:[...new Set([...queued.projects,...request.projects])],tags:[...new Set([...queued.tags,...request.tags])]};
      duplicates++; continue;
    }
    const active=jobs.find(job=>job.key===key && job.status==='active');
    if(active && request.projects.every(label=>active.projects.includes(label)) && request.tags.every(label=>active.tags.includes(label))) { duplicates++;continue; }
    jobs.push({...request,key,id:id(),status:'queued'});added++;
  }
  return { jobs, added, duplicates };
}
export function restoreVideoQueue(text: string | null): VideoJob[] {
  if (!text) return [];
  try {
    const data: unknown = JSON.parse(text);
    if (!Array.isArray(data)) return [];
    return data.flatMap((job): VideoJob[] => {
      if (!job || typeof job.id!=='string' || typeof job.url!=='string' || !Array.isArray(job.projects) || !Array.isArray(job.tags) || !['queued','active','error','done','cancelled'].includes(job.status)) return [];
      try {
        return [{id:job.id,url:videoKey(job.url),key:videoKey(job.url),projects:job.projects.filter(Number.isSafeInteger),tags:job.tags.filter(Number.isSafeInteger),status:job.status==='active'?'queued':job.status,error:typeof job.error==='string'?job.error:undefined,reused:!!job.reused}];
      } catch { return []; }
    });
  } catch { return []; }
}
export function persistVideoQueue(jobs: VideoJob[]) {
  const finished=jobs.filter(job=>['done','cancelled'].includes(job.status)).slice(-20);
  return JSON.stringify(jobs.filter(job=>!['done','cancelled'].includes(job.status)||finished.includes(job)).map(({progress,started,cancelling,...job})=>({...job,status:cancelling?'cancelled':job.status})));
}
