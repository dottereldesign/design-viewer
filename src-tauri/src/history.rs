use crate::db::{self, err, Change, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum Patch {
    Category(i64, Option<String>),
    Member(bool, i64, i64, Option<i64>),
    Archived(i64, bool),
    Link(i64, String, String),
    Source(i64, String),
    SourceUrl(i64, String),
    Rename(bool, i64, String),
    Created(bool, String),
    Deleted(bool, i64, String, Vec<(i64, i64)>),
    CreatedCategory(String),
    DeletedCategory(String),
    RemoveLabel(bool, i64),
}

pub fn snapshot(db: &Connection, c: &Change) -> Result<Option<(String, Vec<Patch>)>> {
    let mut patches = vec![];
    let value = c.value.as_deref().unwrap_or("").trim();
    let target = c.target.unwrap_or(0);
    let label = match c.action.as_str() {
        "set_category" => {
            let mut ids=c.ids.clone();ids.sort();ids.dedup();
            for id in ids {
                let source: Option<String> = db.query_row("SELECT source FROM items WHERE id=? AND kind!='link'",[id],|r|r.get(0)).optional().map_err(err)?;
                if let Some(source)=source {
                    let previous: Option<String>=db.query_row("SELECT name FROM item_categories WHERE item_id=?",[id],|r|r.get(0)).optional().map_err(err)?;
                    if previous.clone().unwrap_or_else(||db::default_category(&source))!=value {patches.push(Patch::Category(id,previous));}
                }
            }
            "Change category"
        }
        "add_project"|"remove_project"|"add_tag"|"remove_tag"|"pin_project"|"unpin_project" => {
            let project=c.action.ends_with("project");
            let table=if project{"item_projects"}else{"item_tags"};
            let mut ids=c.ids.clone();ids.sort();ids.dedup();
            for id in ids {
                let sql=if project {format!("SELECT favourite FROM {table} WHERE item_id=? AND label_id=?")}else{format!("SELECT 0 FROM {table} WHERE item_id=? AND label_id=?")};
                let previous: Option<i64>=db.query_row(&sql,params![id,target],|r|r.get(0)).optional().map_err(err)?;
                let changed=match c.action.as_str(){"pin_project"=>previous==Some(0),"unpin_project"=>previous==Some(1),_=>if c.action.starts_with("add"){previous.is_none()}else{previous.is_some()}};
                if changed { patches.push(Patch::Member(project,id,target,previous)); }
            }
            match c.action.as_str(){"add_project"=>"Add to project","remove_project"=>"Remove from project","add_tag"=>"Add tag","remove_tag"=>"Remove tag","pin_project"=>"Pin reference",_=>"Unpin reference"}
        }
        "remove_items"|"restore_items" => {
            let mut ids=c.ids.clone();ids.sort();ids.dedup();
            for id in ids {
                let previous:Option<bool>=db.query_row("SELECT archived FROM items WHERE id=? AND NOT EXISTS(SELECT 1 FROM duplicate_recovery WHERE item_id=items.id)",[id],|r|r.get(0)).optional().map_err(err)?;
                if let Some(previous)=previous {if previous!=(c.action=="remove_items"){patches.push(Patch::Archived(id,previous));}}
            }
            if c.action=="remove_items"{"Remove from library"}else{"Restore references"}
        }
        "edit_link" => {
            if let Some((name,path))=db.query_row("SELECT name,path FROM items WHERE id=? AND kind='link'",[target],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional().map_err(err)? {
                if name!=c.name.as_deref().unwrap_or("")||path!=value {patches.push(Patch::Link(target,name,path));}
            }
            "Edit link"
        }
        "set_link_destination" => {
            if let Some(source)=db.query_row("SELECT source FROM items WHERE id=? AND kind='link'",[target],|r|r.get::<_,String>(0)).optional().map_err(err)? {patches.push(Patch::Source(target,source));}
            "Move link"
        }
        "set_source_url" => {
            if let Some(url)=db.query_row("SELECT source_url FROM items WHERE id=?",[target],|r|r.get::<_,String>(0)).optional().map_err(err)? {if url!=value{patches.push(Patch::SourceUrl(target,url));}}
            "Change source link"
        }
        "rename_project"|"rename_tag"|"delete_project"|"delete_tag" => {
            let project=c.action.ends_with("project");let table=if project{"projects"}else{"tags"};
            if let Some(name)=db.query_row(&format!("SELECT name FROM {table} WHERE id=?"),[target],|r|r.get::<_,String>(0)).optional().map_err(err)? {
                if c.action.starts_with("rename") {if name!=c.name.as_deref().unwrap_or("").trim(){patches.push(Patch::Rename(project,target,name));}}
                else {
                    let join=if project{"item_projects"}else{"item_tags"};
                    let mut stmt=db.prepare(&format!("SELECT item_id,{} FROM {join} WHERE label_id=?",if project{"favourite"}else{"0"})).map_err(err)?;
                    let rows=stmt.query_map([target],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
                    let members=rows.collect::<std::result::Result<Vec<_>,_>>().map_err(err)?;
                    patches.push(Patch::Deleted(project,target,name,members));
                }
            }
            match c.action.as_str(){"rename_project"=>"Rename project","rename_tag"=>"Rename tag","delete_project"=>"Delete project",_=>"Delete tag"}
        }
        "create_project"|"create_tag" => {
            patches.push(Patch::Created(c.action.ends_with("project"),c.name.as_deref().unwrap_or("").trim().to_owned()));
            if c.action.ends_with("project"){"Create project"}else{"Create tag"}
        }
        "create_category" => {
            let name=c.name.as_deref().unwrap_or("").trim();
            let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM categories WHERE name=?)",[name],|r|r.get(0)).map_err(err)?;
            if !exists {patches.push(Patch::CreatedCategory(name.to_owned()));}
            "Create category"
        }
        _=>return Ok(None),
    };
    Ok(if patches.is_empty(){None}else{Some((label.to_owned(),patches))})
}

pub fn record(db:&Connection,label:&str,patches:Vec<Patch>)->Result<()> {
    db.execute("DELETE FROM organisation_redo",[]).map_err(err)?;
    db.execute("INSERT INTO organisation_history(label,patches) VALUES(?,?)",params![label,serde_json::to_string(&patches).map_err(err)?]).map_err(err)?;
    db.execute("DELETE FROM organisation_history WHERE id NOT IN (SELECT id FROM organisation_history ORDER BY id DESC LIMIT 100)",[]).map_err(err)?;
    Ok(())
}

pub fn undo(db:&Connection)->Result<()> { transfer(db, false) }
pub fn redo(db:&Connection)->Result<()> { transfer(db, true) }

// Save the current value before applying each stored patch. The inverse can then
// travel back to the other stack without replaying commands or changing IDs.
fn inverse(db: &Connection, patch: &Patch) -> Result<Patch> {
    Ok(match patch {
        Patch::Category(id,_) => Patch::Category(*id,db.query_row("SELECT name FROM item_categories WHERE item_id=?",[id],|r|r.get(0)).optional().map_err(err)?),
        Patch::Member(project,id,label,_) => {
            let query=if *project {"SELECT favourite FROM item_projects WHERE item_id=? AND label_id=?"} else {"SELECT 0 FROM item_tags WHERE item_id=? AND label_id=?"};
            Patch::Member(*project,*id,*label,db.query_row(query,params![id,label],|r|r.get(0)).optional().map_err(err)?)
        },
        Patch::Archived(id,_) => Patch::Archived(*id,db.query_row("SELECT archived FROM items WHERE id=?",[id],|r|r.get(0)).map_err(err)?),
        Patch::Link(id,_,_) => {
            let (name,path)=db.query_row("SELECT name,path FROM items WHERE id=?",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
            Patch::Link(*id,name,path)
        },
        Patch::Source(id,_) => Patch::Source(*id,db.query_row("SELECT source FROM items WHERE id=?",[id],|r|r.get(0)).map_err(err)?),
        Patch::SourceUrl(id,_) => Patch::SourceUrl(*id,db.query_row("SELECT source_url FROM items WHERE id=?",[id],|r|r.get(0)).map_err(err)?),
        Patch::Rename(project,id,_) => Patch::Rename(*project,*id,db.query_row(&format!("SELECT name FROM {} WHERE id=?",if *project {"projects"}else{"tags"}),[id],|r|r.get(0)).map_err(err)?),
        Patch::Created(project,name) => {
            let table=if *project {"projects"}else{"tags"};
            let join=if *project {"item_projects"}else{"item_tags"};
            let id=db.query_row(&format!("SELECT id FROM {table} WHERE name=?"),[name],|r|r.get::<_,i64>(0)).map_err(err)?;
            let mut stmt=db.prepare(&format!("SELECT item_id,{} FROM {join} WHERE label_id=?",if *project {"favourite"}else{"0"})).map_err(err)?;
            let rows=stmt.query_map([id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
            Patch::Deleted(*project,id,name.clone(),rows.collect::<std::result::Result<Vec<_>,_>>().map_err(err)?)
        },
        Patch::Deleted(project,id,name,members) => if members.is_empty() {Patch::Created(*project,name.clone())} else {Patch::RemoveLabel(*project,*id)},
        Patch::RemoveLabel(project,id) => {
            let table=if *project {"projects"}else{"tags"};
            let join=if *project {"item_projects"}else{"item_tags"};
            let name=db.query_row(&format!("SELECT name FROM {table} WHERE id=?"),[id],|r|r.get(0)).map_err(err)?;
            let mut stmt=db.prepare(&format!("SELECT item_id,{} FROM {join} WHERE label_id=?",if *project {"favourite"}else{"0"})).map_err(err)?;
            let rows=stmt.query_map([id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
            Patch::Deleted(*project,*id,name,rows.collect::<std::result::Result<Vec<_>,_>>().map_err(err)?)
        },
        Patch::CreatedCategory(name) => Patch::DeletedCategory(name.clone()),
        Patch::DeletedCategory(name) => Patch::CreatedCategory(name.clone()),
    })
}

fn transfer(db:&Connection,redo:bool)->Result<()> {
    let (from,to)=if redo {("organisation_redo","organisation_history")} else {("organisation_history","organisation_redo")};
    let entry:Option<(i64,String,String)>=db.query_row(&format!("SELECT id,label,patches FROM {from} ORDER BY id DESC LIMIT 1"),[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(err)?;
    let (entry,label,patches)=entry.ok_or(if redo {"Nothing to redo yet."} else {"Nothing to undo yet."})?;
    let mut inverses=vec![];
    for patch in serde_json::from_str::<Vec<Patch>>(&patches).map_err(err)? {
        inverses.push(inverse(db,&patch)?);
        match patch {
            Patch::Category(id,name)=>{if let Some(name)=name {db.execute("INSERT INTO item_categories(item_id,name) SELECT id,? FROM items WHERE id=? ON CONFLICT(item_id) DO UPDATE SET name=excluded.name",params![name,id]).map_err(err)?;}else{db.execute("DELETE FROM item_categories WHERE item_id=?",[id]).map_err(err)?;}},
            Patch::Member(project,id,label,previous)=>{
                let table=if project{"item_projects"}else{"item_tags"};
                db.execute(&format!("DELETE FROM {table} WHERE item_id=? AND label_id=?"),params![id,label]).map_err(err)?;
                if let Some(favourite)=previous {
                    let labels=if project{"projects"}else{"tags"};
                    let valid:bool=db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM items WHERE id=?) AND EXISTS(SELECT 1 FROM {labels} WHERE id=?)"),params![id,label],|r|r.get(0)).map_err(err)?;
                    if !valid {return Err("A reference or label needed for Undo no longer exists.".into());}
                    if project {db.execute("INSERT INTO item_projects(item_id,label_id,favourite) VALUES(?,?,?)",params![id,label,favourite]).map_err(err)?;}else{db.execute("INSERT INTO item_tags(item_id,label_id) VALUES(?,?)",params![id,label]).map_err(err)?;}
                }
            },
            Patch::Archived(id,previous)=>{db.execute("UPDATE items SET archived=? WHERE id=? AND NOT EXISTS(SELECT 1 FROM duplicate_recovery WHERE item_id=items.id)",params![previous,id]).map_err(err)?;},
            Patch::Link(id,name,path)=>{db.execute("UPDATE items SET name=?,path=? WHERE id=? AND kind='link'",params![name,path,id]).map_err(err)?;},
            Patch::Source(id,source)=>{db.execute("UPDATE items SET source=? WHERE id=? AND kind='link'",params![source,id]).map_err(err)?;},
            Patch::SourceUrl(id,url)=>{db.execute("UPDATE items SET source_url=? WHERE id=?",params![url,id]).map_err(err)?;},
            Patch::Rename(project,id,name)=>{db.execute(&format!("UPDATE {} SET name=? WHERE id=?",if project{"projects"}else{"tags"}),params![name,id]).map_err(err)?;},
            Patch::Created(project,name)=>{
                let table=if project{"projects"}else{"tags"};let join=if project{"item_projects"}else{"item_tags"};
                let used:bool=db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {join} j JOIN {table} t ON j.label_id=t.id WHERE t.name=?)"),[&name],|r|r.get(0)).map_err(err)?;
                if used {return Err("This label now contains references. Remove its assignments before undoing its creation.".into());}
                db.execute(&format!("DELETE FROM {table} WHERE name=?"),[name]).map_err(err)?;
            },
            Patch::Deleted(project,id,name,members)=>{
                let table=if project{"projects"}else{"tags"};
                db.execute(&format!("INSERT INTO {table}(id,name) VALUES(?,?)"),params![id,name]).map_err(err)?;
                for (item,favourite) in members {
                    if project {db.execute("INSERT INTO item_projects(item_id,label_id,favourite) SELECT id,?,? FROM items WHERE id=?",params![id,favourite,item]).map_err(err)?;}
                    else{db.execute("INSERT INTO item_tags(item_id,label_id) SELECT id,? FROM items WHERE id=?",params![id,item]).map_err(err)?;}
                }
            },
            Patch::RemoveLabel(project,id)=>{db.execute(&format!("DELETE FROM {} WHERE id=?",if project{"projects"}else{"tags"}),[id]).map_err(err)?;},
            Patch::DeletedCategory(name)=>{db.execute("INSERT INTO categories(name) VALUES(?)",[name]).map_err(err)?;},
            Patch::CreatedCategory(name)=>{
                let used:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM item_categories WHERE name=?)",[&name],|r|r.get(0)).map_err(err)?;
                if used{return Err("This category now contains references. Undo their category changes first.".into());}
                db.execute("DELETE FROM categories WHERE name=?",[name]).map_err(err)?;
            }
        }
    }
    inverses.reverse();
    db.execute(&format!("INSERT INTO {to}(label,patches) VALUES(?,?)"),params![label,serde_json::to_string(&inverses).map_err(err)?]).map_err(err)?;
    db.execute(&format!("DELETE FROM {to} WHERE id NOT IN (SELECT id FROM {to} ORDER BY id DESC LIMIT 100)"),[]).map_err(err)?;
    db.execute(&format!("DELETE FROM {from} WHERE id=?"),[entry]).map_err(err)?;
    Ok(())
}
