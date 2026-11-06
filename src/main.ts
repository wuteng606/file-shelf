import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { startDrag } from "@crabnebula/tauri-plugin-drag";

type Kind = "folder" | "image" | "video" | "pdf" | "zip" | "design" | "code" | "audio" | "doc" | "other";

interface Item {
  id: string;
  path: string;
  name: string;
  isDir: boolean;
  size: number | null;
  childCount: number | null;
  kind: Kind;
  addedAt: number;
  missing: boolean;
}

interface Settings {
  shortcut: string;
  autostart: boolean;
  theme: "auto" | "dark" | "light";
}

interface AppState {
  items: Item[];
  settings: Settings;
}

const IS_MAC = navigator.userAgent.includes("Mac");
const REVEAL_LABEL = IS_MAC ? "在访达中显示" : "在资源管理器中显示";

const els = {
  shelf: document.getElementById("shelf")!,
  list: document.getElementById("list")!,
  count: document.getElementById("count")!,
  summary: document.getElementById("summary")!,
  clearBtn: document.getElementById("clearBtn") as HTMLButtonElement,
  settingsBtn: document.getElementById("settingsBtn")!,
  sheet: document.getElementById("sheet")!,
  closeSheet: document.getElementById("closeSheet")!,
  keycap: document.getElementById("keycap") as HTMLButtonElement,
  autostart: document.getElementById("autostartSwitch") as HTMLButtonElement,
  themeSeg: document.getElementById("themeSeg")!,
  toast: document.getElementById("toast")!,
};

let state: AppState = {
  items: [],
  settings: { shortcut: "Alt+Command+Space", autostart: false, theme: "auto" },
};

/* ---------- 图标 ---------- */
const ICONS: Record<string, { g: string; p: string }> = {
  folder: {
    g: "linear-gradient(158deg,#FFD466,#FF9E0B)",
    p: '<path d="M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2.2h7.5A2.5 2.5 0 0 1 21 9.7v8.8a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 18.5v-11Z" fill="#fff" opacity=".95"/>',
  },
  pdf: {
    g: "linear-gradient(158deg,#FF8A78,#E0342B)",
    p: '<path d="M6 2.5h7.2L18.5 8v13a1.2 1.2 0 0 1-1.2 1.2H6A1.2 1.2 0 0 1 4.8 21V3.7A1.2 1.2 0 0 1 6 2.5Z" fill="#fff"/><path d="M13.2 2.5 18.5 8h-4.4a.9.9 0 0 1-.9-.9V2.5Z" fill="#fff" opacity=".55"/>',
  },
  image: {
    g: "linear-gradient(158deg,#7DD3FC,#2F80ED)",
    p: '<rect x="3" y="4.5" width="18" height="15" rx="3" fill="#fff"/><circle cx="8.6" cy="9.8" r="1.7" fill="#2F80ED" opacity=".55"/><path d="M4.5 17.2l4.6-4.3a1.4 1.4 0 0 1 1.9 0l3 2.8 2.3-2a1.4 1.4 0 0 1 1.9 0l2.3 2.1v1.4a1.8 1.8 0 0 1-1.8 1.8H6.3a1.8 1.8 0 0 1-1.8-1.8v0Z" fill="#2F80ED" opacity=".45"/>',
  },
  video: {
    g: "linear-gradient(158deg,#C79BFF,#7C3AED)",
    p: '<rect x="3" y="5" width="18" height="14" rx="3" fill="#fff"/><path d="M10.4 9.3v5.4a.6.6 0 0 0 .92.5l4.2-2.7a.6.6 0 0 0 0-1l-4.2-2.7a.6.6 0 0 0-.92.5Z" fill="#7C3AED" opacity=".72"/>',
  },
  zip: {
    g: "linear-gradient(158deg,#7EE7B8,#059669)",
    p: '<path d="M4 4.5h16v4H4z" fill="#fff"/><path d="M5.2 8.5h13.6V20a1 1 0 0 1-1 1H6.2a1 1 0 0 1-1-1V8.5Z" fill="#fff" opacity=".82"/><rect x="10.3" y="11" width="3.4" height="4.6" rx="1.2" fill="#059669" opacity=".55"/>',
  },
  design: {
    g: "linear-gradient(158deg,#FDBA74,#F97316)",
    p: '<rect x="3.4" y="3.4" width="7.4" height="7.4" rx="2.2" fill="#fff"/><circle cx="16.6" cy="7.1" r="3.7" fill="#fff" opacity=".82"/><rect x="3.4" y="13.2" width="7.4" height="7.4" rx="2.2" fill="#fff" opacity=".68"/><circle cx="16.6" cy="16.9" r="3.7" fill="#fff" opacity=".5"/>',
  },
  code: {
    g: "linear-gradient(158deg,#8FD6FF,#0E86C7)",
    p: '<path d="M8.6 7.4 3.9 12l4.7 4.6M15.4 7.4 20.1 12l-4.7 4.6" stroke="#fff" stroke-width="2.1" fill="none" stroke-linecap="round" stroke-linejoin="round"/>',
  },
  audio: {
    g: "linear-gradient(158deg,#FDA4AF,#E11D48)",
    p: '<path d="M9 16.5V6.4l9-2v10.1" stroke="#fff" stroke-width="1.9" fill="none" stroke-linecap="round" stroke-linejoin="round"/><circle cx="7.4" cy="17" r="2.5" fill="#fff"/><circle cx="16.4" cy="15" r="2.5" fill="#fff"/>',
  },
  doc: {
    g: "linear-gradient(158deg,#93C5FD,#2563EB)",
    p: '<path d="M6 2.5h7.2L18.5 8v13a1.2 1.2 0 0 1-1.2 1.2H6A1.2 1.2 0 0 1 4.8 21V3.7A1.2 1.2 0 0 1 6 2.5Z" fill="#fff"/><path d="M8 12h8M8 15.5h5.5" stroke="#2563EB" stroke-width="1.4" stroke-linecap="round" opacity=".55"/>',
  },
  other: {
    g: "linear-gradient(158deg,#B8BEC9,#6B7280)",
    p: '<path d="M6 2.5h7.2L18.5 8v13a1.2 1.2 0 0 1-1.2 1.2H6A1.2 1.2 0 0 1 4.8 21V3.7A1.2 1.2 0 0 1 6 2.5Z" fill="#fff" opacity=".9"/>',
  },
};

