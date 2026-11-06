# 暂存架 · File Shelf

macOS / Windows 桌面文件暂存架：把文件 / 文件夹临时丢进来，随时拖出去、在访达 / 资源管理器中定位、一键清空。

> 只记录路径引用，**不复制、不移动、不删除**任何磁盘文件。清空列表等同于清空白板。

## 功能

- 拖入文件或文件夹即暂存，支持一次拖入多个
- 从列表按住拖出，可直接拖进访达 / 资源管理器 / 微信 / 邮件 / 浏览器上传框
- 单击行悬停出操作：在访达 / 资源管理器中显示、移除
- 双击用默认程序打开
- 一键清空（可逐个淡出动画）
- 全部暂存项持久化，重启后自动恢复；原文件失效会标灰提示
- 全局快捷键唤起（macOS ⌥⌘Space / Windows Ctrl+Alt+Space），面板固定出现在托盘图标下方（多屏时跟随图标所在屏幕）
- 快捷键可自定义、开机自启可开关、外观支持跟随系统 / 深色 / 浅色
- **完全后台运行**：macOS 标记 `LSUIElement`（不占 Dock、不进 Cmd+Tab、没有应用菜单），只留菜单栏图标；Windows 用 `skipTaskbar`
- 菜单栏图标用**模板图**（纯黑 + alpha），深浅色菜单栏自动反色，和系统图标同一视觉语言
- 左键点击图标显示 / 隐藏，右键出菜单
- 单实例运行：重复打开只会把已有面板叫出来，不会出现第二个托盘图标

## 界面

- 380 × 460 的无边框悬浮面板 + 系统级毛玻璃（macOS NSVisualEffectView / Windows Acrylic）
- macOS：运行时把窗口改造成 **NSPanel + NonactivatingPanel**（同 Spotlight / Raycast），
  这是 macOS 唯一能浮在「别的应用全屏 Space」之上的窗口类型，普通 NSWindow 做不到；
  层级 NSPopUpMenuWindowLevel(101)，collectionBehavior = 所有桌面 + 固定 + 全屏辅助 + 不参与窗口循环
- Windows：`alwaysOnTop` + 工作区约束，避开任务栏；窗口同样无边框半透明
- **固定吸附在托盘图标旁边**（macOS 在图标正下方，Windows 在图标上方），多显示器按图标所在屏幕计算
- 不随应用失活隐藏（`hidesOnDeactivate = false`）
- Esc 收起

## 开发

```bash
npm install
npm run app:dev      # 开发模式（Vite + Tauri）
npm run app:build    # 打包
```

产物（按平台自动选择）：

```
macOS:   src-tauri/target/release/bundle/macos/暂存架.app  +  dmg/暂存架_0.1.0_*.dmg
Windows: src-tauri/target/release/bundle/nsis/*.exe  +  msi/*.msi
```

> Windows 版需要在 **Windows 机器**上执行 `npm run app:build`（Tauri 不支持从 macOS 交叉编译到 Windows）。
> 首次需安装 Rust toolchain + VS Build Tools（C++ 桌面开发）。
>
> 注意：生成 dmg 需要 `hdiutil` 权限，在受限终端里可能失败（`hdiutil: create failed - 操作不被允许`），换普通终端执行即可。

## 数据位置

```
macOS:   ~/Library/Application Support/cn.wuteng.fileshelf/state.json
Windows: %APPDATA%\cn.wuteng.fileshelf\state.json
```

## 说明

- macOS 未签名版本首次打开会被 Gatekeeper 拦截：右键 App → 打开，或
  `xattr -dr com.apple.quarantine "/Applications/暂存架.app"`。
- Windows 未签名版本会被 SmartScreen 拦：点「更多信息」→「仍要运行」。
- 开机自启：macOS 通过 `~/Library/LaunchAgents` 的 LaunchAgent，Windows 通过注册表 Run 项，登录后静默启动（不弹面板）。

## 平台差异

| | macOS | Windows |
|---|---|---|
| 毛玻璃 | NSVisualEffectView | Acrylic（Win10 1803+，不支持时退回 CSS 半透明） |
| 全屏置顶 | NSPanel + NonactivatingPanel | `alwaysOnTop` |
| 面板定位 | 托盘图标正下方 | 托盘图标上方 / 避开任务栏 |
| 默认快捷键 | ⌥⌘Space | Ctrl+Alt+Space |
| 在文件夹中显示 | `open -R` | `explorer /select,` |

## 技术栈

Tauri 2 + Rust + 原生 TypeScript（Vite）。拖出用 `tauri-plugin-drag`，快捷键 `tauri-plugin-global-shortcut`，自启 `tauri-plugin-autostart`，单实例 `tauri-plugin-single-instance`，毛玻璃 `window-vibrancy`。

## 品牌资源

```
.assets/app-icon.svg      应用图标源文件（1024×1024）
.assets/tray.svg          菜单栏模板图标源文件（24×24 线稿）
.assets/app-icon.png      渲染后的 1024 图，用于 `npx tauri icon`
```

改图标流程：编辑 SVG → Chrome 无头渲染成 PNG → `npx tauri icon .assets/app-icon.png -o src-tauri/icons`。
菜单栏图标：渲染成 44×44 透明 PNG，放到 `src-tauri/icons/tray-template.png`（编译时 `include_bytes!` 内嵌）。
