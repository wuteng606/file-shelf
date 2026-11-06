use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, State, Theme, WebviewWindow,
    WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
#[cfg(target_os = "macos")]
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};

#[cfg(target_os = "macos")]
mod macos_overlay;

#[cfg(target_os = "macos")]
const DEFAULT_SHORTCUT: &str = "Alt+Command+Space";
#[cfg(not(target_os = "macos"))]
const DEFAULT_SHORTCUT: &str = "Control+Alt+Space";

/* ---------------- 数据模型 ---------------- */

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub child_count: Option<u32>,
    pub kind: String,
    pub added_at: i64,
    pub missing: bool,
}

fn default_shortcut() -> String {
    DEFAULT_SHORTCUT.to_string()
}

fn default_theme() -> String {
    "auto".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_shortcut")]
    pub shortcut: String,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcut: default_shortcut(),
            autostart: false,
            theme: default_theme(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default, skip)]
    pub notice: Option<String>,
}

/* ---------------- 持久化 ---------------- */

fn data_file(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let _ = fs::create_dir_all(&dir);
    dir.join("state.json")
}

fn load_state(app: &AppHandle) -> AppState {
    fs::read_to_string(data_file(app))
        .ok()
        .and_then(|s| serde_json::from_str::<AppState>(&s).ok())
        .unwrap_or_default()
}

