import { useState } from "react";
import { Dribbble, ExternalLink, MoreHorizontal } from "lucide-react";
import { openUrl } from "./api";
import { isResource, type Item } from "./model";

const categoryOf = (item: Item) => item.source.startsWith("Resources/") ? item.source.slice(10) : "Saved";
const favourites = [
  { name: "Pinterest", url: "https://www.pinterest.com/", icon: "pinterest" },
  { name: "Dribbble", url: "https://dribbble.com/", icon: "dribbble" },
  { name: "Awwwards", url: "https://www.awwwards.com/", icon: "awwwards" },
] as const;

function FavouriteIcon({ name }: { name: typeof favourites[number]["icon"] }) {
  if (name === "dribbble") return <Dribbble size={21} color="#EA4C89" strokeWidth={1.7} aria-hidden="true" />;
  return <svg width="21" height="21" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
    {name === "pinterest" ? <>
      <circle cx="12" cy="12" r="12" fill="#E60023" />
      <path fill="#fff" d="M12.2 4.5c-4 0-6.1 2.8-6.1 5.2 0 1.8.7 3.4 2.1 4 .2.1.4 0 .4-.2l.3-1.1c0-.2 0-.3-.1-.5-.4-.5-.7-1.1-.7-2 0-2.6 1.9-4.4 4.4-4.4 2.4 0 3.7 1.5 3.7 3.5 0 2.7-1.2 4.9-2.9 4.9-.9 0-1.6-.7-1.4-1.7.3-1.2.8-2.4.8-3.2 0-.7-.4-1.3-1.2-1.3-1 0-1.7 1-1.7 2.3 0 .8.3 1.4.3 1.4l-1.1 4.7c-.3 1.2-.2 2.6-.1 3.6.7-.8 1.5-2 1.8-3.2l.6-2.2c.3.6 1.3 1.1 2.3 1.1 3 0 5-2.7 5-6.2 0-2.6-2.2-5-5.4-5Z" />
    </> : <>
      <rect width="24" height="24" rx="5" fill="#A9E9D4" />
      <path d="m4.5 7 2.8 10 4.4-10 3.1 10 4.7-10" fill="none" stroke="#173D32" strokeWidth="2.3" strokeLinejoin="round" />
      <circle cx="20" cy="17" r="1.2" fill="#173D32" />
    </>}
  </svg>;
}
export default function Resources({ search, items, report, menu }: {
  search: string; items: Item[]; report: (e: unknown) => void;
  menu: (id: number, event: React.MouseEvent<HTMLButtonElement>) => void;
}) {
  const [category, setCategory] = useState("");
  const resources = items.filter(isResource);
  const shown = resources.filter(item => (!category || categoryOf(item) === category) && `${item.name} ${categoryOf(item)} ${item.path}`.toLowerCase().includes(search.toLowerCase().trim()));
  return <div className="resources-view">
    <nav className="resource-favourites" aria-label="Favourite websites">
      {favourites.map(site => <button key={site.icon} title={`Open ${site.name}`} onClick={() => void openUrl(site.url).catch(report)}>
        <FavouriteIcon name={site.icon} /><span>{site.name}</span>
      </button>)}
    </nav>
    <div className="resources-filter"><select aria-label="Resource category" value={category} onChange={e => setCategory(e.target.value)}>
      <option value="">All resources</option>
      {[...new Set(resources.map(categoryOf))].sort().map(value => <option key={value}>{value}</option>)}
    </select><small>{shown.length}</small></div>
    <table className="links-table resources-table">
      <thead><tr><th>Name</th><th>Type</th><th>Link</th><th className="actions-col" /></tr></thead>
      <tbody>{shown.map(item => <tr key={item.id} data-item-id={item.id}>
        <td><button onClick={() => void openUrl(item.path).catch(report)}>{item.name}</button></td>
        <td><small>{categoryOf(item)}</small></td>
        <td><button className="link-url" onClick={() => void openUrl(item.path).catch(report)}>{item.path.replace(/^https?:\/\//, "").replace(/\/$/, "")}<ExternalLink size={11} /></button></td>
        <td><button className="icon-button" aria-label={`Actions for ${item.name}`} onClick={event => menu(item.id, event)}><MoreHorizontal size={15} /></button></td>
      </tr>)}</tbody>
    </table>
    {!shown.length && <div className="modal-empty">No resources found</div>}
  </div>;
}
