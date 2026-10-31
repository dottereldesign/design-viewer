import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { Library, Item } from "./model";
export const openUrl = (url: string) => invoke<void>("open_external_url", { url });
export const loadLibrary = () => invoke<Library>("get_library");
export type Change = {
  action: string;
  ids?: number[];
  target?: number;
  name?: string;
  value?: string;
};
export const changeLibrary = (change: Change) =>
  invoke<void>("update", { change });
const cache = new Map<string, string>();
type Job = {
  run: () => void;
  signal?: AbortSignal;
  reject: (reason: Error) => void;
};
let active = 0;
const queue: Job[] = [];
function pump() {
  while (active < 3 && queue.length) {
    const task = queue.shift()!;
    if (task.signal?.aborted) {
      task.reject(new Error("Cancelled"));
      continue;
    }
    active++;
    task.run();
  }
}
export async function media(
  item: Item,
  mode: string,
  signal?: AbortSignal,
): Promise<string> {
  const key = `${item.id}:${item.modified}:${mode}`;
  if (cache.has(key)) return cache.get(key)!;
  if (mode === "preview" && item.kind === "animation")
    return convertFileSrc(item.path);
  if (mode === "original" && !item.path.toLowerCase().endsWith(".mov"))
    return convertFileSrc(item.path);
  return new Promise<string>((resolve, reject) => {
    const task: Job = {
      signal,
      reject,
      run: () => {
        invoke<string>("get_media", { id: item.id, mode })
          .then((path) => {
            const url = convertFileSrc(path);
            cache.set(key, url);
            resolve(url);
          }, reject)
          .finally(() => {
            active--;
            pump();
          });
      },
    };
    if (mode === "thumbnail") queue.push(task);
    else queue.unshift(task);
    pump();
  });
}
