use super::{content, db, library};
use db::Change;
use std::fs;
#[test]
fn captures_stay_uncategorised_through_rescans_and_preserve_chosen_categories() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path().join("Design");fs::create_dir(&root).unwrap();
    let path=temp.path().join("library.sqlite");db::init(&path).unwrap();let conn=db::connect(&path).unwrap();
    db::set_setting(&conn,"root",root.to_str().unwrap()).unwrap();crate::sources::ensure(&conn).unwrap();
    let mut ids=vec![];
    for size in [8,9] {let original=temp.path().join(format!("image-{size}.png"));image::RgbImage::new(size,size).save(&original).unwrap();ids.push(content::capture(&path,capture_request(&original,vec![],vec![])).unwrap().ids[0]);}
    assert!(db::load(&path,false).unwrap().items.iter().all(|i|i.category.as_deref()==Some("")));
    conn.execute("INSERT INTO categories(name) VALUES('Captures')",[]).unwrap();
    conn.execute("INSERT INTO item_categories(item_id,name) VALUES(?,'Captures')",[ids[0]]).unwrap();
    db::change(&path,change("set_category",vec![ids[1]],None,None,Some("branding"))).unwrap();
    let before=db::load(&path,false).unwrap();db::init(&path).unwrap();library::scan_all(&path).unwrap();
    let after=db::load(&path,false).unwrap();assert!(!after.categories.iter().any(|n|n.eq_ignore_ascii_case("Captures")));
    for item in after.items {let original=before.items.iter().find(|i|i.id==item.id).unwrap();assert_eq!(item.path,original.path);assert!(std::path::Path::new(&item.path).is_file());assert_eq!(item.category.as_deref(),Some(if item.id==ids[0]{""}else{"branding"}));}
}
#[test]
fn organisation_history_is_durable_atomic_and_restores_bulk_memberships() {
    let temp=tempfile::tempdir().unwrap();let path=temp.path().join("library.sqlite");db::init(&path).unwrap();
    for name in ["First","Second"] {db::change(&path,change("create_link",vec![],None,Some(name),Some(&format!("https://{}.example",name.to_lowercase())))).unwrap();}
    db::change(&path,change("create_project",vec![],None,Some("Website"),None)).unwrap();
    let lib=db::load(&path,false).unwrap();let project=lib.projects[0].id;let ids:Vec<_>=lib.items.iter().map(|i|i.id).collect();
    db::change(&path,change("add_project",vec![ids[0]],Some(project),None,None)).unwrap();
    db::change(&path,change("pin_project",vec![ids[0]],Some(project),None,None)).unwrap();
    let count=db::load(&path,false).unwrap().undo_count;
    db::change(&path,change("add_project",vec![ids[0]],Some(project),None,None)).unwrap();
    assert_eq!(db::load(&path,false).unwrap().undo_count,count,"No-op must not consume Undo");
    db::change(&path,change("add_project",ids.clone(),Some(project),None,None)).unwrap();
    db::init(&path).unwrap();assert_eq!(db::load(&path,false).unwrap().undo_label.as_deref(),Some("Add to project"));
    db::change(&path,change("undo_organisation",vec![],None,None,None)).unwrap();
    let lib=db::load(&path,false).unwrap();assert_eq!(lib.items.iter().find(|i|i.id==ids[0]).unwrap().favourites,vec![project]);assert!(lib.items.iter().find(|i|i.id==ids[1]).unwrap().projects.is_empty());
    db::change(&path,change("remove_project",ids.clone(),Some(project),None,None)).unwrap();
    db::change(&path,change("undo_organisation",vec![],None,None,None)).unwrap();
    assert_eq!(db::load(&path,false).unwrap().items.iter().find(|i|i.id==ids[0]).unwrap().favourites,vec![project]);
    let count=db::load(&path,false).unwrap().undo_count;
    assert!(db::change(&path,change("add_project",vec![ids[1],999999],Some(project),None,None)).is_err());
    assert_eq!(db::load(&path,false).unwrap().undo_count,count);
    assert!(db::load(&path,false).unwrap().items.iter().find(|i|i.id==ids[1]).unwrap().projects.is_empty());
    db::change(&path,change("delete_project",vec![],Some(project),None,None)).unwrap();
    db::change(&path,change("undo_organisation",vec![],None,None,None)).unwrap();
    assert_eq!(db::load(&path,false).unwrap().items.iter().find(|i|i.id==ids[0]).unwrap().favourites,vec![project]);
    db::change(&path,change("remove_items",ids.clone(),None,None,None)).unwrap();assert!(db::load(&path,false).unwrap().items.is_empty());
    db::change(&path,change("undo_organisation",vec![],None,None,None)).unwrap();assert_eq!(db::load(&path,false).unwrap().items.len(),2);
    for n in 0..110 {db::change(&path,change("rename_project",vec![],Some(project),Some(&format!("Project {n}")),None)).unwrap();}
    assert_eq!(db::load(&path,false).unwrap().undo_count,100);
    let backup=temp.path().join("backup.json");db::export(&path,backup.clone()).unwrap();db::import(&path,backup).unwrap();assert_eq!(db::load(&path,false).unwrap().undo_count,0);
}

