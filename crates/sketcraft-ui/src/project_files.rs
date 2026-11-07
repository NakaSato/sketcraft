//! Portable project files are separate from the local recovery/autosave document.
use ketchup_core::Document;
use std::{path::PathBuf, sync::mpsc};

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "project_file_tests.rs"]
mod tests;

#[derive(Debug)]
pub enum Outcome {
    Opened(Box<Document>, Option<PathBuf>),
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    BrowserOpened(Box<Document>, String, String),
    Saved(Option<PathBuf>),
    Imported(Box<ketchup_core::Model>, std::collections::BTreeSet<u64>),
    Exported,
    Cancelled,
}
pub type Pending = mpsc::Receiver<Result<Outcome, String>>;

pub fn file_name(name: &str) -> String {
    exchange_name(name, "ketchup")
}

pub fn set_document_name(doc: &mut Document, path: &std::path::Path) {
    if doc.shared_project.is_some() {
        return;
    }
    if let Some(name) = path.file_stem() {
        doc.name = name.to_string_lossy().into_owned();
    }
}

pub fn exchange_name(name: &str, extension: &str) -> String {
    let name: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let name = name.trim_matches([' ', '.']);
    let name = if name.is_empty() {
        "Untitled model"
    } else {
        name
    };
    if name
        .to_ascii_lowercase()
        .ends_with(&format!(".{extension}"))
    {
        name.to_owned()
    } else {
        format!("{name}.{extension}")
    }
}

pub fn decode(bytes: &[u8]) -> Result<Document, String> {
    let json = std::str::from_utf8(bytes).map_err(|_| "The project is not UTF-8 text")?;
    Document::from_json(json)
}

#[cfg(not(target_arch = "wasm32"))]
fn run(
    ctx: &eframe::egui::Context,
    action: impl FnOnce() -> Result<Outcome, String> + Send + 'static,
) -> Pending {
    let (sender, receiver) = mpsc::channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = sender.send(action());
        ctx.request_repaint();
    });
    receiver
}

#[cfg(not(target_arch = "wasm32"))]
pub fn open_path(ctx: &eframe::egui::Context, path: PathBuf) -> Pending {
    run(ctx, move || read_project(path))
}

#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub fn open(_ctx: &eframe::egui::Context) -> Pending {
    let (sender, receiver) = mpsc::channel();
    let _ = sender.send(Ok(Outcome::Cancelled));
    receiver
}