fn save_state(app: &AppHandle, st: &AppState) {
    let path = data_file(app);
    let tmp = path.with_extension("json.tmp");
    if let Ok(json) = serde_json::to_string_pretty(st) {
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
}

/* ---------------- 文件扫描 ---------------- */

fn kind_of(path: &Path, is_dir: bool) -> String {
    if is_dir {
        return "folder".into();
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let kind = match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "tif" | "tiff" | "bmp"
        | "svg" | "avif" | "raw" | "dng" => "image",
        "mp4" | "mov" | "m4v" | "avi" | "mkv" | "webm" | "flv" | "wmv" => "video",
        "mp3" | "wav" | "aac" | "flac" | "m4a" | "aiff" | "ogg" => "audio",
        "pdf" => "pdf",
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "dmg" | "iso" => "zip",
        "sketch" | "fig" | "xd" | "psd" | "ai" | "afdesign" | "framer" | "blend" | "indd" => {
            "design"
        }
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "json" | "html" | "css" | "scss" | "less"
        | "py" | "rs" | "go" | "swift" | "rb" | "java" | "c" | "h" | "cpp" | "sh" | "yml"
        | "yaml" | "toml" => "code",
        "doc" | "docx" | "pages" | "txt" | "md" | "rtf" | "xls" | "xlsx" | "numbers" | "ppt"
        | "pptx" | "key" | "csv" => "doc",
        _ => "other",
    };
    kind.to_string()
}

fn make_item(path: &Path) -> Option<Item> {
    let meta = fs::symlink_metadata(path).ok()?;
    let is_dir = meta.is_dir();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let (size, child_count) = if is_dir {
        let count = fs::read_dir(path)
            .ok()
            .map(|rd| rd.flatten().count().min(u32::MAX as usize) as u32);
        (None, count)
    } else {
        (Some(meta.len()), None)
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0);

    Some(Item {
        id: format!("{now}"),
        path: path.to_string_lossy().to_string(),
        name,
        is_dir,
        size,
        child_count,
        kind: kind_of(path, is_dir),
        added_at: (now / 1_000_000) as i64,
        missing: false,
    })
}

/* ---------------- 窗口定位与显隐 ---------------- */

const TRAY_ID: &str = "shelf-tray";

/// 面板是不是「我们主动打开」的状态。
/// 不能只信 is_visible()：窗口被系统挤到别的 Space 时它仍报 true，
/// 下次点击就会变成 hide()，表现成「点了没反应」。
static PANEL_SHOWN: AtomicBool = AtomicBool::new(false);

/// 上一次拿到的有效托盘矩形（启动瞬间 tray.rect() 会返回屏幕底部的垃圾值）
static TRAY_RECT: Mutex<Option<(f64, f64, f64, f64)>> = Mutex::new(None);

/// 托盘图标在屏幕上的矩形（物理像素）：x, y, w, h
fn tray_rect(app: &AppHandle, scale: f64) -> Option<(f64, f64, f64, f64)> {
    let tray = app.tray_by_id(TRAY_ID)?;
    let rect = tray.rect().ok().flatten()?;
    let (x, y) = match rect.position {
        tauri::Position::Physical(p) => (p.x as f64, p.y as f64),
        tauri::Position::Logical(p) => (p.x * scale, p.y * scale),
    };
    let (w, h) = match rect.size {
        tauri::Size::Physical(s) => (s.width as f64, s.height as f64),
        tauri::Size::Logical(s) => (s.width * scale, s.height * scale),
    };
    Some((x, y, w, h))
}

/// 菜单栏只会在某块屏幕的顶部，据此判断 tray.rect() 是不是还没布局好。
fn tray_rect_sane(app: &AppHandle, r: (f64, f64, f64, f64), scale: f64) -> bool {
    let Ok(monitors) = app.available_monitors() else {
        return true;
    };
    monitors.iter().any(|m| {
        let p = m.position();
        let s = m.size();
        let (mx, my) = (p.x as f64, p.y as f64);
        let (mw, mh) = (s.width as f64, s.height as f64);
        if cfg!(target_os = "macos") {
            // 菜单栏只会在某块屏幕的顶部
            r.0 >= mx - 60.0 && r.0 <= mx + mw && r.1 >= my - 60.0 && r.1 <= my + 120.0 * scale
        } else {
            // Windows 任务栏可能在任意一边，只要托盘矩形落在屏幕范围内即可
            r.0 >= mx - 80.0 && r.0 <= mx + mw + 80.0 && r.1 >= my - 80.0 && r.1 <= my + mh + 80.0
        }
    })
}

/// 取托盘矩形：优先用缓存的（上次点击拿到的准确值），拿不到就现查并校验
fn resolve_tray_rect(app: &AppHandle, scale: f64) -> Option<(f64, f64, f64, f64)> {
    if let Some(fresh) = tray_rect(app, scale) {
        if tray_rect_sane(app, fresh, scale) {
            *TRAY_RECT.lock().unwrap() = Some(fresh);
            return Some(fresh);
        }
    }
    *TRAY_RECT.lock().unwrap()
}

/// 选出菜单栏所在的那块屏幕（多显示器时不能想当然用主屏）
fn panel_monitor(
    app: &AppHandle,
    win: &WebviewWindow,
    tray: Option<(f64, f64, f64, f64)>,
) -> Option<Monitor> {
    if let Some((tx, ty, tw, _)) = tray {
        if let Ok(monitors) = app.available_monitors() {
            let mut same_column: Option<Monitor> = None;
            let cx = tx + tw / 2.0;
            for m in monitors {
                let p = m.position();
                let s = m.size();
                let left = p.x as f64;
                let right = left + s.width as f64;
                if cx < left || cx > right {
                    continue;
                }
                let top = p.y as f64;
                let bottom = top + s.height as f64;
                // 托盘在菜单栏上，允许 80pt 纵向余量（菜单栏自动隐藏时坐标可能为负）
                if ty >= top - 80.0 && ty <= bottom {
                    return Some(m);
                }
                if same_column.is_none() {
                    same_column = Some(m);
                }
            }
            if same_column.is_some() {
                return same_column;
            }
        }
    }

    win.current_monitor()
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
}

/// 把面板贴到菜单栏图标正下方，水平居中于图标（macOS：菜单栏在屏幕顶部）
#[cfg(target_os = "macos")]
fn panel_position(
    monitor: &Monitor,
    tray: Option<(f64, f64, f64, f64)>,
    win_w: f64,
    win_h: f64,
) -> (f64, f64) {
    let scale = monitor.scale_factor();
    let mpos = monitor.position();
    let msize = monitor.size();
    let margin = 8.0 * scale;

    let left = mpos.x as f64;
    let top = mpos.y as f64;
    let right = left + msize.width as f64;
    let bottom = top + msize.height as f64;

    let (center_x, anchor_y) = match tray {
        Some((x, y, w, h)) => (x + w / 2.0, y + h),
        // 拿不到图标位置时退回右上角
        None => (right - win_w / 2.0 - 10.0 * scale, top),
    };

    let min_x = left + margin;
    let max_x = (right - win_w - margin).max(min_x);
    let x = (center_x - win_w / 2.0).clamp(min_x, max_x);

    // 全屏应用时菜单栏会自动隐藏，托盘矩形可能落在屏幕外（y 为负），这里兜底贴顶
    let mut y = anchor_y + 6.0 * scale;
    if y < top + margin {
        y = top + margin;
    }
    if y + win_h > bottom - margin {
        y = (bottom - win_h - margin).max(top + margin);
    }
    (x, y)
}

/// Windows：任务栏通常在屏幕底部，面板贴在托盘图标**上方**，并避开任务栏
#[cfg(target_os = "windows")]
fn panel_position(
    monitor: &Monitor,
    tray: Option<(f64, f64, f64, f64)>,
    win_w: f64,
    win_h: f64,
) -> (f64, f64) {
    let scale = monitor.scale_factor();
    let area = monitor.work_area(); // 已排除任务栏
    let margin = 8.0 * scale;

    let left = area.position.x as f64;
    let top = area.position.y as f64;
    let right = left + area.size.width as f64;
    let bottom = top + area.size.height as f64;

    let min_x = left + margin;
    let max_x = (right - win_w - margin).max(min_x);

    let x = match tray {
        Some((tx, _, tw, _)) => (tx + tw / 2.0 - win_w / 2.0).clamp(min_x, max_x),
        None => (right - win_w - margin).max(min_x),
    };

    let y = match tray {
        Some((_, ty, _, _)) => (ty - win_h - 6.0 * scale).max(top + margin),
        None => (bottom - win_h - margin).max(top + margin),
    };

    (x, y.min((bottom - win_h - margin).max(top + margin)))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn panel_position(
    monitor: &Monitor,
    _tray: Option<(f64, f64, f64, f64)>,
    win_w: f64,
    win_h: f64,
) -> (f64, f64) {
    let area = monitor.work_area();
    let margin = 8.0 * monitor.scale_factor();
    (
        (area.position.x as f64 + area.size.width as f64 - win_w - margin).max(margin),
        (area.position.y as f64 + margin),
    )
}

fn place_panel(app: &AppHandle, win: &WebviewWindow) {
    let scale_guess = win.scale_factor().unwrap_or(1.0);
    let tray = resolve_tray_rect(app, scale_guess);

    let Some(monitor) = panel_monitor(app, win, tray) else {
        return;
    };
    let Ok(wsize) = win.outer_size() else {
        return;
    };

    let (x, y) = panel_position(&monitor, tray, wsize.width as f64, wsize.height as f64);

    if std::env::var("SHELF_DEBUG_OVERLAY").is_ok() {
        eprintln!(
            "[place] tray={:?} win={}x{} monitor=({},{}) {}x{} scale={} -> ({}, {})",
            tray,
            wsize.width,
            wsize.height,
            monitor.position().x,
            monitor.position().y,
            monitor.size().width,
            monitor.size().height,
            monitor.scale_factor(),
            x.round(),
            y.round()
        );
    }

    let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// 显示面板（不做 toggle）
fn show_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };

    place_panel(app, &win);
    PANEL_SHOWN.store(true, Ordering::Relaxed);
    let _ = app.emit("shelf:refresh", ());

    // macOS：窗口必须先变成「非激活 NSPanel」才能浮在全屏应用上，
    // 并且显示时不能激活 App（否则会把当前全屏 Space 顶掉）。
    #[cfg(target_os = "macos")]
    {
        macos_overlay::show(&win);

        // 全屏下点菜单栏图标，系统会把菜单栏升起再收回，这个重排可能把面板挤到后面，
        // 隔一小会儿再叠一次最前（万一这期间用户已经收起了，就跳过）。
        let w = win.clone();
        let h = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            if PANEL_SHOWN.load(Ordering::Relaxed) {
                macos_overlay::raise(&w);
            }
            // 启动瞬间菜单栏还没布局好，tray.rect() 会给垃圾值，稍后再贴一次
            std::thread::sleep(std::time::Duration::from_millis(450));
            if PANEL_SHOWN.load(Ordering::Relaxed) {
                place_panel(&h, &w);
            }
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

fn toggle_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };

    if PANEL_SHOWN.load(Ordering::Relaxed) && win.is_visible().unwrap_or(false) {
        PANEL_SHOWN.store(false, Ordering::Relaxed);
        let _ = win.hide();
        return;
    }

    show_window(app);
}

fn apply_vibe(app: &AppHandle, dark: bool) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let _ = app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        {
            let material = if dark {
                NSVisualEffectMaterial::HudWindow
            } else {
                NSVisualEffectMaterial::Popover
            };
            let _ = apply_vibrancy(&win, material, Some(NSVisualEffectState::Active), Some(20.0));
        }
        #[cfg(target_os = "windows")]
        {
            // Windows 用 Acrylic；Win10 早期版本不支持时静默失败，CSS 半透明背景兜底
            let tint = if dark {
                (20, 20, 24, 190)
            } else {
                (246, 247, 251, 190)
            };
            let _ = window_vibrancy::apply_acrylic(&win, Some(tint));
        }
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            let _ = (&win, dark);
        }
    });
}