#[test]
fn category_undo_restores_inherited_category_instead_of_freezing_it() {
    let temp=tempfile::tempdir().unwrap();let path=temp.path().join("library.sqlite");db::init(&path).unwrap();
    let conn=db::connect(&path).unwrap();conn.execute("INSERT INTO items(path,name,kind,source,added) VALUES('C:/logo.png','Logo','image','Design/3d',1)",[]).unwrap();let id=conn.last_insert_rowid();
    db::change(&path,change("set_category",vec![id],None,None,Some("branding"))).unwrap();
    db::change(&path,change("undo_organisation",vec![],None,None,None)).unwrap();
    assert_eq!(db::load(&path,false).unwrap().items[0].category.as_deref(),Some("3d"));
    let overrides:i64=conn.query_row("SELECT count(*) FROM item_categories WHERE item_id=?",[id],|r|r.get(0)).unwrap();assert_eq!(overrides,0);
}
#[test]
fn categories_persist_through_rescans_backups_and_undo_without_moving_files() {
    let temp=tempfile::tempdir().unwrap();
    let root=temp.path().join("Design");fs::create_dir_all(root.join("3d")).unwrap();
    image::RgbImage::new(8,8).save(root.join("3d/logo.png")).unwrap();
    image::RgbImage::new(9,9).save(root.join("3d/scene.png")).unwrap();
    let path=temp.path().join("library.sqlite");db::init(&path).unwrap();
    let connection=db::connect(&path).unwrap();db::set_setting(&connection,"root",root.to_str().unwrap()).unwrap();
    crate::sources::ensure(&connection).unwrap();library::scan_all(&path).unwrap();
    let before=db::load(&path,false).unwrap();let ids:Vec<i64>=before.items.iter().map(|i|i.id).collect();
    assert!(before.items.iter().all(|i|i.category.as_deref()==Some("3d")));
    db::change(&path,change("create_project",vec![],None,Some("Website"),None)).unwrap();
    let lib=db::load(&path,false).unwrap();let project=lib.projects[0].id;let tag=lib.tags[0].id;
    db::change(&path,change("add_project",ids.clone(),Some(project),None,None)).unwrap();
    db::change(&path,change("add_tag",ids.clone(),Some(tag),None,None)).unwrap();
    db::change(&path,change("pin_project",ids.clone(),Some(project),None,None)).unwrap();
    db::change(&path,change("set_category",ids.clone(),None,None,Some("branding"))).unwrap();
    library::scan_all(&path).unwrap();let moved=db::load(&path,false).unwrap();
    for item in &moved.items {
        assert_eq!(item.category.as_deref(),Some("branding"));assert_eq!(item.source,"Design/3d");
        assert!(std::path::Path::new(&item.path).is_file());assert_eq!(item.projects,vec![project]);assert_eq!(item.tags,vec![tag]);assert_eq!(item.favourites,vec![project]);
    }
    assert!(db::change(&path,change("set_category",vec![ids[0],999999],None,None,Some("wrong"))).is_err());
    assert_eq!(db::load(&path,false).unwrap().items[0].category.as_deref(),Some("branding"));
    db::change(&path,change("create_category",vec![],None,Some("Empty category"),None)).unwrap();
    let backup=temp.path().join("metadata.json");db::export(&path,backup.clone()).unwrap();
    let restored=temp.path().join("restored.sqlite");db::init(&restored).unwrap();db::import(&restored,backup).unwrap();
    let imported=db::load(&restored,false).unwrap();assert!(imported.categories.contains(&"Empty category".to_string()));
    assert!(imported.items.iter().all(|i|i.category.as_deref()==Some("branding")));
    db::change(&path,change("set_category",ids.clone(),None,None,Some(""))).unwrap();library::scan_all(&path).unwrap();
    assert!(db::load(&path,false).unwrap().items.iter().all(|i|i.category.as_deref()==Some("")));
    let undo=serde_json::to_string(&ids.iter().map(|id|(*id,"3d")).collect::<Vec<_>>()).unwrap();
    db::change(&path,change("restore_categories",vec![],None,None,Some(&undo))).unwrap();
    assert!(db::load(&path,false).unwrap().items.iter().all(|i|i.category.as_deref()==Some("3d")));
    db::change(&path,change("create_link",vec![],None,Some("Link"),Some("https://example.com"))).unwrap();
    let link=db::load(&path,false).unwrap().items.into_iter().find(|i|i.kind=="link").unwrap();
    assert!(db::change(&path,change("set_category",vec![link.id],None,None,Some("branding"))).is_err());
}
#[test]
fn thumbnail_crop_checks_bounds_and_preserves_selected_pixels() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("input.png");
    let image = image::RgbaImage::from_fn(120,90,|x,y|image::Rgba([x as u8,y as u8,20,128]));
    image.save(&path).unwrap();
    let crop = crate::thumbnail::crop(temp.path(),path.to_str().unwrap(),12,8,80,60).unwrap();
    let cropped = image::open(crop.path).unwrap().to_rgba8();
    assert_eq!(cropped.dimensions(),(80,60));
    assert_eq!(cropped.get_pixel(0,0),image.get_pixel(12,8));
    assert_eq!(cropped.get_pixel(79,59),image.get_pixel(91,67));
    for (x,y,w,h) in [(0,0,0,5),(119,0,2,1),(0,u32::MAX,1,2),(u32::MAX,0,2,1)] {
        assert!(crate::thumbnail::crop(temp.path(),path.to_str().unwrap(),x,y,w,h).is_err());
    }
    assert_eq!(image::open(path).unwrap().to_rgba8(),image);
}
#[test]
fn replacing_thumbnail_preserves_link_and_memberships() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("library.sqlite");
    db::init(&path).unwrap();
    let connection = db::connect(&path).unwrap();
    connection.execute("INSERT INTO items(path,name,kind,source,added) VALUES('https://codepen.io/a/pen/b','Example','link','CodePen',1)",[]).unwrap();
    let id = connection.last_insert_rowid();
    let file = content::stage_begin(temp.path(),"test.png").unwrap();
    image::RgbaImage::from_pixel(160,90,image::Rgba([20,40,60,255])).save(&file.path).unwrap();
    crate::thumbnail::replace(&path,id,&file).unwrap();
    fs::remove_file(&file.path).unwrap();
    let (url,source,thumbnail):(String,String,String) = connection.query_row("SELECT path,source,thumbnail FROM items WHERE id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(url,"https://codepen.io/a/pen/b"); assert_eq!(source,"CodePen");
    assert_eq!(image::image_dimensions(thumbnail).unwrap(),(160,90));
    assert!(crate::thumbnail::replace(&path,id+1,&file).is_err());
}
#[test]
fn clipboard_image_contains_original_pixels_and_transparency_without_writing_clipboard() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Design");
    fs::create_dir(&root).unwrap();
    let image = image::RgbaImage::from_pixel(503, 307, image::Rgba([12, 34, 56, 78]));
    image.save(root.join("original.png")).unwrap();
    image.save(root.join("original.webp")).unwrap();
    fs::write(root.join("video.mp4"), b"not an image").unwrap();
    let path = temp.path().join("copy.sqlite");
    db::init(&path).unwrap();
    library::scan(&path, &root).unwrap();
    let lib = db::load(&path, false).unwrap();
    for item in lib.items {
        let copied = crate::clipboard::image_data(
            &path,
            &temp.path().join("cache"),
            std::path::Path::new("missing-ffmpeg"),
            item.id,
        );
        if item.kind == "video" {
            assert!(copied.is_err());
            continue;
        }
        let copied = copied.unwrap();
        assert_eq!((copied.width, copied.height), (503, 307));
        assert_eq!(copied.bytes.as_ref(), image.as_raw());
        assert!(std::path::Path::new(&item.path).is_file());
        db::change(
            &path,
            change("remove_items", vec![item.id], None, None, None),
        )
        .unwrap();
        assert!(crate::clipboard::image_data(
            &path,
            &temp.path().join("cache"),
            std::path::Path::new("missing-ffmpeg"),
            item.id
        )
        .is_err());
    }
}

