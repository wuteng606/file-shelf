//! macOS 全屏兼容：让面板浮在「全屏应用」之上。
//!
//! 背景：AppKit 窗口默认属于自己所在的 Space。当别的应用进入全屏（独占一个 Space）时，
//! 即使窗口 `alwaysOnTop`，窗口服务器也不会把它画进那个 Space —— 表现就是
//! 「点了菜单栏图标，有焦点但看不到窗口」。
//!
//! 实测（macOS 15）：普通 NSWindow 无论怎么设置 collectionBehavior / level，
//! 都无法进入其它应用的全屏 Space；能进去的只有 **带 `NonactivatingPanel` 的 NSPanel**，
//! 也就是 Spotlight / Raycast 那一类「不抢焦点也能打字」的浮动面板。
//!
//! 所以这里在运行时把窗口"变身"成 NSPanel 子类，需要三件事：
//! 1. 类换成 NSPanel 子类（并重写 `canBecomeKeyWindow`，否则无边框面板打不进字）；
//! 2. `styleMask` 加上 `NonactivatingPanel`：不激活 App 也能拿到键盘；
//! 3. `collectionBehavior` 加 `CanJoinAllSpaces | FullScreenAuxiliary`，层级抬到弹出菜单级。
//!
//! 注意：显示时不能再调 `NSApp.activate`（Tauri 的 `set_focus()` 会调），
//! 一旦激活 App，系统就会把当前全屏 Space 顶掉。`window.show()` 只做
//! `makeKeyAndOrderFront`，正好符合需要。

#![cfg(target_os = "macos")]

use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{msg_send, sel};
use objc2_app_kit::{
    NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask, NSPopUpMenuWindowLevel,
};
use tauri::WebviewWindow;

/// 无边框面板默认 `canBecomeKeyWindow = NO`，不重写就收不到键盘输入。
extern "C-unwind" fn can_become_key(_this: &AnyObject, _cmd: Sel) -> Bool {
    Bool::YES
}

/// 面板不该抢 main window 的位置（否则会影响主窗口的菜单 / 激活逻辑）。
extern "C-unwind" fn can_become_main(_this: &AnyObject, _cmd: Sel) -> Bool {
    Bool::NO
}

/// 运行时注册 `NSPanel` 子类，注册一次后按名字复用。
fn overlay_panel_class() -> Option<&'static AnyClass> {
    if let Some(cls) = AnyClass::get(c"FileShelfOverlayPanel") {
        return Some(cls);
    }
    let superclass = AnyClass::get(c"NSPanel")?;
    let mut builder = ClassBuilder::new(c"FileShelfOverlayPanel", superclass)?;
    unsafe {
        builder.add_method(
            sel!(canBecomeKeyWindow),
            can_become_key as extern "C-unwind" fn(_, _) -> _,
        );
        builder.add_method(
            sel!(canBecomeMainWindow),
            can_become_main as extern "C-unwind" fn(_, _) -> _,
        );
    }
    Some(builder.register())
}

fn raw_ns_window(window: &WebviewWindow) -> Option<*mut std::ffi::c_void> {
    match window.ns_window() {
        Ok(ptr) if !ptr.is_null() => Some(ptr),
        _ => None,
    }
}

/// 把窗口变成「全屏浮层」。必须在主线程调用。
pub fn make_overlay(window: &WebviewWindow) {
    let Some(ptr) = raw_ns_window(window) else {
        return;
    };

    // SAFETY: 指针来自窗口自身的 ns_window()，窗口存活期间有效；调用方保证在主线程。
    unsafe {
        let obj = ptr.cast::<AnyObject>();

        // 1. 换成 NSPanel 子类（NSPanel 才能进别的应用的全屏 Space）
        if let Some(cls) = overlay_panel_class() {
            objc2::ffi::object_setClass(obj, cls as *const AnyClass);
        }

        let ns: &NSWindow = &*ptr.cast::<NSWindow>();

        // 2. 非激活面板：不抢 App 激活也能成为 key window（能打字）
        ns.setStyleMask(ns.styleMask() | NSWindowStyleMask::NonactivatingPanel);

        // 3. 允许加入所有 Space，包括其它应用的全屏 Space
        ns.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );

        // 层级：普通窗口 0、菜单栏 24、弹出菜单 101。101 这一档已经压过全屏画面。
        ns.setLevel(NSPopUpMenuWindowLevel);

        let obj_ref: &AnyObject = &*obj;
        // 面板默认「App 失活就隐藏」，我们要自己控制显隐
        let _: () = msg_send![obj_ref, setHidesOnDeactivate: false];
        // 需要时立刻成为 key window（不用先点一下）
        let _: () = msg_send![obj_ref, setBecomesKeyOnlyIfNeeded: false];

        if debug_enabled() {
            eprintln!(
                "[overlay] {} styleMask={:?} behavior={:?} level={} on_active_space={}",
                window.label(),
                ns.styleMask(),
                ns.collectionBehavior(),
                ns.level(),
                ns.isOnActiveSpace(),
            );
        }
    }
}

fn debug_enabled() -> bool {
    std::env::var("SHELF_DEBUG_OVERLAY").is_ok()
}

/// 主线程里「打标记 + 显示 + 取键盘焦点」，全程不激活 App。
///
/// `window.show()` 在 macOS 上是 `makeKeyAndOrderFront`，不会把当前全屏 Space 顶掉；
/// 千万不要在这里改用 `set_focus()`（它会 `activateIgnoringOtherApps`）。
pub fn show(window: &WebviewWindow) {
    let win: WebviewWindow = window.clone();
    let _ = window.run_on_main_thread(move || {
        make_overlay(&win);
        let _ = win.show();
    });
}

/// 再叠一次到最前（`orderFrontRegardless` + 取 key）。
///
/// 全屏下点菜单栏图标时，系统会把菜单栏升起来又收回去，这个重排有可能把面板挤到后面；
/// 显示后 ~150ms 再叠一次最稳。
pub fn raise(window: &WebviewWindow) {
    let Some(ptr) = raw_ns_window(window) else {
        return;
    };
    let addr = ptr as usize;
    let _ = window.run_on_main_thread(move || {
        let ns: &NSWindow = unsafe { &*(addr as *mut NSWindow) };
        if !ns.isVisible() {
            return;
        }
        ns.orderFrontRegardless();
        if !ns.isKeyWindow() {
            ns.makeKeyWindow();
        }
    });
}