const KIND_LABEL: Record<string, string> = {
  folder: "文件夹",
  image: "图像",
  video: "影片",
  pdf: "PDF 文稿",
  zip: "压缩包",
  design: "设计文稿",
  code: "代码",
  audio: "音频",
  doc: "文稿",
  other: "文件",
};

function tileHTML(kind: string): string {
  const ic = ICONS[kind] ?? ICONS.other;
  return `<div class="tile" style="background:${ic.g}"><svg viewBox="0 0 24 24">${ic.p}</svg></div>`;
}

function esc(s: string): string {
  return String(s).replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c] as string),
  );
}

function fmtSize(b: number | null): string {
  if (!b) return "";
  if (b >= 1073741824) return (b / 1073741824).toFixed(2) + " GB";
  if (b >= 1048576) return (b / 1048576).toFixed(1) + " MB";
  if (b >= 1024) return Math.round(b / 1024) + " KB";
  return b + " B";
}

function fmtTime(ts: number): string {
  const d = new Date(ts * 1000);
  const now = new Date();
  const sameDay = d.toDateString() === now.toDateString();
  const yest = new Date(now);
  yest.setDate(now.getDate() - 1);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  if (sameDay) return `今天 ${hh}:${mm}`;
  if (d.toDateString() === yest.toDateString()) return `昨天 ${hh}:${mm}`;
  return `${d.getMonth() + 1}月${d.getDate()}日 ${hh}:${mm}`;
}

/* ---------- 渲染 ---------- */
const EMPTY_HTML = `
  <div class="empty">
    <div class="eicon">
      <svg viewBox="0 0 24 24" fill="none">
        <path d="M12 15.5V4.5m0 0L8 8.5m4-4 4 4" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>
        <path d="M4 14.5v3.2A2.3 2.3 0 0 0 6.3 20h11.4a2.3 2.3 0 0 0 2.3-2.3v-3.2" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/>
      </svg>
    </div>
    <div class="etitle">拖入文件或文件夹</div>
    <div class="esub">临时放一下，随时拖出或清空<br>不会移动或删除原文件</div>
    <button class="pick" id="pickBtn">或选择文件…</button>
  </div>`;