#[test]
fn resource_capture_seeding_move_restore_and_backup_preserve_destination() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("resources.sqlite");
    db::init(&path).unwrap();
    let conn = db::connect(&path).unwrap();
    crate::resources::seed(&conn).unwrap();
    crate::resources::seed(&conn).unwrap();
    assert_eq!(db::load(&path, false).unwrap().items.len(), 39);
    db::change(
        &path,
        change("create_project", vec![], None, Some("Inspiration"), None),
    )
    .unwrap();
    let lib = db::load(&path, false).unwrap();
    let project = lib.projects[0].id;
    let tag = lib.tags[0].id;
    let input = |resource| {
        serde_json::from_value::<content::Capture>(serde_json::json!({"url":"https://example.com/reference", "resource":resource, "projects":[project], "tags":[tag]})).unwrap()
    };
    let link = content::capture(&path, input(false)).unwrap();
    assert_eq!(link.destination, Some("links"));
    let resource = content::capture(&path, input(true)).unwrap();
    assert_eq!(resource.destination, Some("resources"));
    assert_eq!(resource.ids, link.ids);
    assert_eq!(resource.reused, 1);
    let id = resource.ids[0];
    let loaded = db::load(&path, false).unwrap();
    let item = loaded.items.iter().find(|i| i.id == id).unwrap();
    assert_eq!(item.source, "Resources");
    assert_eq!(item.projects, vec![project]);
    assert_eq!(item.tags, vec![tag]);
    let backup = temp.path().join("metadata.json");
    db::export(&path, backup.clone()).unwrap();
    db::change(
        &path,
        change(
            "set_link_destination",
            vec![],
            Some(id),
            None,
            Some("links"),
        ),
    )
    .unwrap();
    assert_eq!(
        db::load(&path, false)
            .unwrap()
            .items
            .iter()
            .find(|i| i.id == id)
            .unwrap()
            .source,
        ""
    );
    db::import(&path, backup).unwrap();
    assert_eq!(
        db::load(&path, false)
            .unwrap()
            .items
            .iter()
            .find(|i| i.id == id)
            .unwrap()
            .source,
        "Resources"
    );
    db::change(&path, change("remove_items", vec![id], None, None, None)).unwrap();
    crate::resources::seed(&conn).unwrap();
    assert!(!db::load(&path, false)
        .unwrap()
        .items
        .iter()
        .any(|i| i.id == id));
    assert_eq!(content::capture(&path, input(true)).unwrap().ids, vec![id]);
    assert_eq!(db::load(&path, false).unwrap().items.len(), 40);
    let fresh = temp.path().join("restart.sqlite");
    db::init(&fresh).unwrap();
    db::export(&path, temp.path().join("restart.json")).unwrap();
    db::import(&fresh, temp.path().join("restart.json")).unwrap();
    assert_eq!(
        db::load(&fresh, false)
            .unwrap()
            .items
            .iter()
            .find(|i| i.path == "https://example.com/reference")
            .unwrap()
            .source,
        "Resources"
    );
}
#[test]
fn explorer_files_import_into_separate_sources_and_survive_download_deletion() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Design");
    fs::create_dir(&root).unwrap();
    let path = temp.path().join("files.sqlite");
    db::init(&path).unwrap();
    let conn = db::connect(&path).unwrap();
    db::set_setting(&conn, "root", &root.to_string_lossy()).unwrap();
    crate::sources::ensure(&conn).unwrap();
    crate::sources::ensure(&conn).unwrap();
    assert!(temp.path().join("Photography").is_dir());
    assert!(temp.path().join("Videos").is_dir());
    let download = temp.path().join("copied image.png");
    image::RgbImage::from_pixel(18, 22, image::Rgb([5, 200, 70]))
        .save(&download)
        .unwrap();
    let clipboard = content::clipboard_files(vec![download.clone(), download.clone()]).unwrap();
    assert_eq!(clipboard.files.len(), 1);
    assert_eq!(clipboard.files[0].name, "copied image.png");
    let first = content::capture(&path, capture_request(&download, vec![], vec![])).unwrap();
    let mut photo = capture_request(&download, vec![], vec![]);
    photo.source = "Photography".into();
    let second = content::capture(&path, photo).unwrap();
    assert_eq!(
        second.added, 1,
        "The same image can belong to a separate source"
    );
    assert_ne!(first.ids, second.ids);
    let mut photo_again = capture_request(&download, vec![], vec![]);
    photo_again.source = "Photography".into();
    assert_eq!(content::capture(&path, photo_again).unwrap().reused, 1);
    library::scan_all(&path).unwrap();
    let lib = db::load(&path, false).unwrap();
    assert!(
        lib.sources.contains(&"Videos".into()),
        "Empty sources stay visible"
    );
    assert_eq!(lib.source_paths.len(), 3);
    let photo_item = lib
        .items
        .iter()
        .find(|item| item.id == second.ids[0])
        .unwrap();
    assert_eq!(photo_item.source, "Photography/Captures");
    assert_eq!(
        fs::read(&photo_item.path).unwrap(),
        fs::read(&download).unwrap()
    );
    fs::remove_file(&download).unwrap();
    assert!(std::path::Path::new(&photo_item.path).is_file());
    assert!(content::clipboard_files(vec![temp.path().join("missing.png")]).is_err());
    assert!(content::clipboard_files(vec![root]).is_err());
}

#[test]
fn removed_items_keep_files_and_curation_and_stay_removed_after_rescan() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Design");
    fs::create_dir(&root).unwrap();
    let file = root.join("original.png");
    image::RgbImage::new(20, 30).save(&file).unwrap();
    let path = temp.path().join("remove.sqlite");
    db::init(&path).unwrap();
    library::scan(&path, &root).unwrap();
    let id = db::load(&path, false).unwrap().items[0].id;
    for name in ["Older project", "Recent project"] {
        db::change(
            &path,
            change("create_project", vec![], None, Some(name), None),
        )
        .unwrap();
    }
    let lib = db::load(&path, false).unwrap();
    let (a, b) = (lib.projects[0].id, lib.projects[1].id);
    for project in [a, b, a] {
        db::change(
            &path,
            change("add_project", vec![id], Some(project), None, None),
        )
        .unwrap();
    }
    assert_eq!(db::load(&path, false).unwrap().recent_projects, vec![a, b]);
    db::change(&path, change("pin_project", vec![id], Some(a), None, None)).unwrap();
    db::change(&path, change("remove_items", vec![id], None, None, None)).unwrap();
    library::scan(&path, &root).unwrap();
    assert!(file.is_file());
    assert!(db::load(&path, false).unwrap().items.is_empty());
    assert_eq!(db::removed_items(&path).unwrap()[0].id, id);
    db::change(&path, change("restore_items", vec![id], None, None, None)).unwrap();
    let restored = db::load(&path, false).unwrap();
    assert_eq!(restored.items[0].projects, vec![a, b]);
    assert_eq!(restored.items[0].favourites, vec![a]);
    assert!(db::removed_items(&path).unwrap().is_empty());
}

#[test]
fn source_migration_preserves_existing_ids_and_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Design");
    fs::create_dir_all(root.join("web design")).unwrap();
    image::RgbImage::new(20, 30)
        .save(root.join("web design/original.png"))
        .unwrap();
    let path = temp.path().join("migrate.sqlite");
    db::init(&path).unwrap();
    library::scan(&path, &root).unwrap();
    let before = db::load(&path, false).unwrap();
    let conn = db::connect(&path).unwrap();
    db::set_setting(&conn, "root", &root.to_string_lossy()).unwrap();
    crate::sources::ensure(&conn).unwrap();
    crate::sources::ensure(&conn).unwrap();
    library::scan_all(&path).unwrap();
    let after = db::load(&path, false).unwrap();
    assert_eq!(before.items[0].id, after.items[0].id);
    assert_eq!(before.items[0].path, after.items[0].path);
    assert_eq!(after.items[0].source, "Design/web design");
}