fn apply_theme(app: &AppHandle, pref: &str) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let theme = match pref {
        "dark" => Some(Theme::Dark),
        "light" => Some(Theme::Light),
        _ => None,
    };
    let _ = win.set_theme(theme);
    let dark = match win.theme() {
        Ok(Theme::Dark) => true,
        Ok(Theme::Light) => false,
        _ => pref == "dark",
    };
    apply_vibe(app, dark);
}

#[cfg(target_os = "macos")]
fn pretty_accel(accel: &str) -> String {
    accel
        .replace("Command", "⌘")
        .replace("Control", "⌃")
        .replace("Option", "⌥")
        .replace("Alt", "⌥")
        .replace("Shift", "⇧")
        .replace('+', "")
}

#[cfg(not(target_os = "macos"))]
fn pretty_accel(accel: &str) -> String {
    accel
        .replace("Command", "Win")
        .replace("Control", "Ctrl")
        .replace("Option", "Alt")
        .replace('+', " + ")
}

fn register_shortcut(app: &AppHandle, accel: &str) -> Result<(), String> {
    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(accel, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                toggle_window(&handle);
            }
        })
        .map_err(|e| e.to_string())
}

/* ---------------- 命令 ---------------- */

#[tauri::command]
fn take_notice(state: State<'_, Mutex<AppState>>) -> Option<String> {
    state.lock().unwrap().notice.take()
}

