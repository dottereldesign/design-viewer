import { describe, it } from "node:test";
import { deepStrictEqual as equal } from "node:assert";
import { filterItems, orderedProjects, categoryOf, libraryCategories, type Filters, type Item } from "./model";
const items: Item[] = [
  {
    id: 1,
    name: "Calm",
    path: "C:/Design/mobile/calm.webp",
    kind: "image",
    source: "mobile",
    tags: [1],
    projects: [7],
    added: 10,
    modified: 10,
    missing: false,
  },
  {
    id: 2,
    name: "School",
    path: "C:/Design/web_design/school.mp4",
    kind: "video",
    source: "web_design/school",
    tags: [2],
    projects: [7],
    added: 20,
    modified: 20,
    missing: false,
  },
  {
    id: 3,
    name: "Sports",
    path: "https://sport.example",
    kind: "link",
    source: "",
    tags: [3],
    projects: [],
    added: 30,
    modified: 0,
    missing: false,
  },
];
const defaults: Filters = {
  search: "",
  type: "all",
  tags: [],
  source: "",
  project: null,
  linksOnly: false,
  sort: "recent",
  seed: 123,
};

it("Captures is a storage folder; unassigned captures belong to Uncategorised",()=>{
  const capture={...items[0],source:"Design/Captures"};
  equal(categoryOf(capture),"");
  equal(categoryOf({...capture,category:"Captures"}),"");
  equal(categoryOf({...capture,category:"branding"}),"branding");
  equal(libraryCategories({items:[capture],sources:["Design/Captures"],categories:["Captures","Packaging"],projects:[],tags:[],root:"",scanning:false}),["Packaging"]);
  equal(filterItems([capture],{...defaults,category:""}).length,1);
});
describe("combined library views", () => {
  it("uses category overrides instead of disk folders, including explicit uncategorised",()=>{
    const original={...items[0],source:"Design/3d/components"};
    const moved={...original,category:"branding"};
    equal(categoryOf(original),"3d");equal(categoryOf(moved),"branding");equal(categoryOf({...moved,category:""}),"");
    equal(filterItems([moved],{...defaults,category:"3d"}).length,0);
    equal(filterItems([moved],{...defaults,category:"branding"}).map(i=>i.id),[1]);
    equal(filterItems([{...moved,source:"Photography"}],{...defaults,category:"branding",source:"Photography"}).length,1);
    equal(libraryCategories({items:[moved],sources:["Design/3d/components","Photography","Videos"],categories:["Empty"],projects:[],tags:[],root:"",scanning:false}),["3d","branding","Empty"]);
  });
  it("puts recently used projects first, ignores removed project ids and preserves source order", () => {
    const projects = [{ id: 1, name: "Alpha", count: 0 }, { id: 2, name: "Beta", count: 0 }, { id: 3, name: "Gamma", count: 0 }];
    equal(orderedProjects(projects, [3, 99, 2]).map((project) => project.id), [3, 2, 1]);
    equal(projects.map((project) => project.id), [1, 2, 3]);
  });
  it("keeps Resources separate from Links and All while allowing project membership", () => {
    const resource: Item = { ...items[2], id: 99, source: "Resources/SaaS", projects: [7] };
    const combined = [...items, resource];
    equal(filterItems(combined, { ...defaults, linksOnly: true }).map(i => i.id), [3]);
    equal(filterItems(combined, defaults).some(i => i.id === 99), false);
    equal(filterItems(combined, { ...defaults, project: 7 }).some(i => i.id === 99), true);
  });
  it("keeps thumbnail collections separate and searchable through projects and tags", () => {
    for (const source of ["Fonts", "CodePen", "Brand Guidelines"]) {
      const board: Item = { ...items[2], id: 99, source, thumbnail: "thumb.png", projects: [7], tags: [2] };
      equal(filterItems([board], defaults).length, 0);
      equal(filterItems([board], { ...defaults, collection: source }).map(i=>i.id), [99]);
      equal(filterItems([board], { ...defaults, collection: source, tags: [3] }).length, 0);
      equal(filterItems([board], { ...defaults, linksOnly: true }).length, 0);
      equal(filterItems([board], { ...defaults, project: 7, tags: [2] }).map(i=>i.id), [99]);
      equal(filterItems([board], { ...defaults, project: 7, tags: [3] }).length, 0);
    }
  });
  it("pins references ahead of the current sort only in their own project", () => {
    const library = items.map((i) => ({
      ...i,
      favourites: i.id === 1 ? [7] : [],
    }));
    equal(
      filterItems(library, { ...defaults, project: 7 }).map((i) => i.id),
      [1, 2],
    );
    equal(
      filterItems(library, defaults).map((i) => i.id),
      [3, 2, 1],
    );
    equal(
      filterItems(library, { ...defaults, project: 7, sort: "name" }).map(
        (i) => i.id,
      ),
      [1, 2],
    );
  });
  it("includes links and every media type in All", () =>
    equal(
      filterItems(items, defaults).map((i) => i.id),
      [3, 2, 1],
    ));
  it("combines project, descendant source and type filters", () =>
    equal(
      filterItems(items, {
        ...defaults,
        project: 7,
        source: "web_design",
        type: "video",
      }).map((i) => i.id),
      [2],
    ));
  it("matches any selected tag within the active project", () =>
    equal(
      filterItems(items, { ...defaults, project: 7, tags: [2, 3] }).map(
        (i) => i.id,
      ),
      [2],
    ));
  it("searches URLs without casing or surrounding-space surprises", () =>
    equal(
      filterItems(items, {
        ...defaults,
        linksOnly: true,
        search: "  SPORT.EXAMPLE  ",
      }).map((i) => i.id),
      [3],
    ));
  it("keeps shuffle stable without changing the original library order", () => {
    const before = items.map((i) => i.id);
    equal(
      filterItems(items, { ...defaults, sort: "shuffle" }),
      filterItems(items, { ...defaults, sort: "shuffle" }),
    );
    equal(
      items.map((i) => i.id),
      before,
    );
  });
});

it("Photography is a category, with explicit moves taking precedence",()=>{
  const photo={...items[0],source:"Photography"};
  equal(categoryOf(photo),"Photography");
  equal(categoryOf({...photo,source:"Photography/Captures"}),"Photography");
  equal(categoryOf({...photo,category:""}),"");
  equal(categoryOf({...photo,category:"branding"}),"branding");
  equal(filterItems([photo],{...defaults,category:"Photography"}).map(i=>i.id),[1]);
});