#[test]
#[ignore = "Reads a supplied local file and imports into a temporary library only"]
fn copied_file_import_smoke() {
    let source = std::path::PathBuf::from(
        std::env::var_os("DESIGN_VIEWER_TEST_FILE").expect("Set DESIGN_VIEWER_TEST_FILE"),
    );
    let original = fs::read(&source).unwrap();
    let draft = content::clipboard_files(vec![source.clone()]).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Design");
    fs::create_dir(&root).unwrap();
    let path = temp.path().join("smoke.sqlite");
    db::init(&path).unwrap();
    let conn = db::connect(&path).unwrap();
    db::set_setting(&conn, "root", &root.to_string_lossy()).unwrap();
    crate::sources::ensure(&conn).unwrap();
    let mut input = capture_request(&source, vec![], vec![]);
    input.files = draft.files;
    input.source = "Photography".into();
    content::capture(&path, input).unwrap();
    let item = db::load(&path, false).unwrap().items.remove(0);
    assert_eq!(item.source, "Photography/Captures");
    assert_eq!(original, fs::read(item.path).unwrap());
    assert_eq!(original, fs::read(source).unwrap());
}
fn change(
    action: &str,
    ids: Vec<i64>,
    target: Option<i64>,
    name: Option<&str>,
    value: Option<&str>,
) -> Change {
    Change {
        action: action.into(),
        ids,
        target,
        name: name.map(str::to_owned),
        value: value.map(str::to_owned),
    }
}
#[test]
fn scan_curate_restore_and_missing_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Design");
    fs::create_dir_all(root.join("mobile/components")).unwrap();
    let original = root.join("mobile/components/reference.png");
    image::RgbImage::new(20, 30).save(&original).unwrap();
    let dbpath = temp.path().join("library.sqlite");
    db::init(&dbpath).unwrap();
    library::scan(&dbpath, &root).unwrap();
    let first = db::load(&dbpath, false).unwrap();
    assert_eq!(first.tags.len(), 3);
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.sources, vec!["mobile", "mobile/components"]);
    let id = first.items[0].id;
    db::change(
        &dbpath,
        change("create_project", vec![], None, Some("Project one"), None),
    )
    .unwrap();
    let project = db::load(&dbpath, false).unwrap().projects[0].id;
    db::change(
        &dbpath,
        change("add_project", vec![id], Some(project), None, None),
    )
    .unwrap();
    db::change(
        &dbpath,
        change("add_tag", vec![id], Some(first.tags[0].id), None, None),
    )
    .unwrap();
    // Re-indexing preserves the item ID and all curation.
    library::scan(&dbpath, &root).unwrap();
    let curated = db::load(&dbpath, false).unwrap();
    assert_eq!(curated.items[0].id, id);
    assert_eq!(curated.items[0].projects, vec![project]);
    assert_eq!(curated.items[0].tags, vec![first.tags[0].id]);
    let cache = temp.path().join("cache");
    let thumb = library::media(
        &dbpath,
        &cache,
        std::path::Path::new("unused"),
        id,
        "thumbnail",
    )
    .unwrap();
    assert!(std::path::Path::new(&thumb).is_file());
    assert_eq!(image::image_dimensions(thumb).unwrap(), (20, 30));
    db::change(
        &dbpath,
        change(
            "create_link",
            vec![],
            Some(project),
            Some("Reference"),
            Some("https://example.com"),
        ),
    )
    .unwrap();
    let backup = temp.path().join("backup.json");
    db::export(&dbpath, backup.clone()).unwrap();
    let restored = temp.path().join("restored.sqlite");
    db::init(&restored).unwrap();
    db::import(&restored, backup.clone()).unwrap();
    db::import(&restored, backup).unwrap();
    let restored_lib = db::load(&restored, false).unwrap();
    assert_eq!(restored_lib.items.len(), 2);
    assert_eq!(restored_lib.projects[0].count, 2);
    assert_eq!(restored_lib.tags.len(), 3);
    fs::remove_file(&original).unwrap();
    library::scan(&dbpath, &root).unwrap();
    let missing = db::load(&dbpath, false).unwrap();
    assert!(missing.items.iter().find(|i| i.id == id).unwrap().missing);
    assert_eq!(missing.projects[0].count, 2);
    db::change(
        &dbpath,
        change("delete_project", vec![], Some(project), None, None),
    )
    .unwrap();
    let removed = db::load(&dbpath, false).unwrap();
    assert_eq!(removed.items.len(), 2);
    assert!(removed.items.iter().all(|i| i.projects.is_empty()));
}
#[test]
fn rejects_invalid_links_and_failed_scan_preserves_library() {
    let temp = tempfile::tempdir().unwrap();
    let dbpath = temp.path().join("library.sqlite");
    db::init(&dbpath).unwrap();
    assert!(db::change(
        &dbpath,
        change(
            "create_link",
            vec![],
            None,
            Some("Bad"),
            Some("javascript:alert(1)")
        )
    )
    .is_err());
    assert!(library::scan(&dbpath, &temp.path().join("missing")).is_err());
    assert!(db::load(&dbpath, false).unwrap().items.is_empty());
}
#[test]
#[ignore = "Run explicitly against the local inspiration folder"]
fn actual_library_media_smoke() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("Design");
    let ffmpeg = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries/ffmpeg.exe");
    let temp = tempfile::tempdir().unwrap();
    let dbpath = temp.path().join("smoke.sqlite");
    db::init(&dbpath).unwrap();
    let count = library::scan(&dbpath, &root).unwrap();
    assert!(count > 2800);
    let lib = db::load(&dbpath, false).unwrap();
    let mut seen = std::collections::HashSet::new();
    for item in &lib.items {
        let ext = std::path::Path::new(&item.path)
            .extension()
            .unwrap()
            .to_string_lossy()
            .to_lowercase();
        let key = format!("{ext}:{}", item.kind);
        if seen.insert(key.clone()) {
            let thumb = library::media(
                &dbpath,
                &temp.path().join("cache"),
                &ffmpeg,
                item.id,
                "thumbnail",
            )
            .unwrap_or_else(|e| panic!("{key}: {e}"));
            assert!(image::image_dimensions(thumb).is_ok());
            if item.kind == "video" || item.kind == "animation" {
                let preview = library::media(
                    &dbpath,
                    &temp.path().join("cache"),
                    &ffmpeg,
                    item.id,
                    "preview",
                )
                .unwrap_or_else(|e| panic!("{key}: {e}"));
                assert!(fs::metadata(preview).unwrap().len() > 100);
            }
            eprintln!("Decoded {key}");
        }
    }
    eprintln!(
        "Indexed {count} files; {} animated WebPs",
        lib.items
            .iter()
            .filter(|i| i.kind == "animation" && i.path.ends_with(".webp"))
            .count()
    );
}

