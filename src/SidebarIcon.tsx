import type { BoardKey } from "./model";
import { Codepen } from "lucide-react";

export default function SidebarIcon({ name }: { name: BoardKey | "facebook" | "resources" }) {
  if (name === "codepen") return <Codepen size={16} color="#161616" strokeWidth={2} aria-hidden="true" data-sidebar-icon={name} />;
  return <svg width="16" height="16" viewBox="0 0 24 24" fill="none" aria-hidden="true" focusable="false" data-sidebar-icon={name}>
    {name === "fonts" ? <>
      <rect x="1" y="1" width="22" height="22" rx="5" fill="#D66D68" />
      <path d="m4.5 17 4-10 4 10M6 13.5h5M15 11.2c3.6-1.6 4.4 0 4.4 2.1V17m0-3.7c-5-1.5-5.8 4.8-1 3.3" stroke="#fff" strokeWidth="1.45" strokeLinecap="round" strokeLinejoin="round" />
    </> : name === "facebook" ? <>
      <circle cx="12" cy="12" r="12" fill="#0866FF" />
      <path fill="#fff" d="m16.67 15.47.53-3.47h-3.33V9.75c0-.95.47-1.88 1.96-1.88h1.51V4.92s-1.37-.23-2.69-.23c-2.74 0-4.53 1.66-4.53 4.67V12H7.08v3.47h3.04v8.38a12.1 12.1 0 0 0 3.75 0v-8.38h2.8Z" />
    </> : name === "brand-guidelines" ? <>
      <rect x="5" y="2" width="16" height="20" rx="3" fill="#8354D5" />
      <path d="M8 2v20" stroke="#603AA8" strokeWidth="2" />
      <rect x="11" y="5" width="7" height="2" rx="1" fill="#fff" />
      <rect x="11" y="10" width="3" height="4" rx=".7" fill="#F6B9DA" />
      <rect x="15" y="10" width="3" height="4" rx=".7" fill="#FBD478" />
      <rect x="11" y="15" width="7" height="3" rx=".7" fill="#9CE0E4" />
    </> : <>
      <circle cx="12" cy="12" r="11" fill="#E79B32" />
      <circle cx="12" cy="12" r="8" stroke="#fff" strokeOpacity=".55" strokeWidth="1.2" />
      <path d="m16.7 7.3-3 6.4-6.4 3 3-6.4 6.4-3Z" fill="#fff" />
      <path d="m16.7 7.3-3 6.4-3.4-3.4 6.4-3Z" fill="#A55723" />
      <circle cx="12" cy="12" r="1.2" fill="#E79B32" />
    </>}
  </svg>;
}
