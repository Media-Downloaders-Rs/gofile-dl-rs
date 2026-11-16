use crate::{Cancel, File, Listing, Pending, browser::BrowserSession, safe_name};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use url::Url;

pub fn save_listing(
    state: &crate::state::State,
    input: &str,
    mut listing: Listing,
) -> Result<Listing> {
    if !listing.pending.is_empty()
        && let Some(previous) = state.listing(input)?
    {
        let mut ids: HashSet<String> = listing.files.iter().map(|f| f.id.clone()).collect();
        for file in previous.files {
            if ids.insert(file.id.clone()) {
                listing.files.push(file)
            }
        }
    }
    state.save_listing(input, &listing)?;
    Ok(listing)
}

pub fn normalize(input: &str) -> Result<String> {
    let u = Url::parse(input)?;
    let pattern = regex::Regex::new(r"^/d/([A-Za-z0-9_-]{6,64})/?$")?;
    let c = pattern
        .captures(u.path())
        .ok_or_else(|| anyhow::anyhow!("Introduce un enlace https://gofile.io/d/código"))?;
    if u.scheme() != "https"
        || !matches!(u.host_str(), Some("gofile.io" | "www.gofile.io"))
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
    {
        bail!("Enlace GoFile no válido")
    }
    Ok(format!("https://gofile.io/d/{}", &c[1]))
}
pub fn validate_media(u: &Url, _post: &str) -> Result<()> {
    if u.scheme() != "https"
        || !u.host_str().is_some_and(|h| h.ends_with(".gofile.io"))
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
        || !u.path().starts_with("/download/")
    {
        bail!("Servidor de GoFile no reconocido")
    }
    Ok(())
}
fn unpack(v: Value) -> Result<(Value, Value)> {
    if v["ok"] != true {
        // Never print arbitrary error payloads, tokens or exception messages.
        if v["http"] == 401 {
            bail!("Sesión de GoFile vencida; vuelve a intentar")
        }
        bail!("GoFile rechazó el acceso; revisa la contraseña y la disponibilidad")
    }
    if v["data"]["canAccess"] == false {
        bail!("Carpeta privada, caducada o inaccesible")
    }
    Ok((v["data"].clone(), v["metadata"].clone()))
}
pub fn discover<F>(input: &str, cancel: &Cancel, fetch: &mut F) -> Result<Listing>
where
    F: FnMut(&str, usize) -> Result<Value>,
{
    let url = normalize(input)?;
    let code = url.rsplit('/').next().unwrap();
    let (root, meta) = unpack(fetch(code, 1)?)?;
    let title = format!(
        "{} [{code}]",
        safe_name(root["name"].as_str().unwrap_or(code))
    );
    let mut walker = Walker {
        url: &url,
        cancel,
        fetch,
        listing: Listing {
            title: title.clone(),
            ..Default::default()
        },
        folders: HashSet::new(),
        files: HashSet::new(),
        paths: HashSet::new(),
    };
    walker.walk(root, meta, &title, 0)?;
    Ok(walker.listing)
}
struct Walker<'a, F> {
    url: &'a str,
    cancel: &'a Cancel,
    fetch: &'a mut F,
    listing: Listing,
    folders: HashSet<String>,
    files: HashSet<String>,
    paths: HashSet<String>,
}
impl<F: FnMut(&str, usize) -> Result<Value>> Walker<'_, F> {
    fn allocate(&mut self, parent: &str, name: &str, id: &str) -> String {
        let name = safe_name(name);
        let path = format!("{parent}/{name}");
        if self.paths.insert(path.to_lowercase()) {
            return path;
        }
        let ext = std::path::Path::new(&name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let stem = std::path::Path::new(&name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&name);
        let hash = format!("{:x}", Sha256::digest(id.as_bytes()));
        for number in 0.. {
            let tail = if number == 0 {
                String::new()
            } else {
                format!("_{number}")
            };
            let dot = if ext.is_empty() { "" } else { "." };
            let value = format!("{parent}/{stem}__{}{tail}{dot}{ext}", &hash[..10]);
            if self.paths.insert(value.to_lowercase()) {
                return value;
            }
        }
        unreachable!()
    }
    fn add(&mut self, data: &Value, parent: &str) -> Result<()> {
        let id = data["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Archivo sin identificador"))?;
        if self.files.contains(id) {
            return Ok(());
        }
        let link = data["link"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Archivo sin enlace"))?;
        validate_media(&Url::parse(link)?, self.url)?;
        let size = data["size"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Tamaño de GoFile inválido"))?;
        let name = data["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Archivo sin nombre"))?;
        let md5 = data["md5"].as_str().unwrap_or("").to_ascii_lowercase();
        if !md5.is_empty() && (md5.len() != 32 || !md5.bytes().all(|b| b.is_ascii_hexdigit())) {
            bail!("MD5 de GoFile inválido")
        }
        let path = self.allocate(parent, name, id);
        self.files.insert(id.to_owned());
        self.listing.files.push(File {
            fallback: None,
            sha256: String::new(),
            id: id.into(),
            post: self.url.into(),
            url: link.into(),
            path,
            thumbnail: None,
            preview_user_agent: None,
            published_at: None,
            expected_size: Some(size),
            md5,
        });
        Ok(())
    }
    fn walk(&mut self, mut data: Value, mut meta: Value, parent: &str, depth: usize) -> Result<()> {
        self.cancel.check()?;
        if depth > 40 {
            bail!("La carpeta supera 40 niveles")
        }
        if data["type"] == "file" {
            return self.add(&data, parent);
        }
        if data["type"] != "folder" {
            bail!("Formato de carpeta no reconocido")
        }
        let id = data["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Carpeta sin identificador"))?
            .to_owned();
        if !self.folders.insert(id.clone()) {
            return Ok(());
        }
        let mut fingerprints = HashSet::new();
        for page in 1..=10000 {
            self.cancel.check()?;
            let children: Vec<Value> = match &data["children"] {
                Value::Array(a) => a.clone(),
                Value::Object(o) => o.values().cloned().collect(),
                _ => bail!("GoFile no devolvió una lista de archivos"),
            };
            let fingerprint: Vec<String> = children
                .iter()
                .map(|v| v["id"].as_str().unwrap_or("").to_owned())
                .collect();
            if !fingerprints.insert(fingerprint) {
                self.listing.pending.push(Pending {
                    url: self.url.into(),
                    error: "GoFile repitió una página".into(),
                });
                return Ok(());
            }
            for child in children {
                self.cancel.check()?;
                let child_id = child["id"].as_str().unwrap_or("");
                let result = if child["type"] == "folder" {
                    let path =
                        self.allocate(parent, child["name"].as_str().unwrap_or(child_id), child_id);
                    (self.fetch)(child_id, 1)
                        .and_then(unpack)
                        .and_then(|(nested, meta)| self.walk(nested, meta, &path, depth + 1))
                } else if child["type"] == "file" {
                    self.add(&child, parent)
                } else {
                    Err(anyhow::anyhow!("Tipo de archivo no reconocido"))
                };
                if let Err(error) = result {
                    self.cancel.check()?;
                    self.listing.pending.push(Pending {
                        url: self.url.into(),
                        error: error.to_string(),
                    });
                }
            }
            let more = meta["hasNextPage"]
                .as_bool()
                .unwrap_or_else(|| meta["totalPages"].as_u64().unwrap_or(1) > page as u64);
            if !more {
                return Ok(());
            }
            match (self.fetch)(&id, page + 1).and_then(unpack) {
                Ok((next, m)) => {
                    data = next;
                    meta = m;
                }
                Err(error) => {
                    self.cancel.check()?;
                    self.listing.pending.push(Pending {
                        url: self.url.into(),
                        error: error.to_string(),
                    });
                    return Ok(());
                }
            }
        }
        self.listing.pending.push(Pending {
            url: self.url.into(),
            error: "Límite de paginación alcanzado".into(),
        });
        Ok(())
    }
}
pub fn browser_listing(
    browser: &mut BrowserSession,
    url: &str,
    password: Option<&str>,
    cancel: &Cancel,
) -> Result<(Listing, String)> {
    browser.block_urls(&["*gofile.io/download/*"])?;
    browser.navigate(&normalize(url)?)?;
    let ready=browser.evaluate("(async()=>{const {accountsReady}=await import('/js/accounts/accountService.js');await accountsReady;const {getActiveAccount}=await import('/js/accounts/accountStore.js');return !!getActiveAccount();})()")?;
    if ready != true {
        bail!("GoFile no inició la sesión de invitado")
    }
    let cookie = browser
        .cookies("https://gofile.io/")?
        .into_iter()
        .find(|c| c.name == "accountToken")
        .map(|c| c.value)
        .ok_or_else(|| anyhow::anyhow!("GoFile no creó una sesión de descarga"))?;
    let password = password
        .filter(|s| !s.is_empty())
        .map(|s| format!("{:x}", Sha256::digest(s.as_bytes())));
    let listing = discover(url, cancel, &mut |id, page| {
        let args = json!({"identifier":id,"page":page,"password":password});
        browser.evaluate(&format!(r#"(async ({{identifier,page,password}})=>{{const {{getActiveAccount}}=await import('/js/accounts/accountStore.js');const {{getFolder}}=await import('/js/services/contents.js');try{{const result=await getFolder(getActiveAccount().token,identifier,{{page,pageSize:100,sortField:'name',sortDirection:1,password}});return {{ok:true,...result}};}}catch(e){{return {{ok:false,status:e.apiStatus||'',kind:e.kind||'',http:e.httpStatus||0}};}}}})({args})"#))
    })?;
    Ok((listing, cookie))
}