fn capture_request(path: &std::path::Path, projects: Vec<i64>, tags: Vec<i64>) -> content::Capture {
    content::Capture {
        destination: String::new(),
        thumbnail: None,
        resource: false,
        source: String::new(),
        files: vec![content::CaptureFile {
            path: path.to_string_lossy().into_owned(),
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
        }],
        url: String::new(),
        name: "Saved reference".into(),
        source_url: "https://example.com/design".into(),
        projects,
        tags,
    }
}
#[test]
fn captures_reuse_pixels_and_preserve_project_pins_in_backup() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Design");
    fs::create_dir(&root).unwrap();
    let path = temp.path().join("capture.sqlite");
    db::init(&path).unwrap();
    let dbconn = db::connect(&path).unwrap();
    db::set_setting(&dbconn, "root", &root.to_string_lossy()).unwrap();
    drop(dbconn);
    let original = temp.path().join("original.png");
    let renamed = temp.path().join("completely-different-name.bmp");
    let image = image::RgbImage::from_pixel(28, 40, image::Rgb([40, 90, 160]));
    image.save(&original).unwrap();
    image.save(&renamed).unwrap();
    db::change(
        &path,
        change("create_project", vec![], None, Some("One"), None),
    )
    .unwrap();
    db::change(
        &path,
        change("create_project", vec![], None, Some("Two"), None),
    )
    .unwrap();
    let lib = db::load(&path, false).unwrap();
    let project = lib.projects[0].id;
    let tag = lib.tags[0].id;
    let first =
        content::capture(&path, capture_request(&original, vec![project], vec![tag])).unwrap();
    assert_eq!(first.added, 1);
    assert_eq!(first.reused, 0);
    let id = first.ids[0];
    assert!(original.exists());
    let second = content::capture(
        &path,
        capture_request(&renamed, vec![lib.projects[1].id], vec![]),
    )
    .unwrap();
    assert_eq!(second.reused, 1);
    assert_eq!(second.ids, vec![id]);
    assert_eq!(fs::read_dir(root.join("Captures")).unwrap().count(), 1);
    db::change(
        &path,
        change("pin_project", vec![id], Some(project), None, None),
    )
    .unwrap();
    let saved = db::load(&path, false).unwrap();
    assert_eq!(saved.items[0].favourites, vec![project]);
    assert_eq!(saved.items[0].projects.len(), 2);
    assert_eq!(saved.items[0].source_url, "https://example.com/design");
    let backup = temp.path().join("backup.json");
    db::export(&path, backup.clone()).unwrap();
    let restored = temp.path().join("restore.sqlite");
    db::init(&restored).unwrap();
    db::import(&restored, backup).unwrap();
    assert_eq!(
        db::load(&restored, false).unwrap().items[0]
            .favourites
            .len(),
        1
    );
    let mut link = capture_request(&original, vec![project], vec![tag]);
    link.files.clear();
    link.url = "https://EXAMPLE.com".into();
    let first = content::capture(&path, link).unwrap();
    assert_eq!(first.added, 1);
    let mut link = capture_request(&original, vec![], vec![]);
    link.files.clear();
    link.url = "https://example.com/".into();
    assert_eq!(content::capture(&path, link).unwrap().reused, 1);
}
#[test]
fn duplicates_merge_curation_and_can_be_restored() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Design");
    fs::create_dir(&root).unwrap();
    let path = temp.path().join("dupes.sqlite");
    db::init(&path).unwrap();
    let image = image::RgbImage::from_pixel(18, 22, image::Rgb([5, 200, 70]));
    image.save(root.join("one.png")).unwrap();
    image.save(root.join("two.bmp")).unwrap();
    fs::write(
        root.join("clip.mp4"),
        b"same video contents with different filenames",
    )
    .unwrap();
    fs::write(
        root.join("renamed.mov"),
        b"same video contents with different filenames",
    )
    .unwrap();
    library::scan(&path, &root).unwrap();
    let lib = db::load(&path, false).unwrap();
    let kept = lib
        .items
        .iter()
        .find(|i| i.path.ends_with("one.png"))
        .unwrap()
        .id;
    let removed = lib
        .items
        .iter()
        .find(|i| i.path.ends_with("two.bmp"))
        .unwrap()
        .id;
    let scan = content::duplicates(&path).unwrap();
    assert_eq!(scan.groups.len(), 2);
    assert!(scan
        .groups
        .iter()
        .any(|g| g.contains(&kept) && g.contains(&removed)));
    db::change(
        &path,
        change("create_project", vec![], None, Some("Pinned project"), None),
    )
    .unwrap();
    let project = db::load(&path, false).unwrap().projects[0].id;
    db::change(
        &path,
        change("add_project", vec![removed], Some(project), None, None),
    )
    .unwrap();
    db::change(
        &path,
        change("pin_project", vec![removed], Some(project), None, None),
    )
    .unwrap();
    db::change(
        &path,
        change("add_tag", vec![removed], Some(lib.tags[0].id), None, None),
    )
    .unwrap();
    assert_eq!(
        content::remove_duplicates(&path, temp.path(), kept, vec![removed]).unwrap(),
        1
    );
    assert!(!root.join("two.bmp").exists());
    assert!(root.join("one.png").exists());
    let current = db::load(&path, false).unwrap();
    assert_eq!(current.items.len(), 3);
    let retained = current.items.iter().find(|i| i.id == kept).unwrap();
    assert_eq!(retained.favourites, vec![project]);
    assert_eq!(retained.tags, vec![lib.tags[0].id]);
    assert_eq!(current.projects[0].count, 1);
    assert_eq!(content::recovery(&path).unwrap().len(), 1);
    content::restore(&path, removed).unwrap();
    assert!(root.join("two.bmp").exists());
    assert_eq!(db::load(&path, false).unwrap().items.len(), 4);
    assert!(content::recovery(&path).unwrap().is_empty());
    // A changed duplicate must not be removed using a stale match.
    image::RgbImage::from_pixel(18, 22, image::Rgb([250, 0, 0]))
        .save(root.join("two.bmp"))
        .unwrap();
    assert!(content::remove_duplicates(&path, temp.path(), kept, vec![removed]).is_err());
    assert!(root.join("two.bmp").exists());
}

#[test]
fn duplicate_progress_covers_empty_cached_and_unavailable_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Design");
    fs::create_dir(&root).unwrap();
    let path = temp.path().join("progress.sqlite");
    db::init(&path).unwrap();
    let mut events = Vec::new();
    let empty = content::duplicates_with_progress(&path, &mut |p| events.push(p)).unwrap();
    assert!(empty.groups.is_empty());
    assert_eq!(events.last().unwrap().phase, "grouping");
    assert_eq!(events.last().unwrap().total, 0);

    let image = image::RgbImage::from_pixel(18, 22, image::Rgb([5, 200, 70]));
    image.save(root.join("one.png")).unwrap();
    image.save(root.join("copy.bmp")).unwrap();
    image.save(root.join("unavailable.png")).unwrap();
    library::scan(&path, &root).unwrap();
    fs::remove_file(root.join("unavailable.png")).unwrap();
    for _ in 0..2 {
        events.clear();
        let scan = content::duplicates_with_progress(&path, &mut |p| events.push(p)).unwrap();
        assert_eq!(scan.groups.len(), 1);
        assert_eq!(scan.skipped, 1);
        assert_eq!(events.first().unwrap().checked, 0);
        assert!(events.iter().any(|p| !p.current.is_empty()));
        assert!(events
            .windows(2)
            .all(|pair| pair[0].checked <= pair[1].checked));
        let last = events.last().unwrap();
        assert_eq!((last.checked, last.total), (3, 3));
        assert_eq!(last.phase, "grouping");
    }
}
#[test]
fn chunked_capture_stays_in_staging_and_preserves_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let entry = content::stage_begin(temp.path(), "Unicode 名称.png").unwrap();
    let token = std::path::Path::new(&entry.path)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    content::stage_chunk(temp.path(), token, b"first").unwrap();
    content::stage_chunk(temp.path(), token, b"second").unwrap();
    assert_eq!(fs::read(entry.path).unwrap(), b"firstsecond");
    assert!(content::stage_chunk(temp.path(), "../outside.png", b"bad").is_err());
    assert!(content::stage_begin(temp.path(), "unsupported.exe").is_err());
}