#[tauri::command]
fn get_state(state: State<'_, Mutex<AppState>>) -> AppState {
    state.lock().unwrap().clone()
}

#[tauri::command]
fn add_paths(
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<usize, String> {
    let mut st = state.lock().unwrap();
    let mut added = 0usize;

    for p in paths {
        if st.items.iter().any(|i| i.path == p) {
            continue;
        }
        let mut item = match make_item(Path::new(&p)) {
            Some(v) => v,
            None => continue,
        };
        item.id = format!("{}_{}", item.id, added);
        st.items.push(item);
        added += 1;
    }

    if added > 0 {
        save_state(&app, &st);
    }
    Ok(added)
}

#[tauri::command]
fn remove_item(id: String, app: AppHandle, state: State<'_, Mutex<AppState>>) {
    let mut st = state.lock().unwrap();
    st.items.retain(|i| i.id != id);
    save_state(&app, &st);
}

#[tauri::command]
fn clear_items(app: AppHandle, state: State<'_, Mutex<AppState>>) {
    let mut st = state.lock().unwrap();
    st.items.clear();
    save_state(&app, &st);
}

#[tauri::command]
fn recheck(app: AppHandle, state: State<'_, Mutex<AppState>>) -> bool {
    let mut st = state.lock().unwrap();
    let mut changed = false;
    for item in st.items.iter_mut() {
        let missing = !Path::new(&item.path).exists();
        if missing != item.missing {
            item.missing = missing;
            changed = true;
        }
    }
    if changed {
        save_state(&app, &st);
    }
    changed
}

/* ---------------- 跨平台系统调用 ---------------- */

#[cfg(target_os = "macos")]
fn sys_reveal(path: &str) -> std::io::Result<()> {
    std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "windows")]
fn sys_reveal(path: &str) -> std::io::Result<()> {
    // explorer /select,"C:\path with space\file.txt" —— 路径带空格也能选中
    std::process::Command::new("explorer")
        .arg(format!("/select,{path}"))
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "linux")]
fn sys_reveal(path: &str) -> std::io::Result<()> {
    let dir = std::path::Path::new(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    std::process::Command::new("xdg-open").arg(dir).spawn().map(|_| ())
}

#[cfg(target_os = "macos")]
fn sys_open(path: &str) -> std::io::Result<()> {
    std::process::Command::new("open").arg(path).spawn().map(|_| ())
}

#[cfg(target_os = "windows")]
fn sys_open(path: &str) -> std::io::Result<()> {
    std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "linux")]
fn sys_open(path: &str) -> std::io::Result<()> {
    std::process::Command::new("xdg-open").arg(path).spawn().map(|_| ())
}

