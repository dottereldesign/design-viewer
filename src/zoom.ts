export type ImageTransform = { scale: number; x: number; y: number };
export const fitTransform: ImageTransform = { scale: 1, x: 0, y: 0 };

// Coordinates are relative to the centre of the viewport. Keep the image point
// beneath the cursor fixed as its distance from the image centre changes.
export function zoomAtPoint(
  current: ImageTransform,
  point: { x: number; y: number },
  factor: number,
): ImageTransform {
  const scale = Math.max(1, Math.min(16, current.scale * factor));
  if (scale === 1) return { ...fitTransform };
  const ratio = scale / current.scale;
  return {
    scale,
    x: point.x - (point.x - current.x) * ratio,
    y: point.y - (point.y - current.y) * ratio,
  };
}