function render() {
  const items = state.items;

  if (!items.length) {
    els.list.innerHTML = EMPTY_HTML;
    els.count.textContent = "0";
    els.summary.textContent = "空";
    els.clearBtn.disabled = true;
    document.getElementById("pickBtn")?.addEventListener("click", pickFiles);
    return;
  }

  els.list.innerHTML = items
    .map((it) => {
      const meta = it.missing
        ? "文件已移动或删除"
        : [
            it.isDir
              ? `${KIND_LABEL.folder}${it.childCount != null ? ` · ${it.childCount} 项` : ""}`
              : `${KIND_LABEL[it.kind] ?? "文件"}${it.size ? ` · ${fmtSize(it.size)}` : ""}`,
            fmtTime(it.addedAt),
          ].join(" · ");

      return `
      <div class="row${it.missing ? " missing" : ""}" data-id="${esc(it.id)}">
        ${tileHTML(it.kind)}
        <div class="info">
          <div class="name">${esc(it.name)}</div>
          <div class="meta">${esc(meta)}</div>
        </div>
        <div class="acts">
          <button class="act" title="${REVEAL_LABEL}" data-act="reveal">
            <svg viewBox="0 0 24 24" fill="none">
              <path d="M3.5 8.5v10a2 2 0 0 0 2 2h9" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/>
              <path d="M3.5 7.5A2 2 0 0 1 5.5 5.5h3l1.8 2h7.2a2 2 0 0 1 2 2v2" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>
              <path d="M20 13.5H12l2.4-2.4M20 13.5 17.6 16" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </button>
          <button class="act rm" title="移除" data-act="remove">
            <svg viewBox="0 0 24 24" fill="none">
              <path d="M7 7l10 10M17 7 7 17" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>
            </svg>
          </button>
        </div>
      </div>`;
    })
    .join("");

  els.count.textContent = String(items.length);

  const total = items.reduce((a, b) => a + (b.size ?? 0), 0);
  els.summary.textContent = `${items.length} 项 · ${total ? fmtSize(total) : "—"}`;
  els.clearBtn.disabled = false;
}

/* ---------- 数据同步 ---------- */
async function refresh() {
  state = await invoke<AppState>("get_state");
  render();
}

async function addPaths(paths: string[]) {
  if (!paths.length) return;
  const added = await invoke<number>("add_paths", { paths });
  await refresh();
  if (added === 0) toast("已经在暂存架里了");
  else toast(added === 1 ? "已暂存" : `已暂存 ${added} 项`);
}

/* ---------- Toast ---------- */
let toastTimer: number | undefined;
function toast(text: string, ms = 1600) {
  els.toast.textContent = text;
  els.toast.classList.add("show");
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => els.toast.classList.remove("show"), ms);
}

/* ---------- 拖出 ---------- */
const dragIcons: Record<string, string> = {};

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function makeDragIcon(ic: { g: string; p: string }): Promise<string> {
  const size = 96;
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext("2d")!;
  const colors = ic.g.match(/#[0-9A-Fa-f]{6}/g) ?? ["#8E8E93", "#55555A"];
  const grad = ctx.createLinearGradient(0, 0, size, size);
  grad.addColorStop(0, colors[0]);
  grad.addColorStop(1, colors[1] ?? colors[0]);

  ctx.save();
  ctx.shadowColor = "rgba(0,0,0,0.28)";
  ctx.shadowBlur = 8;
  ctx.shadowOffsetY = 2;
  roundRect(ctx, 10, 10, size - 20, size - 20, 24);
  ctx.fillStyle = grad;
  ctx.fill();
  ctx.restore();

  const done = () => canvas.toDataURL("image/png");
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      ctx.drawImage(img, 24, 24, size - 48, size - 48);
      resolve(done());
    };
    img.onerror = () => resolve(done());
    img.src =
      "data:image/svg+xml;utf8," +
      encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">${ic.p}</svg>`);
  });
}

async function warmDragIcons() {
  for (const [kind, ic] of Object.entries(ICONS)) {
    dragIcons[kind] = await makeDragIcon(ic);
  }
}

const FALLBACK_ICON =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

/* ---------- 列表交互 ---------- */
let pressStart: { id: string; x: number; y: number; armed: boolean } | null = null;