#[tauri::command]
fn reveal(path: String) -> Result<(), String> {
    sys_reveal(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    sys_open(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn hide_window(app: AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
}

#[tauri::command]
fn open_data_dir(app: AppHandle) -> Result<(), String> {
    let file = data_file(&app);
    let dir = file.parent().map(|p| p.to_path_buf()).unwrap_or(file);
    sys_open(&dir.to_string_lossy()).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_shortcut(
    shortcut: String,
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    if shortcut.trim().is_empty() {
        return Err("快捷键不能为空".into());
    }
    let old = state.lock().unwrap().settings.shortcut.clone();

    let _ = app.global_shortcut().unregister_all();
    match register_shortcut(&app, &shortcut) {
        Ok(()) => {
            let mut st = state.lock().unwrap();
            st.settings.shortcut = shortcut;
            save_state(&app, &st);
            Ok(())
        }
        Err(e) => {
            let _ = register_shortcut(&app, &old);
            Err(e)
        }
    }
}

#[tauri::command]
fn suspend_shortcut(app: AppHandle) {
    let _ = app.global_shortcut().unregister_all();
}

#[tauri::command]
fn resume_shortcut(app: AppHandle, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let accel = state.lock().unwrap().settings.shortcut.clone();
    let _ = app.global_shortcut().unregister_all();
    register_shortcut(&app, &accel)
}

#[tauri::command]
fn set_autostart(
    enabled: bool,
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    if enabled {
        app.autolaunch().enable().map_err(|e| e.to_string())?;
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())?;
    }
    let mut st = state.lock().unwrap();
    st.settings.autostart = enabled;
    save_state(&app, &st);
    Ok(())
}

#[tauri::command]
fn set_theme_pref(
    theme: String,
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let pref = theme.as_str();
    if !matches!(pref, "auto" | "dark" | "light") {
        return Err("未知主题".into());
    }
    apply_theme(&app, pref);
    let mut st = state.lock().unwrap();
    st.settings.theme = pref.to_string();
    save_state(&app, &st);
    Ok(())
}

#[tauri::command]
fn apply_theme_cmd(theme: String, app: AppHandle) {
    apply_theme(&app, theme.as_str());
}

/* ---------------- 启动 ---------------- */

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 已经有一个实例在跑：直接把面板叫出来
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || show_window(&handle));
        }))
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_state,
            take_notice,
            add_paths,
            remove_item,
            clear_items,
            recheck,
            reveal,
            open_path,
            hide_window,
            open_data_dir,
            set_shortcut,
            suspend_shortcut,
            resume_shortcut,
            set_autostart,
            set_theme_pref,
            apply_theme_cmd
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // 纯菜单栏应用：不出现在 Dock
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let mut loaded = load_state(&handle);
            loaded.settings.autostart = app.autolaunch().is_enabled().unwrap_or(false);
            let should_show = !std::env::args().any(|a| a == "--hidden");

            if register_shortcut(&handle, &loaded.settings.shortcut).is_err() {
                loaded.notice = Some(format!(
                    "快捷键 {} 被占用，请在设置里换一个",
                    pretty_accel(&loaded.settings.shortcut)
                ));
            }

            app.manage(Mutex::new(loaded.clone()));

            apply_theme(&handle, &loaded.settings.theme);

            // 菜单栏图标
            let show_item = MenuItem::with_id(app, "show", "显示暂存架", true, None::<&str>)?;
            let clear_item = MenuItem::with_id(app, "clear", "清空列表", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &show_item,
                    &clear_item,
                    &PredefinedMenuItem::separator(app)?,
                    &quit_item,
                ],
            )?;

            let mut tray = TrayIconBuilder::with_id(TRAY_ID)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_window(app),
                    "clear" => {
                        if let Some(state) = app.try_state::<Mutex<AppState>>() {
                            let snapshot = {
                                let mut st = state.lock().unwrap();
                                st.items.clear();
                                st.clone()
                            };
                            save_state(app, &snapshot);
                        }
                        let _ = app.emit("shelf:changed", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_window(tray.app_handle());
                    }
                });

            // 菜单栏图标：macOS 用模板图（纯黑 + alpha），系统按明暗自动反色，
            // 比直接塞彩色 App 图标更贴原生；Windows 托盘仍用彩色图标。
            #[cfg(target_os = "macos")]
            {
                let img = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;
                tray = tray.icon(img).icon_as_template(true);
            }
            #[cfg(not(target_os = "macos"))]
            {
                if let Some(icon) = app.default_window_icon() {
                    tray = tray.icon(icon.clone());
                }
            }
            tray.build(app)?;

            if should_show {
                show_window(&handle);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {});
}