#[test]
fn thumbnail_collections_capture_reuse_and_backup_without_media_import() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("boards.sqlite");
    db::init(&path).unwrap();
    assert!(db::load(&path, false).unwrap().items.is_empty());
    let original = temp.path().join("pasted.png");
    image::RgbaImage::from_pixel(1600, 1000, image::Rgba([12,34,56,78])).save(&original).unwrap();
    db::change(&path, change("create_project",vec![],None,Some("Website"),None)).unwrap();
    let lib=db::load(&path,false).unwrap();
    let project=lib.projects[0].id;
    let tag=lib.tags[0].id;
    for (destination, source, url) in [("fonts","Fonts","https://fonts.example/typeface"),("codepen","CodePen","https://codepen.io/example/pen/123"),("brand-guidelines","Brand Guidelines","https://brand.example/guidelines")] {
        let mut c=capture_request(&original,vec![project],vec![tag]);
        c.files.clear();c.source_url.clear();c.url=url.into();c.destination=destination.into();
        c.thumbnail=Some(content::CaptureFile{path:original.to_string_lossy().into_owned(),name:"pasted.png".into()});
        let result=content::capture(&path,c).unwrap();
        assert_eq!(result.destination,Some(destination));
        let lib=db::load(&path,false).unwrap();
        let item=lib.items.iter().find(|i|i.id==result.ids[0]).unwrap();
        assert_eq!(item.source,source);assert_eq!(item.kind,"link");
        assert_eq!(item.projects,vec![project]);assert_eq!(item.tags,vec![tag]);
        let thumbnail=image::open(&item.thumbnail).unwrap();
        assert_eq!((thumbnail.width(),thumbnail.height()),(1600,1000));
        assert_eq!(thumbnail.to_rgba8().get_pixel(0,0).0,[12,34,56,78]);
        assert!(std::path::Path::new(&item.thumbnail).starts_with(temp.path().join("link-thumbnails")));
        let mut c=capture_request(&original,vec![],vec![]);c.files.clear();c.source_url.clear();c.url=url.into();c.destination=destination.into();
        assert_eq!(content::capture(&path,c).unwrap().reused,1);
    }
    assert_eq!(db::load(&path,false).unwrap().items.len(),3);
    fs::remove_file(&original).unwrap();
    let backup=temp.path().join("boards-backup.json");db::export(&path,backup.clone()).unwrap();
    let restored=temp.path().join("restored.sqlite");db::init(&restored).unwrap();db::import(&restored,backup).unwrap();
    let lib=db::load(&restored,false).unwrap();
    assert_eq!(lib.items.len(),3);
    assert!(lib.items.iter().any(|i|i.source=="Fonts"));
    for item in &lib.items {assert!(!item.thumbnail.is_empty());assert!(std::path::Path::new(&item.thumbnail).is_file());assert_eq!(item.projects.len(),1);assert_eq!(item.tags.len(),1);}
    assert!(lib.items.iter().any(|i|i.source=="CodePen"));assert!(lib.items.iter().any(|i|i.source=="Brand Guidelines"));
    let mut invalid=capture_request(temp.path(),vec![],vec![]);invalid.files.clear();invalid.url="https://valid.example/".into();invalid.destination="unknown".into();
    assert!(content::capture(&path,invalid).is_err());
}

