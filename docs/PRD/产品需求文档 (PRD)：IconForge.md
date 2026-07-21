# 产品需求文档 (PRD)：IconForge

## 1. 项目概述
### 1.1 软件名称与代号
- **软件中文名称**：图标工坊 (或 图标提取大师)
- **软件英文名称**：IconForge
- **推荐项目代号**：`icon-forge-app`
- **产品定位**：一款轻量、极速、免安装的 Windows 桌面工具，专为开发者和设计师打造，提供 PE 文件图标提取与高清图片转 ICO 功能。

### 1.2 核心技术栈
- **前端框架**：Vue 3 (Composition API + `<script setup>`)
- **UI 组件库**：Element Plus
- **构建工具**：Vite
- **桌面框架**：Tauri 2.x
- **后端语言**：Rust
- **UI 风格**：全局暗黑模式 (Dark Mode)，基于 Element Plus 官方暗黑主题

---

## 2. 前端 UI/UX 规范 (Vue + Element Plus)

### 2.1 全局暗黑模式配置
本项目强制使用 Element Plus 官方暗黑主题，禁止使用亮色模式。
- **样式引入**：在 `main.ts` 中必须引入官方暗黑样式：`import 'element-plus/theme-chalk/dark/css-vars.css'`
- **HTML 属性**：在 `index.html` 的 `<html>` 标签上硬编码 `class="dark"`，确保首屏加载无闪烁。
- **全局背景色**：自定义 CSS 变量覆盖默认背景色，使其更具极客感。
  ```css
  html.dark {
    --el-bg-color: #141414;
    --el-bg-color-overlay: #1d1e1f;
    background-color: var(--el-bg-color);
    color: var(--el-text-color-primary);
  }
  ```

### 2.2 界面布局要求
采用经典的**左侧导航 + 右侧工作区**布局：
- **左侧边栏 (el-menu)**：
  - 包含两个核心菜单项：「图标提取 (Icon Extract)」与「图片转 ICO (Image2ICO)」。
  - 使用 Element Plus 的 `el-icon` 搭配文字。
  - 暗黑模式下菜单背景色应与全局背景融合，高亮状态使用品牌色（如 `#409eff`）。
- **右侧工作区 (el-container)**：
  - 顶部为操作栏（文件拖拽区 / 选择按钮）。
  - 中部为处理进度条（`el-progress`）或预览区（`el-image`）。
  - 底部为结果展示列表（`el-table`）及导出按钮。

---

## 3. 核心功能需求

### 3.1 模块一：Icon Extract (图标提取)
- **交互**：支持拖拽 `.exe`、`.dll`、`.ocx` 等 PE 文件到指定区域，或通过按钮选择文件。
- **Rust 后端逻辑**：
  - 解析 PE 文件的 Resource Directory，提取 RT_GROUP_ICON 和 RT_ICON 资源。
  - 提取出所有包含的图标尺寸（如 16x16, 32x32, 256x256 等）。
- **前端展示**：
  - 以网格形式（`el-card` 或 `el-image`）展示提取出的所有图标。
  - 支持单选/多选。
  - 提供「导出选中」和「全部导出」按钮，调用 Tauri 的 `fs` 或 `dialog` API 保存为 `.ico` 或 `.png` 文件。

### 3.2 模块二：Image2ICO (图片转 ICO)
- **交互**：支持拖拽多张高清图片（PNG/JPG）到工作区。
- **Rust 后端逻辑**：
  - 接收图片二进制数据，使用 `image` crate 进行解码。
  - 自动将图片缩放/重采样为 ICO 标准尺寸（16, 32, 48, 64, 128, 256）。
  - 使用 `ico` crate 将多尺寸图片打包为标准的 `.ico` 文件。
- **前端展示**：
  - 显示待转换的图片列表。
  - 提供尺寸勾选框（`el-checkbox-group`），允许用户自定义需要包含的 ICO 尺寸。
  - 点击「生成 ICO」，前端显示加载状态，完成后弹出保存对话框。

---

## 4. Tauri IPC 接口设计 (Rust <-> Vue)

前端通过 `@tauri-apps/api/core` 的 `invoke` 方法与 Rust 后端通信。请 AI 在 Rust 端实现以下 Command：

```rust
// 1. 提取 PE 文件中的图标信息
#[tauri::command]
async fn extract_icons(file_path: String) -> Result<Vec<IconInfo>, String> {
    // IconInfo 结构体应包含: id, width, height, bit_depth, 以及 base64 预览数据或本地临时路径
    todo!()
}

// 2. 将图片转换为 ICO 并保存到指定路径
#[tauri::command]
async fn convert_image_to_ico(
    image_paths: Vec<String>, 
    target_sizes: Vec<u32>, 
    save_path: String
) -> Result<(), String> {
    todo!()
}
```

---

## 5. AI 执行指令 (给 Agent 的提示)

**致 AI 编程助手：**
请严格按照以上 PRD 执行开发任务。
1. **第一步**：使用 `npm create vue@latest icon-forge-app` 初始化项目，并安装 `element-plus`, `@tauri-apps/cli`, `@tauri-apps/api`。
2. **第二步**：配置 `vite.config.ts` 和 Tauri 的 `tauri.conf.json`，确保开发环境能正常启动。
3. **第三步**：实现全局暗黑模式的 CSS 配置和基础 Layout 布局。
4. **第四步**：编写 Vue 前端页面，完成拖拽上传和 UI 交互。
5. **第五步**：在 `src-tauri/src/lib.rs` (或 `main.rs`) 中实现上述定义的 Rust IPC 接口，引入 `image` 和 `ico` 依赖完成核心逻辑。
6. **注意**：所有 UI 文本使用中文，代码注释使用中文，保持代码整洁，遵循 Rust 和 Vue 3 最佳实践。