import { useEffect, useRef, useState, type DragEvent, type PointerEvent as ReactPointerEvent } from "react";
import { categoryLabel } from "./model";

type Feedback = { count: number; x: number; y: number; action: string };
type Options = { enabled:boolean; ids:(id:number)=>number[]; category:(ids:number[],name:string)=>void; project:(ids:number[],id:number)=>void; started:()=>void };
type PointerDrag = {id:number;ids:number[];startX:number;startY:number;x:number;y:number;active:boolean;host:HTMLElement};

export default function useItemDragFeedback(options:Options) {
  const latest=useRef(options);latest.current=options;
  const pointer=useRef<PointerDrag|null>(null);
  const suppressClick=useRef(false);
  const [feedback,setFeedback]=useState<Feedback|null>(null);
  const ghost=useRef<HTMLCanvasElement|null>(null);
  const count=useRef(0);
  useEffect(()=>{
    let scrollFrame=0,clickTimer=0;
    const clear=()=>{
      const session=pointer.current;pointer.current=null;
      cancelAnimationFrame(scrollFrame);
      if(session?.host.hasPointerCapture(session.id))session.host.releasePointerCapture(session.id);
      document.body.classList.remove('dragging-items');
      count.current=0;setFeedback(null);ghost.current?.remove();ghost.current=null;
      document.querySelectorAll('.project-row.drop-target,.source-item.drop-target').forEach(row=>row.classList.remove('drop-target'));
    };
    const show=(element:Element|null,x:number,y:number)=>{
      if(!count.current)return;
      const sidebar=element?.closest('.sidebar');
      const target=element?.closest<HTMLElement>('[data-category],[data-project-id]');
      const category=target?.dataset.category;
      const action=category!==undefined?`Move to ${categoryLabel(category)}`:target?.dataset.projectName?`Add to ${target.dataset.projectName}`:'references';
      // Keep the badge outside the sidebar so every destination stays readable.
      const left=sidebar?sidebar.getBoundingClientRect().right+12:x+18;
      setFeedback({count:count.current,x:Math.max(8,Math.min(left,window.innerWidth-272)),y:Math.max(8,Math.min(y+18,window.innerHeight-48)),action});
    };
    const over=(e:globalThis.DragEvent)=>{
      show(e.target instanceof Element?e.target:null,e.clientX,e.clientY);
    };
    const highlight=(session:PointerDrag)=>{
      const element=document.elementFromPoint(session.x,session.y);
      const target=element?.closest('[data-category],[data-project-id]');
      document.querySelectorAll('.source-item.drop-target,.project-row.drop-target').forEach(row=>{if(row!==target)row.classList.remove('drop-target');});
      target?.classList.add('drop-target');show(element,session.x,session.y);
    };
    const scroll=()=>{
      const session=pointer.current;if(!session?.active)return;
      const list=document.querySelector<HTMLElement>('.source-list');
      if(list){const rect=list.getBoundingClientRect();if(session.x>=rect.left&&session.x<=rect.right&&session.y>=rect.top&&session.y<=rect.bottom){
        const speed=session.y<rect.top+28?-Math.ceil((rect.top+28-session.y)/3):session.y>rect.bottom-28?Math.ceil((session.y-rect.bottom+28)/3):0;
        if(speed){list.scrollTop+=speed;highlight(session);}
      }}
      scrollFrame=requestAnimationFrame(scroll);
    };
    const move=(e:globalThis.PointerEvent)=>{
      const session=pointer.current;if(!session||session.id!==e.pointerId)return;
      if(e.pointerType==='mouse'&&e.buttons===0){clear();return;}
      session.x=e.clientX;session.y=e.clientY;
      if(!session.active){
        if(Math.hypot(session.x-session.startX,session.y-session.startY)<6)return;
        session.active=true;count.current=session.ids.length;clearTimeout(clickTimer);suppressClick.current=true;
        latest.current.started();document.body.classList.add('dragging-items');
        scrollFrame=requestAnimationFrame(scroll);
      }
      // Removing a virtual grid row can release pointer capture. Keep the
      // session on the document and reacquire it on the stable app container.
      if(!session.host.hasPointerCapture(session.id))session.host.setPointerCapture(session.id);
      e.preventDefault();highlight(session);
    };
    const up=(e:globalThis.PointerEvent)=>{
      const session=pointer.current;if(!session||session.id!==e.pointerId)return;
      const target=session.active?document.elementFromPoint(e.clientX,e.clientY)?.closest<HTMLElement>('[data-category],[data-project-id]'):null;
      clear();
      if(session.active){
        e.preventDefault();clickTimer=window.setTimeout(()=>{suppressClick.current=false;},250);
        if(target?.dataset.category!==undefined)latest.current.category(session.ids,target.dataset.category);
        else if(target?.dataset.projectId)latest.current.project(session.ids,Number(target.dataset.projectId));
      }
    };
    const click=(e:MouseEvent)=>{if(suppressClick.current){e.preventDefault();e.stopImmediatePropagation();suppressClick.current=false;clearTimeout(clickTimer);}};
    const cancel=()=>{clear();clickTimer=window.setTimeout(()=>{suppressClick.current=false;},250);};
    const leave=(e:globalThis.DragEvent)=>{if(!e.relatedTarget)setFeedback(null);};
    const key=(e:KeyboardEvent)=>{if(e.key==='Escape'&&pointer.current?.active){e.preventDefault();e.stopImmediatePropagation();cancel();}else if(e.key==='Escape')clear();};
    document.addEventListener('pointermove',move,{capture:true,passive:false});
    document.addEventListener('pointerup',up,true);document.addEventListener('pointercancel',cancel,true);
    window.addEventListener('click',click,true);
    window.addEventListener('dragover',over,true);
    window.addEventListener('drop',clear,true);
    window.addEventListener('dragend',clear,true);
    window.addEventListener('dragleave',leave);
    window.addEventListener('blur',cancel);
    window.addEventListener('keydown',key,true);
    return()=>{
      clear();clearTimeout(clickTimer);
      document.removeEventListener('pointermove',move,true);document.removeEventListener('pointerup',up,true);
      document.removeEventListener('pointercancel',cancel,true);window.removeEventListener('click',click,true);
      window.removeEventListener('dragover',over,true);window.removeEventListener('drop',clear,true);
      window.removeEventListener('dragend',clear,true);window.removeEventListener('dragleave',leave);
      window.removeEventListener('blur',cancel);window.removeEventListener('keydown',key,true);
      ghost.current?.remove();
    };
  },[]);
  const pointerDown=(e:ReactPointerEvent<HTMLElement>)=>{
    suppressClick.current=false;
    if(!latest.current.enabled||e.button!==0||!e.isPrimary)return;
    const target=e.target instanceof Element?e.target:null;
    const card=target?.closest<HTMLElement>('.card[data-item-id]');
    if(!card||target?.closest('button:not(.card-open),input,textarea,select'))return;
    pointer.current={id:e.pointerId,ids:latest.current.ids(Number(card.dataset.itemId)),startX:e.clientX,startY:e.clientY,x:e.clientX,y:e.clientY,active:false,host:e.currentTarget};
  };
  const start=(e:DragEvent)=>{
    if(pointer.current){e.preventDefault();return;}
    const raw=e.dataTransfer.getData('application/design-items');
    if(!raw)return;
    let ids:unknown;try{ids=JSON.parse(raw);}catch{return;}
    if(!Array.isArray(ids)||!ids.length||!ids.every(Number.isInteger))return;
    count.current=ids.length;
    ghost.current?.remove();
    const canvas=document.createElement('canvas');canvas.width=1;canvas.height=1;
    canvas.className='item-drag-native-ghost';canvas.setAttribute('aria-hidden','true');
    document.body.append(canvas);ghost.current=canvas;
    e.dataTransfer.setDragImage(canvas,0,0);
    setFeedback({count:ids.length,x:Math.max(8,Math.min(e.clientX+18,window.innerWidth-272)),y:Math.max(8,Math.min(e.clientY+18,window.innerHeight-48)),action:'references'});
  };
  return {start,pointerDown,badge:feedback&&<div className="item-drag-badge" aria-hidden="true" style={{left:feedback.x,top:feedback.y}}><b>{feedback.count}</b><span>{feedback.action==='references'&&feedback.count===1?'reference':feedback.action}</span></div>};
}