#[test]
fn video_download_progress_and_url_normalization() {
    use crate::download;
    assert_eq!(download::normalize(" https://www.facebook.com/watch/?ref=saved&v=1459999332693030 ").unwrap(),"https://www.facebook.com/watch/?v=1459999332693030");
    assert!(download::normalize("file:///private.mp4").is_err());
    assert!(download::normalize("https://username:secret@example.com/video").is_err());
    let p=download::progress(r#"DV_PROGRESS:{"status":"downloading","downloaded_bytes":50,"total_bytes":200,"speed":25.5,"eta":6.2}"#).unwrap();
    assert_eq!(p.percent,Some(25.));assert_eq!(p.total,Some(200));assert_eq!(p.eta,Some(7));
    let p=download::progress(r#"DV_PROGRESS:{"status":"downloading","downloaded_bytes":50,"total_bytes":null,"total_bytes_estimate":100,"eta":null}"#).unwrap();
    assert_eq!(p.percent,Some(50.));assert_eq!(p.eta,None);
    assert_eq!(download::progress(r#"DV_PROGRESS:{"status":"finished","downloaded_bytes":50}"#).unwrap().phase,"processing");
    assert!(download::progress("unstructured output").is_none());
}
#[test]
fn downloaded_video_is_copied_curated_and_reused_after_tracking_changes() {
    use crate::download;
    let temp=tempfile::tempdir().unwrap();let root=temp.path().join("Videos");fs::create_dir(&root).unwrap();
    let path=temp.path().join("library.sqlite");db::init(&path).unwrap();
    let dbconn=db::connect(&path).unwrap();db::set_setting(&dbconn,"root_videos",&root.to_string_lossy()).unwrap();
    db::change(&path,change("create_project",vec![],None,Some("Website"),None)).unwrap();
    let lib=db::load(&path,false).unwrap();let project=lib.projects[0].id;let tag=lib.tags[0].id;
    let source=temp.path().join("download.mp4");fs::write(&source,b"temporary downloaded video bytes").unwrap();
    let url=download::normalize("https://www.facebook.com/watch/?ref=saved&v=123").unwrap();
    let request=download::Request{url,projects:vec![project],tags:vec![tag]};
    let saved=download::import(&path,&root.canonicalize().unwrap(),&source,"Example video",&request).unwrap();fs::remove_file(source).unwrap();
    let lib=db::load(&path,false).unwrap();let item=&lib.items[0];
    assert_eq!(item.kind,"video");assert_eq!(item.source,"Videos/Downloads");assert_eq!(item.name,"Example video");
    assert_eq!(item.projects,vec![project]);assert_eq!(item.tags,vec![tag]);assert!(std::path::Path::new(&item.path).is_file());
    let request=download::Request{url:download::normalize("https://m.facebook.com/watch/?v=123&ref=other").unwrap(),projects:vec![],tags:vec![]};
    assert_eq!(download::existing(&path,&request).unwrap().unwrap().id,saved.id);
    assert_eq!(db::load(&path,false).unwrap().items.len(),1);
    db::change(&path,change("remove_items",vec![saved.id],None,None,None)).unwrap();
    assert!(download::existing(&path,&request).unwrap().unwrap().reused);
    let lib=db::load(&path,false).unwrap();assert_eq!(lib.items[0].tags,vec![tag]);assert_eq!(lib.items[0].projects,vec![project]);
}
#[test]
#[ignore = "Downloads only the explicitly supplied URL; optionally adds it to the explicitly supplied library"]
fn requested_video_download_smoke() {
    use crate::download;
    let url=std::env::var("DESIGN_DOWNLOAD_SMOKE_URL").expect("Set the requested video URL");
    let temp=tempfile::tempdir().unwrap();
    let supplied=std::env::var_os("DESIGN_DOWNLOAD_SMOKE_DB");
    let path=supplied.clone().map(std::path::PathBuf::from).unwrap_or_else(||temp.path().join("library.sqlite"));
    db::init(&path).unwrap();
    let conn=db::connect(&path).unwrap();
    if supplied.is_some(){crate::sources::ensure(&conn).unwrap();}else{let root=temp.path().join("Videos");fs::create_dir(&root).unwrap();db::set_setting(&conn,"root_videos",&root.to_string_lossy()).unwrap();}
    let binaries=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
    let mut events=vec![];
    let saved=download::run(&path,&binaries.join("yt-dlp.exe"),&binaries.join("ffmpeg.exe"),download::Request{url,projects:vec![],tags:vec![]},&std::sync::atomic::AtomicBool::new(false),&mut |p|events.push(p)).unwrap();
    let lib=db::load(&path,false).unwrap();let item=lib.items.iter().find(|i|i.id==saved.id).unwrap();
    assert!(std::path::Path::new(&item.path).is_file());assert_eq!(item.kind,"video");assert_eq!(item.source,"Videos/Downloads");
    let cache=path.parent().unwrap().join("cache");fs::create_dir_all(&cache).unwrap();
    for mode in ["thumbnail","preview"] {let media=library::media(&path,&cache,&binaries.join("ffmpeg.exe"),saved.id,mode).unwrap();assert!(std::path::Path::new(&media).is_file());}
    if !saved.reused {assert!(events.iter().any(|p|p.phase=="downloading"));}
    println!("Saved item {} to {}; reused={}; progress events={}; thumbnail and hover preview generated",saved.id,item.path,saved.reused,events.len());
}

#[test]
fn cancelled_video_download_stops_process_and_cleans_staging() {
    let temp=tempfile::tempdir().unwrap();let path=temp.path().join("library.sqlite");db::init(&path).unwrap();let root=temp.path().join("Videos");fs::create_dir(&root).unwrap();
    db::set_setting(&db::connect(&path).unwrap(),"root_videos",&root.to_string_lossy()).unwrap();
    let binaries=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
    let result=crate::download::run(&path,&binaries.join("yt-dlp.exe"),&binaries.join("ffmpeg.exe"),crate::download::Request{url:"https://example.com/cancel-before-connect".into(),projects:vec![],tags:vec![]},&std::sync::atomic::AtomicBool::new(true),&mut |_|{});
    assert_eq!(result.err().unwrap(),"Download cancelled.");assert!(db::load(&path,false).unwrap().items.is_empty());assert_eq!(fs::read_dir(temp.path().join("video-downloads")).unwrap().count(),0);
}

#[test]
fn photography_category_migrates_existing_photos_once_and_preserves_curation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("library.sqlite");
    db::init(&path).unwrap();
    let conn = db::connect(&path).unwrap();
    conn.execute_batch("DELETE FROM settings WHERE key='photography_category_migrated';
      INSERT INTO items(id,path,name,kind,source,added) VALUES
        (1,'photo-1.png','First photo','image','Photography',1),
        (2,'photo-2.png','Second photo','image','Photography/Captures',2),
        (3,'curated.png','Already curated','image','Photography',3),
        (4,'design.png','Design reference','image','Design/branding',4),
        (5,'https://example.com','Link','link','Photography',5);
      INSERT INTO item_categories(item_id,name) VALUES(2,''),(3,'Branding');
      INSERT INTO projects(id,name) VALUES(7,'Keep project');
      INSERT INTO tags(id,name) VALUES(9,'Keep tag');
      INSERT INTO item_projects(item_id,label_id,favourite) VALUES(1,7,1);
      INSERT INTO item_tags(item_id,label_id) VALUES(1,9);").unwrap();
    db::init(&path).unwrap();
    let lib = db::load(&path,false).unwrap();
    assert!(lib.categories.iter().any(|name|name=="Photography"));
    for id in [1,2] { assert_eq!(lib.items.iter().find(|i|i.id==id).unwrap().category.as_deref(),Some("Photography")); }
    let first = lib.items.iter().find(|i|i.id==1).unwrap();
    assert_eq!(first.path,"photo-1.png");
    assert_eq!(first.projects,vec![7]); assert_eq!(first.tags,vec![9]); assert_eq!(first.favourites,vec![7]);
    assert_eq!(lib.items.iter().find(|i|i.id==3).unwrap().category.as_deref(),Some("Branding"));
    assert_eq!(lib.items.iter().find(|i|i.id==4).unwrap().category.as_deref(),Some("branding"));
    assert_eq!(lib.items.iter().find(|i|i.id==5).unwrap().category,None);
    db::change(&path,change("set_category",vec![1],None,None,Some(""))).unwrap();
    db::init(&path).unwrap();
    assert_eq!(db::load(&path,false).unwrap().items.iter().find(|i|i.id==1).unwrap().category.as_deref(),Some(""));
    assert_eq!(db::default_category("Photography/Captures"),"Photography");
}

#[test]
fn photography_resources_upgrade_existing_library_once_without_overwriting_saved_links() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("resources.sqlite"); db::init(&path).unwrap();
    let conn = db::connect(&path).unwrap();
    db::set_setting(&conn,"resources_seeded","1").unwrap();
    db::set_setting(&conn,"pattern_texture_resources_seeded","1").unwrap();
    conn.execute_batch("INSERT INTO items(path,name,kind,source,added,archived) VALUES
      ('https://unsplash.com/','My saved name','link','Resources/Custom',1,0),
      ('https://www.lensculture.com/','Removed resource','link','Resources/Photography',2,1),
      ('https://example.com/my-resource','Existing resource','link','Resources/Websites',3,0);").unwrap();
    crate::resources::seed(&conn).unwrap(); crate::resources::seed(&conn).unwrap();
    let loaded = db::load(&path,false).unwrap();
    assert_eq!(loaded.items.len(),7);
    assert_eq!(loaded.items.iter().filter(|i|i.source=="Resources/Photography").count(),5);
    let saved=loaded.items.iter().find(|i|i.path=="https://unsplash.com/").unwrap();
    assert_eq!(saved.name,"My saved name"); assert_eq!(saved.source,"Resources/Custom");
    assert!(!loaded.items.iter().any(|i|i.path=="https://www.lensculture.com/"));
    assert!(loaded.items.iter().any(|i|i.name=="Existing resource"));
    let removed=loaded.items.iter().find(|i|i.source=="Resources/Photography").unwrap().id;
    db::change(&path,change("remove_items",vec![removed],None,None,None)).unwrap();
    crate::resources::seed(&conn).unwrap();
    assert!(!db::load(&path,false).unwrap().items.iter().any(|i|i.id==removed));
}

#[test]
fn organisation_history_redo_roundtrips_all_actions_and_survives_restarts() {
    let temp=tempfile::tempdir().unwrap();let path=temp.path().join("history.sqlite");db::init(&path).unwrap();
    let conn=db::connect(&path).unwrap();
    conn.execute_batch("INSERT INTO items(id,path,name,kind,source,added) VALUES
      (1,'media-1.png','First','image','Design/3d',1),
      (2,'media-2.png','Second','image','Design/3d',2),
      (3,'https://old.example/','Old link','link','',3);").unwrap();
    let state=|| {
        let mut value=serde_json::to_value(db::load(&path,false).unwrap()).unwrap();
        for key in ["undo_label","undo_count","redo_label","redo_count","recent_projects"] {value.as_object_mut().unwrap().remove(key);}
        value
    };
    let mut states=vec![state()];
    let mut apply=|c| {db::change(&path,c).unwrap();states.push(state());};
    apply(change("create_category",vec![],None,Some("Gallery"),None));
    apply(change("set_category",vec![1,2],None,None,Some("Gallery")));
    apply(change("create_project",vec![],None,Some("Board"),None));
    let project=db::load(&path,false).unwrap().projects[0].id;
    apply(change("create_tag",vec![],None,Some("Custom tag"),None));
    let tag=db::load(&path,false).unwrap().tags.iter().find(|t|t.name=="Custom tag").unwrap().id;
    apply(change("add_project",vec![1,2],Some(project),None,None));
    apply(change("pin_project",vec![1],Some(project),None,None));
    apply(change("add_tag",vec![1,2],Some(tag),None,None));
    apply(change("remove_project",vec![2],Some(project),None,None));
    apply(change("remove_tag",vec![2],Some(tag),None,None));
    apply(change("rename_project",vec![],Some(project),Some("Renamed board"),None));
    apply(change("rename_tag",vec![],Some(tag),Some("Renamed tag"),None));
    apply(change("edit_link",vec![],Some(3),Some("Edited link"),Some("https://new.example/")));
    apply(change("set_link_destination",vec![],Some(3),None,Some("resources")));
    apply(change("set_source_url",vec![],Some(1),None,Some("https://source.example/")));
    apply(change("remove_items",vec![2],None,None,None));
    apply(change("restore_items",vec![2],None,None,None));
    apply(change("delete_project",vec![],Some(project),None,None));
    apply(change("delete_tag",vec![],Some(tag),None,None));
    let steps=states.len()-1;
    for _ in 0..3 {
        for n in (0..steps).rev() {
            db::change(&path,change("undo_organisation",vec![],None,None,None)).unwrap();
            assert_eq!(state(),states[n],"Undo step {n}");
        }
        assert_eq!(db::load(&path,false).unwrap().redo_count,steps);
        db::init(&path).unwrap();
        for n in 0..steps {
            db::change(&path,change("redo_organisation",vec![],None,None,None)).unwrap();
            assert_eq!(state(),states[n+1],"Redo step {n}");
        }
        assert_eq!(db::load(&path,false).unwrap().redo_count,0);
    }
    db::change(&path,change("undo_organisation",vec![],None,None,None)).unwrap();
    let redo_count=db::load(&path,false).unwrap().redo_count;
    db::change(&path,change("set_category",vec![1],None,None,Some("Gallery"))).unwrap();
    assert_eq!(db::load(&path,false).unwrap().redo_count,redo_count,"No-op keeps Redo");
    assert!(db::change(&path,change("set_category",vec![1,999999],None,None,Some("Wrong"))).is_err());
    assert_eq!(db::load(&path,false).unwrap().redo_count,redo_count,"Failed edit keeps Redo");
    db::change(&path,change("set_category",vec![1],None,None,Some("New branch"))).unwrap();
    assert_eq!(db::load(&path,false).unwrap().redo_count,0);
    assert!(db::change(&path,change("redo_organisation",vec![],None,None,None)).is_err());
    db::change(&path,change("undo_organisation",vec![],None,None,None)).unwrap();
    let backup=temp.path().join("backup.json");db::export(&path,backup.clone()).unwrap();db::import(&path,backup).unwrap();
    let loaded=db::load(&path,false).unwrap();assert_eq!(loaded.redo_count,0);assert_eq!(loaded.undo_count,0);
}

#[test]
fn organisation_history_failed_redo_keeps_both_stacks_atomic() {
    let temp=tempfile::tempdir().unwrap();let path=temp.path().join("history.sqlite");db::init(&path).unwrap();
    db::change(&path,change("create_project",vec![],None,Some("Board"),None)).unwrap();
    db::change(&path,change("undo_organisation",vec![],None,None,None)).unwrap();
    let conn=db::connect(&path).unwrap();conn.execute("INSERT INTO projects(name) VALUES('Board')",[]).unwrap();
    let before=db::load(&path,false).unwrap();
    assert!(db::change(&path,change("redo_organisation",vec![],None,None,None)).is_err());
    let after=db::load(&path,false).unwrap();
    assert_eq!(before.undo_count,after.undo_count);assert_eq!(before.redo_count,after.redo_count);assert_eq!(before.redo_label,after.redo_label);
    assert_eq!(after.projects.len(),1);
}

#[test]
fn pattern_texture_resources_upgrade_once_and_preserve_user_changes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("resources.sqlite");
    db::init(&path).unwrap();
    let conn = db::connect(&path).unwrap();
    for key in ["resources_seeded", "photography_resources_seeded"] {
        db::set_setting(&conn, key, "1").unwrap();
    }
    conn.execute_batch("INSERT INTO items(path,name,kind,source,added,archived) VALUES
      ('https://heropatterns.com/','My patterns','link','Resources/Custom',1,0),
      ('https://texturelabs.org/','Removed textures','link','Resources/Patterns & Textures',2,1),
      ('https://pattern.monster/','Saved pattern','link','',3,0);").unwrap();
    crate::resources::seed(&conn).unwrap();
    crate::resources::seed(&conn).unwrap();
    let loaded = db::load(&path, false).unwrap();
    assert_eq!(loaded.items.len(), 22);
    assert_eq!(loaded.items.iter().filter(|i| i.source == "Resources/Patterns & Textures").count(), 21);
    let saved = loaded.items.iter().find(|i| i.path == "https://heropatterns.com/").unwrap();
    assert_eq!(saved.name, "My patterns");
    assert_eq!(saved.source, "Resources/Custom");
    assert!(!loaded.items.iter().any(|i| i.path == "https://texturelabs.org/"));
    let promoted = loaded.items.iter().find(|i| i.path == "https://pattern.monster/").unwrap();
    assert_eq!(promoted.name, "Saved pattern");
    assert_eq!(promoted.source, "Resources/Patterns & Textures");
    db::change(&path, change("remove_items", vec![promoted.id], None, None, None)).unwrap();
    crate::resources::seed(&conn).unwrap();
    assert!(!db::load(&path, false).unwrap().items.iter().any(|i| i.id == promoted.id));
}
