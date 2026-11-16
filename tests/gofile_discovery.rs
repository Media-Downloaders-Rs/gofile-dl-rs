use personal_downloaders::{
    Cancel,
    sites::{self, gofile},
};
use serde_json::{Value, json};

fn item(id: &str, name: &str) -> Value {
    json!({"id":id,"type":"file","name":name,"size":4,"md5":"098f6bcd4621d373cade4e832627b4f6","link":format!("https://store8.gofile.io/download/web/{id}/{name}")})
}
fn folder(id: &str, children: Vec<Value>, next: bool) -> Value {
    json!({"ok":true,"data":{"id":id,"type":"folder","name":"Root","children":children},"metadata":{"hasNextPage":next}})
}

#[test]
fn validates_input_before_opening_any_session() {
    assert_eq!(
        sites::inputs(
            personal_downloaders::Site::Gofile,
            "https://www.gofile.io/d/abcdef?x=1 https://gofile.io/d/abcdef"
        )
        .unwrap(),
        vec!["https://gofile.io/d/abcdef"]
    );
    assert!(gofile::normalize("https://user:secret@gofile.io/d/abcdef").is_err());
    assert!(gofile::normalize("https://gofile.io.evil.test/d/abcdef").is_err());
}

#[test]
fn pagination_nested_folders_duplicates_and_names_are_preserved() {
    let mut calls = Vec::new();
    let listing = gofile::discover(
        "https://gofile.io/d/abcdef",
        &Cancel::default(),
        &mut |id, page| {
            calls.push((id.to_owned(), page));
            Ok(match (id, page) {
                ("abcdef", 1) => folder(
                    id,
                    vec![
                        item("f1", "same.mp4"),
                        json!({"id":"nested","type":"folder","name":"Nested"}),
                    ],
                    true,
                ),
                ("nested", 1) => folder(id, vec![item("f2", "photo.jpg")], false),
                ("abcdef", 2) => folder(
                    id,
                    vec![item("f1", "same.mp4"), item("f3", "same.mp4")],
                    false,
                ),
                _ => panic!("unexpected page"),
            })
        },
    )
    .unwrap();
    assert_eq!(listing.files.len(), 3);
    assert!(
        listing
            .files
            .iter()
            .any(|f| f.path == "Root [abcdef]/Nested/photo.jpg")
    );
    let paths: std::collections::HashSet<_> = listing
        .files
        .iter()
        .map(|f| f.path.to_lowercase())
        .collect();
    assert_eq!(paths.len(), 3);
    assert_eq!(calls.len(), 3);
    assert!(listing.pending.is_empty());
}

#[test]
fn access_password_and_expired_session_errors_are_not_empty_successes() {
    for result in [
        json!({"ok":false,"status":"error-passwordWrong"}),
        json!({"ok":false,"http":401}),
        json!({"ok":true,"data":{"canAccess":false}}),
    ] {
        let mut fetch = |_: &str, _: usize| Ok(result.clone());
        let error = gofile::discover("https://gofile.io/d/abcdef", &Cancel::default(), &mut fetch)
            .unwrap_err()
            .to_string();
        assert!(!error.is_empty());
    }
}

#[test]
fn a_failed_nested_folder_does_not_lose_valid_files() {
    let listing = gofile::discover(
        "https://gofile.io/d/abcdef",
        &Cancel::default(),
        &mut |id, _| {
            if id == "abcdef" {
                Ok(folder(
                    id,
                    vec![
                        item("f1", "ok.mp4"),
                        json!({"id":"nested","type":"folder","name":"Private"}),
                    ],
                    false,
                ))
            } else {
                Ok(json!({"ok":false,"http":403}))
            }
        },
    )
    .unwrap();
    assert_eq!(listing.files.len(), 1);
    assert_eq!(listing.pending.len(), 1);
}

#[test]
fn repeated_pages_and_invalid_hashes_remain_pending() {
    let mut bad = item("bad", "bad.mp4");
    bad["md5"] = json!("invalid");
    let listing = gofile::discover(
        "https://gofile.io/d/abcdef",
        &Cancel::default(),
        &mut |id, _| Ok(folder(id, vec![item("f1", "ok.mp4"), bad.clone()], true)),
    )
    .unwrap();
    assert_eq!(listing.files.len(), 1);
    assert!(!listing.pending.is_empty());
}

#[test]
fn partial_refresh_keeps_files_from_failed_folders_and_pages() {
    use personal_downloaders::{Site, state::State};
    let root = tempfile::tempdir().unwrap();
    let state = State::open(root.path(), Site::Gofile).unwrap();
    let url = "https://gofile.io/d/abcdef";
    let previous = gofile::discover(url, &Cancel::default(), &mut |id, page| {
        Ok(match (id, page) {
            ("abcdef", 1) => folder(
                id,
                vec![
                    item("a", "a.mp4"),
                    json!({"id":"nested","type":"folder","name":"Nested"}),
                ],
                true,
            ),
            ("nested", 1) => folder(id, vec![item("b", "b.mp4")], false),
            ("abcdef", 2) => folder(id, vec![item("c", "c.mp4")], false),
            _ => panic!(),
        })
    })
    .unwrap();
    state.save_listing(url, &previous).unwrap();
    let refreshed = gofile::discover(url, &Cancel::default(), &mut |id, page| {
        if id == "abcdef" && page == 1 {
            Ok(folder(
                id,
                vec![
                    item("a", "a.mp4"),
                    json!({"id":"nested","type":"folder","name":"Nested"}),
                ],
                true,
            ))
        } else {
            Ok(json!({"ok":false,"http":403}))
        }
    })
    .unwrap();
    assert_eq!(refreshed.files.len(), 1);
    assert_eq!(refreshed.pending.len(), 2);
    let saved = gofile::save_listing(&state, url, refreshed).unwrap();
    assert_eq!(saved.files.len(), 3);
    assert_eq!(saved.pending.len(), 2);
    assert_eq!(state.listing(url).unwrap().unwrap().files.len(), 3);
}