#[cfg(not(target_arch = "wasm32"))]
fn read_project(path: PathBuf) -> Result<Outcome, String> {
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    decode(&bytes).map(|mut doc| {
        if let Some(link) = &mut doc.shared_project {
            if let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) {
                link.file_updated_at = modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
            }
        }
        Outcome::Opened(Box::new(doc), Some(path))
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn write(path: &std::path::Path, contents: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temporary
        .write_all(contents)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_to_path(
    ctx: &eframe::egui::Context,
    path: PathBuf,
    contents: String,
) -> Pending {
    run(ctx, move || {
        let path = ketchup_path(path)?;
        write(&path, contents.as_bytes())?;
        Ok(Outcome::Saved(Some(path)))
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save(
    ctx: &eframe::egui::Context,
    path: Option<PathBuf>,
    _name: String,
    contents: String,
) -> Pending {
    run(ctx, move || {
        let Some(path) = path else {
            return Ok(Outcome::Cancelled);
        };
        let path = ketchup_path(path)?;
        write(&path, contents.as_bytes())?;
        Ok(Outcome::Saved(Some(path)))
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn ketchup_path(path: PathBuf) -> Result<PathBuf, String> {
    extension_path(path, "ketchup")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extension_path(path: PathBuf, extension: &str) -> Result<PathBuf, String> {
    if path.extension().is_some_and(|ext| {
        ext.as_encoded_bytes()
            .eq_ignore_ascii_case(extension.as_bytes())
    }) {
        return Ok(path);
    }
    let mut name = path.into_os_string();
    name.push(format!(".{extension}"));
    let path = PathBuf::from(name);
    // The picker confirmed overwriting the selected path, not an appended path.
    if path.exists() {
        return Err(format!(
            "{} already exists. Select that .{extension} file in the save dialog to replace it.",
            path.display()
        ));
    }
    Ok(path)
}

#[cfg(target_arch = "wasm32")]
fn pick_browser_file(
    accept: &str,
    on_picked: impl FnOnce(web_sys::File) + 'static,
) -> Result<(), eframe::wasm_bindgen::JsValue> {
    use eframe::wasm_bindgen::{closure::Closure, JsCast, JsValue};
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No browser window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("No browser document"))?;
    let body = document
        .body()
        .ok_or_else(|| JsValue::from_str("No browser document body"))?;
    let input = document
        .create_element("input")?
        .dyn_into::<web_sys::HtmlInputElement>()?;
    input.set_type("file");
    input.set_accept(accept);
    let _ = input.style().set_property("display", "none");
    body.append_child(&input)?;

    let input_clone = input.clone();
    let listener = Closure::once(move |_: web_sys::Event| {
        if let Some(files) = input_clone.files() {
            if let Some(file) = files.get(0) {
                on_picked(file);
            }
        }
        let _ = input_clone.remove();
    });
    input.add_event_listener_with_callback("change", listener.as_ref().unchecked_ref())?;
    listener.forget();
    input.click();
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn open(ctx: &eframe::egui::Context) -> Pending {
    let (sender, receiver) = mpsc::channel();
    let ctx = ctx.clone();
    let res = pick_browser_file(".ketchup", move |file| {
        wasm_bindgen_futures::spawn_local(async move {
            let buffer_promise = file.array_buffer();
            let result = match wasm_bindgen_futures::JsFuture::from(buffer_promise).await {
                Ok(buffer) => decode(&js_sys::Uint8Array::new(&buffer).to_vec()).map(|mut doc| {
                    if let Some(link) = &mut doc.shared_project {
                        link.file_updated_at = (file.last_modified() / 1000.) as u64;
                    }
                    set_document_name(&mut doc, std::path::Path::new(&file.name()));
                    Outcome::BrowserOpened(
                        Box::new(doc),
                        file.name(),
                        format!("{}:{}:{}", file.name(), file.size(), file.last_modified()),
                    )
                }),
                Err(_) => Err("The browser could not read this project".into()),
            };
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    });
    if let Err(e) = res {
        let (err_sender, err_receiver) = mpsc::channel();
        let _ = err_sender.send(Err(format!("The browser could not open the file picker: {e:?}")));
        return err_receiver;
    }
    receiver
}

#[cfg(target_arch = "wasm32")]
pub fn save(
    ctx: &eframe::egui::Context,
    _path: Option<PathBuf>,
    name: String,
    contents: String,
) -> Pending {
    let (sender, receiver) = mpsc::channel();
    let name = file_name(&name);
    let launch = js_sys::Function::new_with_args("name, contents", include_str!("project_save.js"));
    // Open the picker before spawning the async task, while the save action
    // still has the browser's user activation.
    let result = launch.call2(
        &eframe::wasm_bindgen::JsValue::NULL,
        &eframe::wasm_bindgen::JsValue::from_str(&name),
        &eframe::wasm_bindgen::JsValue::from_str(&contents),
    );
    match result {
        Ok(value) if !value.is_null() => {
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&value))
                    .await
                    .map(|value| match value.as_string() {
                        // Browsers expose the chosen filename, not its full path.
                        Some(name) => Outcome::Saved(Some(PathBuf::from(name))),
                        None => Outcome::Cancelled,
                    })
                    .map_err(browser_save_error);
                let _ = sender.send(result);
                ctx.request_repaint();
            });
        }
        result => {
            let result = result
                .and_then(|_| download(&name, &contents))
                .map(|_| Outcome::Saved(None))
                .map_err(browser_save_error);
            let _ = sender.send(result);
            ctx.request_repaint();
        }
    }
    receiver
}

#[cfg(target_arch = "wasm32")]
fn browser_save_error(error: eframe::wasm_bindgen::JsValue) -> String {
    js_sys::Reflect::get(&error, &"message".into())
        .ok()
        .and_then(|message| message.as_string())
        .or_else(|| error.as_string())
        .unwrap_or_else(|| "The browser could not save this project".into())
}

#[cfg(target_arch = "wasm32")]
fn download(name: &str, contents: &str) -> Result<(), eframe::wasm_bindgen::JsValue> {
    download_bytes(
        name,
        contents.as_bytes(),
        if name.ends_with(".obj") {
            "text/plain"
        } else {
            "application/json"
        },
    )
}

#[cfg(target_arch = "wasm32")]
fn download_bytes(
    name: &str,
    contents: &[u8],
    mime: &str,
) -> Result<(), eframe::wasm_bindgen::JsValue> {
    use eframe::wasm_bindgen::{closure::Closure, JsCast, JsValue};
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No browser window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("No browser document"))?;
    let body = document
        .body()
        .ok_or_else(|| JsValue::from_str("No browser document body"))?;
    let anchor = document
        .create_element("a")?
        .dyn_into::<web_sys::HtmlAnchorElement>()?;
    let url = blob_url(contents, mime)?;
    anchor.set_href(&url);
    anchor.set_download(name);
    if let Err(error) = body.append_child(&anchor) {
        let _ = web_sys::Url::revoke_object_url(&url);
        return Err(error);
    }
    anchor.click();
    anchor.remove();
    // Keep the URL alive long enough for the browser to start the download.
    let cleanup = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    let _ = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(cleanup.unchecked_ref(), 10_000);
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn blob_url(contents: &[u8], mime: &str) -> Result<String, eframe::wasm_bindgen::JsValue> {
    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(contents));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)?;
    web_sys::Url::create_object_url_with_blob(&blob)
}

#[cfg(target_arch = "wasm32")]
fn open_pdf(contents: &[u8]) -> Result<(), eframe::wasm_bindgen::JsValue> {
    use eframe::wasm_bindgen::JsValue;
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No browser window"))?;
    let url = blob_url(contents, "application/pdf")?;
    // Keep this synchronous with Generate PDF so user activation is available.
    let opened = window.open_with_url_and_target(&url, "_blank");
    match opened {
        Ok(Some(tab)) => {
            let _ = tab.set_opener(&JsValue::NULL);
            release_pdf_when_closed(window, tab, url);
            Ok(())
        }
        result => {
            let _ = web_sys::Url::revoke_object_url(&url);
            match result {
                Err(error) => Err(error),
                _ => Err(JsValue::from_str(
                    "Allow pop-ups for SketCraft, then generate the PDF again.",
                )),
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn release_pdf_when_closed(window: web_sys::Window, tab: web_sys::Window, url: String) {
    use eframe::wasm_bindgen::{closure::Closure, JsCast};
    // A download's short timeout is unsuitable for a viewer: keep its URL valid
    // for reload/save, then release the PDF bytes after the viewer tab closes.
    let timer_window = window.clone();
    let cleanup = Closure::once_into_js(move || {
        if tab.closed().unwrap_or(false) {
            let _ = web_sys::Url::revoke_object_url(&url);
        } else {
            release_pdf_when_closed(window, tab, url);
        }
    });
    let _ = timer_window
        .set_timeout_with_callback_and_timeout_and_arguments_0(cleanup.unchecked_ref(), 1_000);
}

#[cfg(not(target_arch = "wasm32"))]
fn open_pdf(path: &std::path::Path) -> Result<(), String> {
    // Use the OS file association, passing the absolute path as one argument.
    // No shell interpolation: spaces and metacharacters remain part of the name.
    let path = std::path::absolute(path).map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/usr/bin/open");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    let mut child = command.arg(path).spawn().map_err(|e| e.to_string())?;
    // Some viewers stay running. Finish the export immediately and reap the
    // launcher separately rather than keeping the file-operation modal open.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// The format to read: the chosen one, or in "any" mode the file's own extension.
pub(crate) fn import_extension(chosen: &str, file_name: &str) -> Result<String, String> {
    if chosen != "any" {
        return Ok(chosen.to_owned());
    }
    let extension = std::path::Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if crate::mesh_exchange::IMPORT_EXTENSIONS.contains(&extension.as_str()) {
        Ok(extension)
    } else {
        Err(format!(
            "Unsupported import format. Choose a .{} file",
            crate::mesh_exchange::IMPORT_EXTENSIONS.join(", .")
        ))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn import_path(
    ctx: &eframe::egui::Context,
    model: ketchup_core::Model,
    path: PathBuf,
    extension: String,
) -> Pending {
    let extension = import_extension(&extension, &path.to_string_lossy());
    run(ctx, move || {
        let extension = extension?;
        if std::fs::metadata(&path).map_err(|e| e.to_string())?.len()
            > crate::interchange::MAX_SKP_BYTES as u64
        {
            return Err("Geometry import is limited to 64 MiB per file".into());
        }
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("SketchUp import");
        (if extension == "skp" {
            crate::interchange::import_skp(&bytes, name)
        } else {
            crate::mesh_exchange::import(&bytes, &extension, name)
        })
        .and_then(|imported| prepare_import(model, &imported))
    })
}

#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub fn import(
    _ctx: &eframe::egui::Context,
    _model: ketchup_core::Model,
    _extension: String,
) -> Pending {
    let (sender, receiver) = mpsc::channel();
    let _ = sender.send(Ok(Outcome::Cancelled));
    receiver
}

#[cfg(target_arch = "wasm32")]
pub fn import(
    ctx: &eframe::egui::Context,
    model: ketchup_core::Model,
    extension: String,
) -> Pending {
    let (sender, receiver) = mpsc::channel();
    let ctx = ctx.clone();
    let accept = if extension == "any" {
        crate::mesh_exchange::IMPORT_EXTENSIONS.map(|e| format!(".{e}")).join(",")
    } else {
        format!(".{extension}")
    };
    let chosen = extension.clone();
    let res = pick_browser_file(&accept, move |file| {
        wasm_bindgen_futures::spawn_local(async move {
            if file.size() > crate::interchange::MAX_SKP_BYTES as f64 {
                let _ = sender.send(Err("Geometry import is limited to 64 MiB per file".into()));
                ctx.request_repaint();
                return;
            }
            let buffer_promise = file.array_buffer();
            let result = match wasm_bindgen_futures::JsFuture::from(buffer_promise).await {
                Ok(buffer) => match import_extension(&chosen, &file.name()) {
                    Err(message) => Err(message),
                    Ok(ext_clone) => {
                    let name = file.name();
                    let name = std::path::Path::new(&name)
                        .file_stem()
                        .and_then(|n| n.to_str())
                        .unwrap_or("SketchUp import");
                    (if ext_clone == "skp" {
                        crate::interchange::import_skp_async(
                            &js_sys::Uint8Array::new(&buffer).to_vec(),
                            name,
                        )
                        .await
                    } else {
                        crate::mesh_exchange::import(
                            &js_sys::Uint8Array::new(&buffer).to_vec(),
                            &ext_clone,
                            name,
                        )
                    })
                    .and_then(|imported| prepare_import(model, &imported))
                    }
                },
                Err(_) => Err("The browser could not read this geometry file".into()),
            };
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    });
    if let Err(e) = res {
        let (err_sender, err_receiver) = mpsc::channel();
        let _ = err_sender.send(Err(format!("The browser could not open the file picker: {e:?}")));
        return err_receiver;
    }
    receiver
}

#[cfg(not(target_arch = "wasm32"))]
pub fn export_to_path(
    ctx: &eframe::egui::Context,
    path: PathBuf,
    contents: Vec<u8>,
    stl: bool,
) -> Pending {
    run(ctx, move || {
        let extension = if stl { "stl" } else { "obj" };
        write(&extension_path(path, extension)?, &contents)?;
        Ok(Outcome::Exported)
    })
}

#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub fn export(
    _ctx: &eframe::egui::Context,
    _name: String,
    _contents: Vec<u8>,
    _stl: bool,
) -> Pending {
    let (sender, receiver) = mpsc::channel();
    let _ = sender.send(Ok(Outcome::Cancelled));
    receiver
}

#[cfg(target_arch = "wasm32")]
pub fn export(ctx: &eframe::egui::Context, name: String, contents: Vec<u8>, stl: bool) -> Pending {
    let (sender, receiver) = mpsc::channel();
    let result = download_bytes(
        &exchange_name(&name, if stl { "stl" } else { "obj" }),
        &contents,
        "application/octet-stream",
    )
    .map(|_| Outcome::Exported)
    .map_err(|e| {
        e.as_string()
            .unwrap_or_else(|| "The browser could not download this OBJ file".into())
    });
    let _ = sender.send(result);
    ctx.request_repaint();
    receiver
}

/// Prepare the complete next document on the file-operation thread on desktop;
/// committing its result on the UI thread only swaps owned models.
pub fn prepare_import(
    mut model: ketchup_core::Model,
    imported: &ketchup_core::Model,
) -> Result<Outcome, String> {
    imported.validate().map_err(str::to_owned)?;
    let selected = model.paste_entities(imported, ketchup_core::DVec3::ZERO);
    if selected.is_empty() {
        return Err("The imported geometry cannot be inserted in this editing context".into());
    }
    model.validate().map_err(str::to_owned)?;
    Ok(Outcome::Imported(Box::new(model), selected))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn export_cutting_plan_to_path(
    ctx: &eframe::egui::Context,
    path: PathBuf,
    name: String,
    model: ketchup_core::Model,
    selected: std::collections::BTreeSet<u64>,
    layout_settings: crate::cutting_layout::Settings,
) -> Pending {
    run(ctx, move || {
        layout_settings.validate()?;
        let plan = crate::cutting_plan::CuttingPlan::collect(&model, &selected)?;
        let contents = plan.pdf_with_layout(&name, !selected.is_empty(), true, layout_settings);
        let path = extension_path(path, "pdf")?;
        write(&path, &contents)?;
        open_pdf(&path).map_err(|error| {
            format!(
                "PDF saved to {}, but its viewer could not be opened: {error}",
                path.display()
            )
        })?;
        Ok(Outcome::Exported)
    })
}

#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub fn export_cutting_plan(
    ctx: &eframe::egui::Context,
    name: String,
    model: ketchup_core::Model,
    selected: std::collections::BTreeSet<u64>,
    layout_settings: crate::cutting_layout::Settings,
) -> Pending {
    run(ctx, move || {
        layout_settings.validate()?;
        let plan = crate::cutting_plan::CuttingPlan::collect(&model, &selected)?;
        let _contents = plan.pdf_with_layout(&name, !selected.is_empty(), true, layout_settings);
        Ok(Outcome::Cancelled)
    })
}

#[cfg(target_arch = "wasm32")]
pub fn export_cutting_plan(
    ctx: &eframe::egui::Context,
    name: String,
    model: ketchup_core::Model,
    selected: std::collections::BTreeSet<u64>,
    layout_settings: crate::cutting_layout::Settings,
) -> Pending {
    let (sender, receiver) = mpsc::channel();
    let result = layout_settings
        .validate()
        .and_then(|()| crate::cutting_plan::CuttingPlan::collect(&model, &selected))
        .and_then(|plan| {
            open_pdf(&plan.pdf_with_layout(&name, !selected.is_empty(), true, layout_settings))
                .map(|_| Outcome::Exported)
                .map_err(|e| {
                    e.as_string()
                        .unwrap_or_else(|| "The browser could not open this PDF".into())
                })
        });
    let _ = sender.send(result);
    ctx.request_repaint();
    receiver
}

#[cfg(not(target_arch = "wasm32"))]
pub fn export_screenshots_to_path(
    ctx: &eframe::egui::Context,
    path: PathBuf,
    name: String,
    settings: crate::screenshots::Settings,
    pages: Vec<image::RgbaImage>,
) -> Pending {
    run(ctx, move || {
        let export = crate::screenshots::encode_with_title(&settings, &pages, &name)?;
        write(&extension_path(path, export.extension)?, &export.bytes)?;
        Ok(Outcome::Exported)
    })
}

#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub fn export_screenshots(
    _ctx: &eframe::egui::Context,
    _name: String,
    _settings: crate::screenshots::Settings,
    _pages: Vec<image::RgbaImage>,
) -> Pending {
    let (sender, receiver) = mpsc::channel();
    let _ = sender.send(Ok(Outcome::Cancelled));
    receiver
}

#[cfg(target_arch = "wasm32")]
pub fn export_screenshots(
    ctx: &eframe::egui::Context,
    name: String,
    settings: crate::screenshots::Settings,
    pages: Vec<image::RgbaImage>,
) -> Pending {
    let (sender, receiver) = mpsc::channel();
    let result = crate::screenshots::encode_with_title(&settings, &pages, &name).and_then(|export| {
        download_bytes(
            &exchange_name(&format!("{name} - screenshot"), export.extension),
            &export.bytes,
            export.mime,
        )
        .map(|_| Outcome::Exported)
        .map_err(|e| {
            e.as_string()
                .unwrap_or_else(|| "The browser could not download this screenshot".into())
        })
    });
    let _ = sender.send(result);
    ctx.request_repaint();
    receiver
}

#[cfg(not(target_arch = "wasm32"))]
pub fn export_estimate_pdf_to_path(
    ctx: &eframe::egui::Context,
    path: PathBuf,
    contents: Vec<u8>,
) -> Pending {
    run(ctx, move || {
        let path = extension_path(path, "pdf")?;
        write(&path, &contents)?;
        open_pdf(&path).map_err(|error| {
            format!(
                "PDF saved to {}, but its viewer could not be opened: {error}",
                path.display()
            )
        })?;
        Ok(Outcome::Exported)
    })
}

#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub fn export_estimate_pdf(
    _ctx: &eframe::egui::Context,
    _name: String,
    _contents: Vec<u8>,
) -> Pending {
    let (sender, receiver) = mpsc::channel();
    let _ = sender.send(Ok(Outcome::Cancelled));
    receiver
}

#[cfg(target_arch = "wasm32")]
pub fn export_estimate_pdf(
    ctx: &eframe::egui::Context,
    _name: String,
    contents: Vec<u8>,
) -> Pending {
    let (sender, receiver) = mpsc::channel();
    // Opening synchronously preserves browser user activation for the PDF viewer.
    let result = open_pdf(&contents)
        .map(|_| Outcome::Exported)
        .map_err(|error| {
            error
                .as_string()
                .unwrap_or_else(|| "The browser could not open this PDF".into())
        });
    let _ = sender.send(result);
    ctx.request_repaint();
    receiver
}
