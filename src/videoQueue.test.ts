import { describe, it } from 'node:test';
import { deepStrictEqual as equal, throws } from 'node:assert';
import { enqueueVideos, persistVideoQueue, restoreVideoQueue, videoKey, videoRequests, type VideoJob } from './videoQueue';

describe('video queue',()=>{
  let counter=0;const id=()=>String(++counter);
  it('validates a whole batch before adding any links and normalizes Facebook tracking URLs',()=>{
    equal(videoKey('https://m.facebook.com/watch/?ref=other&v=123#test'),'https://www.facebook.com/watch/?v=123');
    throws(()=>videoRequests('https://example.com/good\nnot a link',[],[]));
    throws(()=>videoKey('file:///video.mp4'));throws(()=>videoKey('https://user:secret@example.com/video'));
    equal(videoRequests(' https://example.com/one\n\nhttps://example.com/two ',[7,7],[1]).map(r=>r.projects),[[7],[7]]);
  });
  it('keeps FIFO order, avoids duplicate downloads and merges metadata on waiting jobs',()=>{
    const first=enqueueVideos([],videoRequests('https://example.com/one\nhttps://example.com/two',[7],[1]),id);
    const next=enqueueVideos(first.jobs,videoRequests('https://example.com/one\nhttps://example.com/three',[8],[2]),id);
    equal(next.added,1);equal(next.duplicates,1);
    equal(next.jobs.map(j=>j.url),['https://example.com/one','https://example.com/two','https://example.com/three']);
    equal(next.jobs[0].projects,[7,8]);equal(next.jobs[0].tags,[1,2]);
    equal(first.jobs[0].projects,[7]);
  });
  it('skips an active duplicate but queues new assignments to apply after it finishes',()=>{
    const active={...enqueueVideos([],videoRequests('https://example.com/one',[7],[1]),id).jobs[0],status:'active' as const};
    equal(enqueueVideos([active],videoRequests(active.url,[7],[]),id).added,0);
    const followup=enqueueVideos([active],videoRequests(active.url,[8],[2]),id);
    equal(followup.jobs.length,2);equal(followup.jobs[1].projects,[8]);equal(followup.jobs[1].status,'queued');
  });
  it('restores interrupted work in order and retains failed jobs without auto retrying them',()=>{
    const jobs=enqueueVideos([],videoRequests('https://example.com/one\nhttps://example.com/two',[7],[1]),id).jobs;
    jobs[0]={...jobs[0],status:'active',cancelling:false,started:123,progress:{phase:'downloading',percent:40,downloaded:40,total:100,speed:10,eta:6}};
    const failed:VideoJob={...jobs[1],id:'failed',status:'error',error:'Private video'};
    const text=persistVideoQueue([...jobs,failed]);
    equal(text.includes('progress'),false);equal(text.includes('cancelling'),false);
    const restored=restoreVideoQueue(text);equal(restored.map(j=>j.status),['queued','queued','error']);equal(restored[0].projects,[7]);equal(restored[2].error,'Private video');
    equal(restoreVideoQueue(persistVideoQueue([{...jobs[0],cancelling:true}]))[0].status,'cancelled');
    equal(restoreVideoQueue('broken JSON'),[]);equal(restoreVideoQueue('{"status":"queued"}'),[]);
    equal(restoreVideoQueue(JSON.stringify([{...jobs[0],url:'javascript:bad()'}])),[]);
  });
});
