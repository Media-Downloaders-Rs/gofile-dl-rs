use personal_downloaders::{Cancel, Site, state::State};
use serde_json::json;
use sha2::{Digest, Sha256};

fn fixture(root: &std::path::Path, body: &[u8]) {
    let meta = root.join(".gofile-dl/abcdef");
    std::fs::create_dir_all(&meta).unwrap();
    let media = root.join("Root [abcdef]/file.mp4");
    std::fs::create_dir_all(media.parent().unwrap()).unwrap();
    std::fs::write(media, body).unwrap();
    let manifest = json!({"source":"https://gofile.io/d/abcdef","folder":"Root [abcdef]","files":[{"id":"file-id","name":"file.mp4","url":"https://store8.gofile.io/download/web/file-id/file.mp4","path":"Root [abcdef]/file.mp4","size":4,"md5":"098f6bcd4621d373cade4e832627b4f6"}],"pending":[]});
    std::fs::write(meta.join("manifest.json"), manifest.to_string()).unwrap();
    let key = format!("{:x}", Sha256::digest(b"Root [abcdef]/file.mp4\nfile-id"));
    let receipt = root.join(".gofile-dl/transfers").join(key);
    std::fs::create_dir_all(&receipt).unwrap();
    std::fs::write(
        receipt.join("complete.json"),
        json!({"id":"file-id","size":4,"md5":"098f6bcd4621d373cade4e832627b4f6"}).to_string(),
    )
    .unwrap();
}

#[test]
fn manifest_and_verified_receipt_are_imported_and_survive_cdn_change() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path(), b"test");
    let state = State::open(root.path(), Site::Gofile).unwrap();
    state
        .import_legacy(Site::Gofile, &Cancel::default())
        .unwrap();
    let listing = state
        .listing("https://gofile.io/d/abcdef")
        .unwrap()
        .unwrap();
    assert_eq!(listing.files.len(), 1);
    let mut f = listing.files[0].clone();
    assert!(state.completed(&f, &Cancel::default()).unwrap());
    f.url = f.url.replace("store8.", "store9.");
    assert!(state.completed(&f, &Cancel::default()).unwrap());
    assert!(
        root.path()
            .join(".gofile-dl/abcdef/manifest.json")
            .is_file()
    );
}

#[test]
fn a_receipt_with_wrong_bytes_does_not_mark_a_file_complete() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path(), b"xxxx");
    let state = State::open(root.path(), Site::Gofile).unwrap();
    state
        .import_legacy(Site::Gofile, &Cancel::default())
        .unwrap();
    let listing = state
        .listing("https://gofile.io/d/abcdef")
        .unwrap()
        .unwrap();
    assert!(
        !state
            .completed(&listing.files[0], &Cancel::default())
            .unwrap()
    );
    assert_eq!(
        std::fs::read(root.path().join("Root [abcdef]/file.mp4")).unwrap(),
        b"xxxx"
    );
}
