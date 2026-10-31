import { useEffect, useRef, useState, type ReactNode } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { CaptureDraft, CaptureFile } from "./CaptureDialog";
import type { Item } from "./model";

export const thumbnailRatios = [["4:3",4/3],["1:1",1],["16:9",16/9],["3:4",3/4],["Free",0]] as const;
export function RatioSelect({value, change, disabled = false}: {value:number; change:(n:number)=>void; disabled?:boolean}) {
  return <select aria-label="Thumbnail aspect ratio" value={value} disabled={disabled} onChange={e=>change(Number(e.target.value))}>
    {thumbnailRatios.map(([label,ratio])=><option value={ratio} key={label}>{label}</option>)}
  </select>;
}

// The crop is kept in normalized image coordinates, independent of window size/DPI.
export function ThumbnailCrop({file, ratio, save, close, actions, onRatio, externalBusy=false, externalError=""}: {file:CaptureFile; ratio:number; save:(file:CaptureFile)=>Promise<void>; close:()=>void; actions?:ReactNode; onRatio?:(n:number)=>void; externalBusy?:boolean; externalError?:string}) {
  const [dimensions,setDimensions] = useState({width:0,height:0});
  const [aspect,setAspect] = useState(ratio);
  const [zoom,setZoom] = useState(1);
  const [center,setCenter] = useState({x:.5,y:.5});
  const [saving,setBusy] = useState(false), [error,setError] = useState("");
  const busy = saving || externalBusy;
  const imageRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{x:number;y:number;cx:number;cy:number}|null>(null);
  const originalAspect = dimensions.width / dimensions.height || 1;
  const cropAspect = aspect || originalAspect;
  const w = Math.min(1,cropAspect/originalAspect)/zoom;
  const h = Math.min(1,originalAspect/cropAspect)/zoom;
  const cx = Math.max(w/2,Math.min(1-w/2,center.x));
  const cy = Math.max(h/2,Math.min(1-h/2,center.y));
  const commit = async () => {
    if (!dimensions.width || busy) return;
    setBusy(true); setError("");
    let cropped: CaptureFile | undefined;
    try {
      const x = Math.max(0,Math.round((cx-w/2)*dimensions.width));
      const y = Math.max(0,Math.round((cy-h/2)*dimensions.height));
      cropped = await invoke<CaptureFile>("crop_thumbnail",{path:file.path,x,y,width:Math.min(dimensions.width-x,Math.max(1,Math.round(w*dimensions.width))),height:Math.min(dimensions.height-y,Math.max(1,Math.round(h*dimensions.height)))});
      await save(cropped);
      cropped = undefined;
    } catch(e) {setError(String(e));setBusy(false);}
    finally { if (cropped) void invoke("discard_capture",{paths:[cropped.path]}).catch(()=>{}); }
  };
  return <div className="modal-backdrop" onMouseDown={e=>{e.stopPropagation();if(e.target===e.currentTarget&&!busy)close();}}>
    <div className="modal thumbnail-editor" role="dialog" aria-modal="true" aria-label="Adjust thumbnail" onKeyDown={e=>{e.stopPropagation();if(e.key==='Escape'&&!busy)close();if(e.key==='Enter'){e.preventDefault();void commit();}}}>
      <div className="modal-heading"><h2>Adjust thumbnail</h2><button className="icon-button" aria-label="Close thumbnail editor" disabled={busy} onClick={close}>×</button></div>
      <div className="thumbnail-controls"><RatioSelect value={aspect} change={n=>{setAspect(n);setZoom(1);onRatio?.(n);}} disabled={busy}/><label>Zoom <input aria-label="Thumbnail zoom" type="range" min="1" max="4" step="0.01" value={zoom} disabled={busy} onChange={e=>setZoom(Number(e.target.value))}/></label><button className="subtle-button" disabled={busy} onClick={()=>{setAspect(0);onRatio?.(0);setZoom(1);setCenter({x:.5,y:.5});}}>Whole image</button>{actions}</div>
      <div className="thumbnail-crop-image" ref={imageRef}>
        <img draggable={false} src={convertFileSrc(file.path)} alt="Thumbnail to crop" onLoad={e=>setDimensions({width:e.currentTarget.naturalWidth,height:e.currentTarget.naturalHeight})}/>
        {!!dimensions.width&&<div className="thumbnail-crop-box" style={{left:`${(cx-w/2)*100}%`,top:`${(cy-h/2)*100}%`,width:`${w*100}%`,height:`${h*100}%`}}
          onPointerDown={e=>{if(busy)return;e.currentTarget.setPointerCapture(e.pointerId);drag.current={x:e.clientX,y:e.clientY,cx,cy};}}
          onPointerMove={e=>{const d=drag.current,rect=imageRef.current?.getBoundingClientRect();if(!d||!rect)return;setCenter({x:Math.max(w/2,Math.min(1-w/2,d.cx+(e.clientX-d.x)/rect.width)),y:Math.max(h/2,Math.min(1-h/2,d.cy+(e.clientY-d.y)/rect.height))});}}
          onPointerUp={()=>{drag.current=null;}} onPointerCancel={()=>{drag.current=null;}} />}
      </div>
      <small>Drag the frame to position it. Zoom to crop closer.</small>
      {(error||externalError)&&<div className="form-error" role="alert">{error||externalError}</div>}
      <div className="modal-footer"><button className="subtle-button" disabled={busy} onClick={close}>Cancel</button><button className="primary" disabled={busy||!dimensions.width} onClick={()=>void commit()}>{busy?'Saving…':'Save thumbnail'}</button></div>
    </div>
  </div>;
}

