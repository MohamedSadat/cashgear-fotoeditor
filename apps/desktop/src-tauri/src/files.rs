use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{
    ipc::{InvokeBody, Request, Response},
    AppHandle, Emitter, Manager, Resource, ResourceId, State, WebviewWindow,
};
use tauri_plugin_dialog::DialogExt;
use tempfile::NamedTempFile;

const CHUNK: usize = 1024 * 1024;
const MAX_IMAGE: u64 = 128 * CHUNK as u64;
const MAX_PROJECT: u64 = 4 * 1024 * CHUNK as u64;
const RECENT_LIMIT: usize = 10;
const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "psd",
];
pub const OPEN_EVENT: &str = "desktop-open-request";

type Result<T> = std::result::Result<T, String>;

fn file_error(error: impl std::fmt::Display) -> String {
    format!("Windows file access failed: {error}")
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Image,
    Project,
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn file_kind(path: &Path) -> Option<FileKind> {
    let ext = extension(path);
    if ext == "cgfoto" {
        Some(FileKind::Project)
    } else {
        IMAGE_EXTENSIONS
            .contains(&ext.as_str())
            .then_some(FileKind::Image)
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn file_limit(path: &Path) -> u64 {
    if file_kind(path) == Some(FileKind::Project) {
        MAX_PROJECT
    } else {
        MAX_IMAGE
    }
}

struct Location {
    path: PathBuf,
    writable: bool,
}
impl Resource for Location {}

struct Input(Mutex<File>);
impl Resource for Input {}

struct Output {
    path: PathBuf,
    file: Mutex<Option<(NamedTempFile, u64)>>,
    limit: u64,
}
impl Resource for Output {}

#[derive(Serialize)]
pub struct PickedFile {
    id: ResourceId,
    name: String,
}

#[derive(Serialize)]
pub struct OpenFile {
    id: ResourceId,
    size: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct RecentPath {
    id: String,
    path: PathBuf,
}

#[derive(Serialize)]
pub struct RecentFile {
    id: String,
    name: String,
    kind: FileKind,
}

#[derive(Default)]
pub struct Files {
    pending: Mutex<VecDeque<PathBuf>>,
    catalog: Mutex<()>,
}

fn add_location(window: &WebviewWindow, path: PathBuf, writable: bool) -> PickedFile {
    let name = file_name(&path);
    let id = window.resources_table().add(Location { path, writable });
    PickedFile { id, name }
}

pub fn queue_open(app: &AppHandle, paths: impl IntoIterator<Item = PathBuf>) {
    let files = app.state::<Files>();
    let mut pending = files.pending.lock().unwrap();
    for path in paths {
        if path.is_absolute() && file_kind(&path).is_some() && !pending.contains(&path) {
            if pending.len() == RECENT_LIMIT {
                break;
            }
            pending.push_back(path);
        }
    }
    let queued = !pending.is_empty();
    drop(pending);
    if queued {
        let _ = app.emit_to("main", OPEN_EVENT, ());
    }
}

pub fn launch_paths(args: impl IntoIterator<Item = PathBuf>, cwd: &Path) -> Vec<PathBuf> {
    args.into_iter()
        .filter(|path| !path.as_os_str().to_string_lossy().starts_with('-'))
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        })
        .collect()
}

#[tauri::command]
pub fn pending_open(files: State<'_, Files>) -> Option<String> {
    files
        .pending
        .lock()
        .unwrap()
        .front()
        .map(|path| file_name(path))
}

#[tauri::command]
pub fn take_pending_open(
    window: WebviewWindow,
    files: State<'_, Files>,
    open: bool,
) -> Option<PickedFile> {
    let path = files.pending.lock().unwrap().pop_front()?;
    open.then(|| add_location(&window, path, false))
}

#[tauri::command]
pub async fn pick_open(window: WebviewWindow, kind: FileKind) -> Result<Option<PickedFile>> {
    let selected = window
        .dialog()
        .file()
        .set_parent(&window)
        .set_title(match kind {
            FileKind::Project => "Open FotoEditor project",
            FileKind::Image => "Open image",
        })
        .add_filter(
            match kind {
                FileKind::Project => "FotoEditor project",
                FileKind::Image => "Supported images",
            },
            match kind {
                FileKind::Project => &["cgfoto"],
                FileKind::Image => IMAGE_EXTENSIONS,
            },
        )
        .blocking_pick_file();
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected.into_path().map_err(file_error)?;
    if file_kind(&path) != Some(kind) {
        return Err("Choose a supported file for this Open action.".into());
    }
    Ok(Some(add_location(&window, path, false)))
}

#[tauri::command]
pub async fn pick_save(
    window: WebviewWindow,
    suggested_name: String,
    format: String,
) -> Result<Option<PickedFile>> {
    let (label, extensions): (&str, &[&str]) = match format.as_str() {
        "cgfoto" => ("FotoEditor project", &["cgfoto"]),
        "png" => ("PNG image", &["png"]),
        "jpg" | "jpeg" => ("JPEG image", &["jpg", "jpeg"]),
        "webp" => ("WebP image", &["webp"]),
        _ => return Err("Unsupported save format.".into()),
    };
    let selected = window
        .dialog()
        .file()
        .set_parent(&window)
        .set_title(if format == "cgfoto" {
            "Save project"
        } else {
            "Export image"
        })
        .set_file_name(file_name(Path::new(&suggested_name)))
        .add_filter(label, extensions)
        .blocking_save_file();
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected.into_path().map_err(file_error)?;
    // The dialog must confirm overwrite of the actual destination.
    if !extensions.contains(&extension(&path).as_str()) {
        return Err(format!("Choose a file with the .{format} extension."));
    }
    Ok(Some(add_location(&window, path, true)))
}

async fn file_task<T: Send + 'static>(
    task: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(file_error)?
}

#[tauri::command]
pub async fn open_native(window: WebviewWindow, location: ResourceId) -> Result<OpenFile> {
    let location = window
        .resources_table()
        .get::<Location>(location)
        .map_err(file_error)?;
    let (file, size) = file_task(move || {
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // FILE_SHARE_READ. Keep the input stable across streamed reads.
            options.share_mode(1);
        }
        let file = options.open(&location.path).map_err(file_error)?;
        let metadata = file.metadata().map_err(file_error)?;
        if !metadata.is_file() {
            return Err("Choose a regular file.".into());
        }
        if metadata.len() > file_limit(&location.path) {
            return Err(if file_kind(&location.path) == Some(FileKind::Project) {
                "Projects must be at most 4 GiB"
            } else {
                "Image files must be at most 128 MiB"
            }
            .into());
        }
        Ok((file, metadata.len()))
    })
    .await?;
    let id = window.resources_table().add(Input(Mutex::new(file)));
    Ok(OpenFile { id, size })
}

