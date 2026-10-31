import { useCallback, useEffect, useRef, useState } from "react";
import { fitTransform, zoomAtPoint, type ImageTransform } from "./zoom";

export default function ZoomImage({
  src, alt, onError, onZoomChange, resetToken,
}: {
  src: string; alt: string; onError: () => void;
  onZoomChange: (scale: number) => void; resetToken: number;
}) {
  const viewport = useRef<HTMLDivElement>(null);
  const image = useRef<HTMLImageElement>(null);
  const current = useRef<ImageTransform>({ ...fitTransform });
  const drag = useRef<{ id: number; x: number; y: number; origin: ImageTransform } | null>(null);
  const [transform, setTransform] = useState<ImageTransform>({ ...fitTransform });
  const [fitted, setFitted] = useState({ width: 0, height: 0 });
  const [dragging, setDragging] = useState(false);
  const commit = useCallback((next: ImageTransform) => {
    current.current = next;
    setTransform(next);
    onZoomChange(next.scale);
  }, [onZoomChange]);
  const reset = useCallback(() => commit({ ...fitTransform }), [commit]);
  const measure = useCallback(() => {
    const el = viewport.current, img = image.current;
    if (!el || !img?.naturalWidth || !el.clientWidth || !el.clientHeight) return;
    const fit = Math.min(1, el.clientWidth / img.naturalWidth, el.clientHeight / img.naturalHeight);
    setFitted({ width: img.naturalWidth * fit, height: img.naturalHeight * fit });
    reset();
  }, [reset]);
  useEffect(() => { reset(); }, [resetToken, src, reset]);
  useEffect(() => {
    const el = viewport.current;
    if (!el) return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    // React wheel handlers are passive; this listener prevents the browser's
    // page zoom/scroll while the image itself is being magnified.
    const wheel = (event: WheelEvent) => {
      if (!image.current?.naturalWidth || !event.deltaY) return;
      event.preventDefault();
      event.stopPropagation();
      const rect = el.getBoundingClientRect();
      const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? rect.height : 1);
      commit(zoomAtPoint(current.current, {
        x: event.clientX - rect.left - rect.width / 2,
        y: event.clientY - rect.top - rect.height / 2,
      }, Math.exp(-Math.max(-600, Math.min(600, delta)) * 0.0015)));
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => { observer.disconnect(); el.removeEventListener("wheel", wheel); };
  }, [commit, measure]);

  return <div ref={viewport} className={`zoom-image ${transform.scale > 1 ? "magnified" : ""} ${dragging ? "dragging" : ""}`}
    role="group" aria-label="Image viewer: scroll to zoom, drag to pan, double-click to reset"
    onDoubleClick={reset}
    onPointerDown={event => {
      if (event.button !== 0 || current.current.scale <= 1) return;
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      drag.current = { id: event.pointerId, x: event.clientX, y: event.clientY, origin: { ...current.current } };
      setDragging(true);
    }}
    onPointerMove={event => {
      const start = drag.current;
      if (!start || start.id !== event.pointerId) return;
      commit({ ...start.origin, x: start.origin.x + event.clientX - start.x, y: start.origin.y + event.clientY - start.y });
    }}
    onPointerUp={event => {
      if (drag.current?.id !== event.pointerId) return;
      drag.current = null; setDragging(false);
      if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    }}
    onLostPointerCapture={() => { drag.current = null; setDragging(false); }}
  >
    <img ref={image} src={src} alt={alt} draggable={false} onLoad={measure} onError={onError}
      style={{ width: fitted.width, height: fitted.height, visibility: fitted.width ? "visible" : "hidden", transform: `translate(${transform.x}px, ${transform.y}px) scale(${transform.scale})` }} />
  </div>;
}