els.list.addEventListener("pointerdown", (e) => {
  const target = e.target as HTMLElement;
  if (target.closest(".act")) return;
  const row = target.closest(".row") as HTMLElement | null;
  if (!row) return;
  const item = state.items.find((x) => x.id === row.dataset.id);
  if (!item || item.missing) return;
  pressStart = { id: item.id, x: e.clientX, y: e.clientY, armed: true };
});

window.addEventListener("pointermove", (e) => {
  if (!pressStart?.armed) return;
  if (Math.hypot(e.clientX - pressStart.x, e.clientY - pressStart.y) < 7) return;
  pressStart.armed = false;
  const item = state.items.find((x) => x.id === pressStart!.id);
  if (!item) return;
  startDrag({ item: [item.path], icon: dragIcons[item.kind] ?? FALLBACK_ICON }).catch(() =>
    toast("拖出失败"),
  );
});

window.addEventListener("pointerup", () => {
  if (pressStart) pressStart.armed = false;
});

els.list.addEventListener("click", async (e) => {
  const target = e.target as HTMLElement;
  const btn = target.closest(".act") as HTMLElement | null;
  const row = target.closest(".row") as HTMLElement | null;
  if (!row) return;
  const id = row.dataset.id!;

  if (btn) {
    if (btn.dataset.act === "remove") {
      row.style.opacity = "0";
      row.style.transform = "translateX(14px)";
      await new Promise((r) => setTimeout(r, 150));
      await invoke("remove_item", { id });
      await refresh();
    } else {
      const item = state.items.find((x) => x.id === id);
      if (item) await invoke("reveal", { path: item.path }).catch((err) => toast(String(err)));
    }
  }
});

els.list.addEventListener("dblclick", async (e) => {
  const target = e.target as HTMLElement;
  if (target.closest(".act")) return;
  const row = target.closest(".row") as HTMLElement | null;
  if (!row) return;
  const item = state.items.find((x) => x.id === row.dataset.id);
  if (item && !item.missing) await invoke("open_path", { path: item.path }).catch((err) => toast(String(err)));
});

els.clearBtn.addEventListener("click", async () => {
  if (!state.items.length) return;
  const rows = Array.from(els.list.querySelectorAll<HTMLElement>(".row"));
  rows.forEach((r, i) => {
    setTimeout(() => {
      r.style.opacity = "0";
      r.style.transform = "translateX(14px)";
    }, i * 32);
  });
  setTimeout(async () => {
    await invoke("clear_items");
    await refresh();
    toast("已清空（文件仍在原处）");
  }, Math.min(rows.length * 32 + 160, 420));
});

async function pickFiles() {
  const picked = await openDialog({ multiple: true, directory: false, title: "选择要暂存的文件" });
  if (Array.isArray(picked)) await addPaths(picked);
  else if (typeof picked === "string") await addPaths([picked]);
}

/* ---------- 原生拖入 ---------- */
const win = getCurrentWindow();
win.onDragDropEvent((event) => {
  const p = event.payload;
  if (p.type === "enter" || p.type === "over") {
    els.shelf.classList.add("dragover");
  } else if (p.type === "leave") {
    els.shelf.classList.remove("dragover");
  } else if (p.type === "drop") {
    els.shelf.classList.remove("dragover");
    addPaths(p.paths);
  }
});

/* ---------- 设置 ---------- */
function openSheet() {
  els.sheet.classList.add("open");
  syncSettingsUI();
}
function closeSheet() {
  els.sheet.classList.remove("open");
  cancelRecord();
}

els.settingsBtn.addEventListener("click", () => {
  els.sheet.classList.contains("open") ? closeSheet() : openSheet();
});
els.closeSheet.addEventListener("click", closeSheet);

document.getElementById("openDataBtn")?.addEventListener("click", () => {
  invoke("open_data_dir").catch(() => toast("打开失败"));
});

function syncSettingsUI() {
  els.keycap.textContent = prettyShortcut(state.settings.shortcut);
  els.autostart.setAttribute("aria-checked", String(state.settings.autostart));
  els.themeSeg.querySelectorAll<HTMLButtonElement>("button").forEach((b) => {
    b.classList.toggle("on", b.dataset.themeValue === state.settings.theme);
  });
}