export default function ThumbnailEditor({item,close,complete}: {item:Item;close:()=>void;complete:()=>Promise<void>}) {
  const [file,setFile] = useState<CaptureFile|null>(item.thumbnail?{path:item.thumbnail,name:'Thumbnail.png'}:null);
  const [ratio,setRatio] = useState(4/3);
  const [busy,setBusy] = useState(false),[error,setError] = useState("");
  const staged = useRef<string[]>([]);
  const cleanup = useRef<ReturnType<typeof setTimeout>|undefined>(undefined);
  useEffect(()=>{clearTimeout(cleanup.current);return()=>{cleanup.current=setTimeout(()=>void invoke('discard_capture',{paths:staged.current}).catch(()=>{}),0);};},[]);
  const save = async (next:CaptureFile) => {staged.current.push(next.path);await invoke('replace_thumbnail',{id:item.id,file:next});await complete();};
  const capture = async () => {
    if(busy)return;setBusy(true);setError("");
    try {const next=await invoke<CaptureFile|null>('capture_screen',{ratio});if(next){staged.current.push(next.path);setFile(next);await save(next);}}
    catch(e){setError(String(e));} finally{setBusy(false);}
  };
  const paste = async () => {
    if(busy)return;setBusy(true);setError("");
    try {const incoming=await invoke<CaptureDraft>('read_clipboard');staged.current.push(...incoming.files.map(f=>f.path));if(!incoming.files[0])throw Error('Copy an image first.');setFile(incoming.files[0]);}
    catch(e){setError(String(e));}finally{setBusy(false);}
  };
  if (file) return <ThumbnailCrop key={file.path} file={file} ratio={ratio} onRatio={setRatio} save={save} close={close} externalBusy={busy} externalError={error} actions={<><button className="subtle-button" disabled={busy} onClick={()=>void capture()}>Capture & save</button><button className="subtle-button" disabled={busy} onClick={()=>void paste()}>Paste replacement</button></>}/>;
  return <div className="modal-backdrop" onMouseDown={e=>{if(e.target===e.currentTarget&&!busy)close();}}>
    <div className="modal thumbnail-editor" role="dialog" aria-modal="true" aria-label="Edit thumbnail" onKeyDown={e=>{if(e.key==='Escape'&&busy)e.stopPropagation();}}>
      <div className="modal-heading"><h2>Edit thumbnail</h2><button className="icon-button" disabled={busy} onClick={close} aria-label="Close thumbnail">×</button></div>
      <div className="thumbnail-controls"><RatioSelect value={ratio} change={setRatio} disabled={busy}/><button autoFocus className="primary" disabled={busy} onClick={()=>void capture()}>{busy?'Working…':'Capture & save'}</button><button className="subtle-button" disabled={busy} onClick={()=>void paste()}>Paste image</button></div>
      {error&&<div className="form-error" role="alert">{error}</div>}
    </div>
  </div>;
}