#[tauri::command]
pub async fn read_native(
    window: WebviewWindow,
    id: ResourceId,
    offset: u64,
    length: usize,
) -> Result<Response> {
    if length == 0 || length > CHUNK || offset.saturating_add(length as u64) > MAX_PROJECT {
        return Err("Invalid file read range.".into());
    }
    let input = window
        .resources_table()
        .get::<Input>(id)
        .map_err(file_error)?;
    file_task(move || {
        let mut file = input.0.lock().unwrap();
        file.seek(SeekFrom::Start(offset)).map_err(file_error)?;
        let mut bytes = vec![0; length];
        file.read_exact(&mut bytes).map_err(file_error)?;
        Ok(Response::new(bytes))
    })
    .await
}

fn staging_file(destination: &Path) -> Result<NamedTempFile> {
    let parent = destination
        .parent()
        .ok_or("The destination has no parent folder.")?;
    tempfile::Builder::new()
        .prefix(".cashgear-fotoeditor-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(file_error)
}

fn publish(file: NamedTempFile, path: &Path) -> Result<()> {
    file.as_file().sync_all().map_err(file_error)?;
    file.persist(path).map_err(file_error)?;
    Ok(())
}

#[tauri::command]
pub async fn begin_native_write(window: WebviewWindow, location: ResourceId) -> Result<ResourceId> {
    let location = window
        .resources_table()
        .get::<Location>(location)
        .map_err(file_error)?;
    if !location.writable && file_kind(&location.path) != Some(FileKind::Project) {
        return Err("Choose an export destination with Save.".into());
    }
    let output = file_task(move || {
        Ok(Output {
            limit: file_limit(&location.path),
            path: location.path.clone(),
            file: Mutex::new(Some((staging_file(&location.path)?, 0))),
        })
    })
    .await?;
    let id = window.resources_table().add(output);
    Ok(id)
}

#[tauri::command]
pub async fn write_native(window: WebviewWindow, request: Request<'_>) -> Result<()> {
    let id: ResourceId = request
        .headers()
        .get("x-fotoeditor-file")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .ok_or("Missing output file handle.")?;
    let InvokeBody::Raw(chunk) = request.body() else {
        return Err("Expected binary file data.".into());
    };
    if chunk.is_empty() || chunk.len() > 4 * CHUNK {
        return Err("Invalid file write length.".into());
    }
    let output = window
        .resources_table()
        .get::<Output>(id)
        .map_err(file_error)?;
    let bytes = chunk.clone();
    file_task(move || {
        let mut open = output.file.lock().unwrap();
        let (file, size) = open.as_mut().ok_or("The output file is closed.")?;
        if *size + bytes.len() as u64 > output.limit {
            return Err("The output exceeds its file size limit.".into());
        }
        if let Err(error) = file.write_all(&bytes) {
            open.take();
            return Err(file_error(error));
        }
        *size += bytes.len() as u64;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn commit_native(window: WebviewWindow, id: ResourceId) -> Result<()> {
    let output = window
        .resources_table()
        .get::<Output>(id)
        .map_err(file_error)?;
    file_task(move || {
        let (temp, size) = output
            .file
            .lock()
            .unwrap()
            .take()
            .ok_or("The output file is closed.")?;
        if size == 0 {
            return Err("The output file is empty.".into());
        }
        publish(temp, &output.path)
    })
    .await
}

fn recent_path(window: &WebviewWindow) -> Result<PathBuf> {
    let dir = window.path().app_local_data_dir().map_err(file_error)?;
    fs::create_dir_all(&dir).map_err(file_error)?;
    Ok(dir.join("recent-files.json"))
}

fn load_recents(path: &Path) -> Result<Vec<RecentPath>> {
    match File::open(path) {
        Ok(file) => serde_json::from_reader(file.take(256 * 1024)).map_err(file_error),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(file_error(error)),
    }
}

fn recent_view(recents: Vec<RecentPath>) -> Vec<RecentFile> {
    recents
        .into_iter()
        .take(RECENT_LIMIT)
        .filter_map(|entry| {
            Some(RecentFile {
                id: entry.id,
                name: file_name(&entry.path),
                kind: file_kind(&entry.path)?,
            })
        })
        .collect()
}

fn save_recents(path: &Path, entries: &[RecentPath]) -> Result<()> {
    let mut file = staging_file(path)?;
    serde_json::to_writer(&mut file, entries).map_err(file_error)?;
    publish(file, path)
}

#[tauri::command]
pub async fn recent_files(window: WebviewWindow) -> Result<Vec<RecentFile>> {
    file_task(move || Ok(recent_view(load_recents(&recent_path(&window)?)?))).await
}

#[tauri::command]
pub async fn pick_recent(window: WebviewWindow, id: String) -> Result<PickedFile> {
    let path = recent_path(&window)?;
    let entry = file_task(move || {
        load_recents(&path)?
            .into_iter()
            .take(RECENT_LIMIT)
            .find(|entry| {
                entry.id == id && entry.path.is_absolute() && file_kind(&entry.path).is_some()
            })
            .ok_or("This file is missing from Recent files.".into())
    })
    .await?;
    Ok(add_location(&window, entry.path, false))
}

#[tauri::command]
pub async fn remember_native(
    window: WebviewWindow,
    location: ResourceId,
) -> Result<Vec<RecentFile>> {
    let location = window
        .resources_table()
        .get::<Location>(location)
        .map_err(file_error)?;
    file_task(move || {
        let state = window.state::<Files>();
        let _lock = state.catalog.lock().unwrap();
        let path = recent_path(&window)?;
        let mut list = load_recents(&path)?;
        let normalized = fs::canonicalize(&location.path).unwrap_or_else(|_| location.path.clone());
        let id = list
            .iter()
            .find(|entry| entry.path == normalized)
            .map(|entry| entry.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        list.retain(|entry| entry.path != normalized);
        list.insert(
            0,
            RecentPath {
                id,
                path: normalized,
            },
        );
        list.truncate(RECENT_LIMIT);
        save_recents(&path, &list)?;
        Ok(recent_view(list))
    })
    .await
}

#[tauri::command]
pub async fn forget_native(window: WebviewWindow, id: String) -> Result<Vec<RecentFile>> {
    file_task(move || {
        let state = window.state::<Files>();
        let _lock = state.catalog.lock().unwrap();
        let path = recent_path(&window)?;
        let mut list = load_recents(&path)?;
        list.retain(|entry| entry.id != id);
        save_recents(&path, &list)?;
        Ok(recent_view(list))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aborted_or_failed_publish_preserves_destination() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("test.cgfoto");
        fs::write(&destination, b"previous").unwrap();
        let mut temp = staging_file(&destination).unwrap();
        temp.write_all(b"incomplete").unwrap();
        drop(temp);
        assert_eq!(fs::read(&destination).unwrap(), b"previous");

        let locked = fs::OpenOptions::new()
            .read(true)
            .open(&destination)
            .unwrap();
        #[cfg(windows)]
        let locked = {
            use std::os::windows::fs::OpenOptionsExt;
            drop(locked);
            fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&destination)
                .unwrap()
        };
        let mut temp = staging_file(&destination).unwrap();
        temp.write_all(b"next").unwrap();
        #[cfg(windows)]
        {
            assert!(publish(temp, &destination).is_err());
            assert_eq!(fs::read(&destination).unwrap(), b"previous");
            temp = staging_file(&destination).unwrap();
            temp.write_all(b"next").unwrap();
        }
        drop(locked);
        publish(temp, &destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"next");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