function prettyShortcut(accel: string): string {
  if (!accel) return "未设置";
  if (!IS_MAC) {
    return accel
      .replace(/Command|Cmd/g, "Win")
      .replace(/Control|Ctrl/g, "Ctrl")
      .replace(/Option|Alt/g, "Alt")
      .replace(/Shift/g, "Shift")
      .replace(/\+/g, " + ");
  }
  return accel
    .replace(/Command|Cmd/g, "⌘")
    .replace(/Option|Alt/g, "⌥")
    .replace(/Control|Ctrl/g, "⌃")
    .replace(/Shift/g, "⇧")
    .replace(/\+/g, " ");
}

/* 快捷键录制 */
let recording = false;

function startRecord() {
  recording = true;
  els.keycap.classList.add("recording");
  els.keycap.textContent = "按下组合键…";
  // 录制期间先注销全局快捷键，避免按下旧组合时把面板收起
  invoke("suspend_shortcut").catch(() => {});
}

function cancelRecord() {
  if (!recording) return;
  recording = false;
  els.keycap.classList.remove("recording");
  els.keycap.textContent = prettyShortcut(state.settings.shortcut);
  invoke("resume_shortcut").catch(() => {});
}

function keyToAccel(e: KeyboardEvent): string | null {
  const mods: string[] = [];
  if (e.metaKey) mods.push("Command");
  if (e.ctrlKey) mods.push("Control");
  if (e.altKey) mods.push("Option");
  if (e.shiftKey) mods.push("Shift");

  let key = "";
  if (e.code === "Space") key = "Space";
  else if (e.code.startsWith("Key")) key = e.code.slice(3);
  else if (e.code.startsWith("Digit")) key = e.code.slice(5);
  else if (/^F\d{1,2}$/.test(e.code)) key = e.code;
  else return null;

  if (!mods.length) return null;
  return [...mods, key].join("+");
}

els.keycap.addEventListener("click", startRecord);

window.addEventListener("keydown", async (e) => {
  if (recording) {
    e.preventDefault();
    if (e.key === "Escape") {
      cancelRecord();
      return;
    }
    const accel = keyToAccel(e);
    if (!accel) return;
    els.keycap.textContent = prettyShortcut(accel);
    recording = false;
    els.keycap.classList.remove("recording");
    try {
      await invoke("set_shortcut", { shortcut: accel });
      state.settings.shortcut = accel;
      toast("快捷键已更新");
    } catch (err) {
      toast("这个组合键不可用");
      els.keycap.textContent = prettyShortcut(state.settings.shortcut);
    }
    return;
  }

  if (e.key === "Escape") {
    if (els.sheet.classList.contains("open")) closeSheet();
    else await invoke("hide_window");
  }
});

els.autostart.addEventListener("click", async () => {
  const next = els.autostart.getAttribute("aria-checked") !== "true";
  els.autostart.setAttribute("aria-checked", String(next));
  try {
    await invoke("set_autostart", { enabled: next });
    state.settings.autostart = next;
    toast(next ? "已开启开机启动" : "已关闭开机启动");
  } catch {
    els.autostart.setAttribute("aria-checked", String(!next));
    toast("设置失败");
  }
});

els.themeSeg.addEventListener("click", async (e) => {  const btn = (e.target as HTMLElement).closest("button") as HTMLButtonElement | null;
  if (!btn) return;
  const value = btn.dataset.themeValue as Settings["theme"];
  state.settings.theme = value;
  syncSettingsUI();
  applyTheme();
  await invoke("set_theme_pref", { theme: value }).catch(() => {});
});

/* ---------- 主题 ---------- */
function applyTheme() {
  const pref = state.settings.theme;
  const dark = pref === "dark" || (pref === "auto" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  invoke("apply_theme_cmd", { theme: pref }).catch(() => {});
}

window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  if (state.settings.theme === "auto") applyTheme();
});

/* ---------- 启动 ---------- */
(async () => {
  const s = await invoke<AppState>("get_state");
  state = s;
  applyTheme();
  syncSettingsUI();
  render();

  warmDragIcons();
  listen("shelf:changed", () => refresh());
  listen("shelf:refresh", async () => {
    await invoke("recheck").catch(() => false);
    await refresh();
  });

  // 后台校验路径，静默更新失效状态
  const changed = await invoke<boolean>("recheck").catch(() => false);
  if (changed) await refresh();

  const notice = await invoke<string | null>("take_notice").catch(() => null);
  if (notice) setTimeout(() => toast(notice, 3200), 700);
})();
