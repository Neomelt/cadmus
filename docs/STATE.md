# 项目状态（交接备忘）

> 更新：2026-10-12 · App **v0.3.6** 已发布 · 用途：新会话 / 新协作者快速对齐进度与约定。

## 当前状态

- **一期功能全部实现并发布**：导入管理（EPUB/TXT/PDF）→ 阅读（排版全项/五主题/图片/封面/脚注智能配对）→ PDF 重排 + 原文页⇄重排切换 → 批注（多色高亮/笔记/书签/面板/边界拖动/导出 Markdown）→ 全文搜索 → 位置锚点 → 书库排序筛选 → 继续阅读
- **手感对齐 Books/Kindle**：批次一（v0.3.2/v0.3.4）底部进度条、整屏翻页 + 点按分区、目录当前章高亮；批次二（v0.3.5/v0.3.6）标注面板关闭修复、段距/首行缩进、继续阅读卡、高亮边界拖拽
- **版本线**：v0.1.3 → v0.1.4 → v0.1.5 → v0.2.0 → v0.2.1 → v0.2.3 → v0.3.0 → v0.3.1 → v0.3.2 → v0.3.4 → v0.3.5 → **v0.3.6（Latest）**（均 iPad 专用未签名 IPA，SideStore 侧载）

## 待办（按优先级）

1. **批次三**：PDF 缩略图导航 / 跳页输入（原文页视图）；标注列表按章节分组
2. **评审遗留**：PDF 书签树（FR-2.8）；搜索中文分词（FR-2.9，现为子串匹配）；锚点升级为 `(block_id, char_offset)`（ARCHITECTURE §5.2）；标注列表补齐"按书筛选"（FR-3.4）；高亮卡片的样式打磨
3. **CR-1 人工抽样复核**（重排质量 go/no-go，尚未执行）：准备 3 页对照片段（含 1 页含图/公式），请用户拍板
4. **Android 手机端**（设备范围含，未启动）：pdfium android 预编译 + 文件选择器 + 分享面板 + 导出目录（Tauri document_dir 在 Android 为私有目录，需分享渠道）
5. 已知限制：PDF 跨栏/跨页断字不合并；复合词偶发误合并；扫描版 PDF 整页位图降级

## 发布流程（每个切片）

1. TDD（先红后绿）→ `cargo fmt --check` + `clippy -D warnings` → 两个 workspace 全测（`src-tauri` 是独立 workspace，单独跑）
2. README 状态行 + CHANGELOG + 版本号（`src-tauri/tauri.conf.json` + `src-tauri/Cargo.toml`，跑一次测试刷新 lock）→ commit + push master
3. ios workflow 自动出包（macos-15；pdfium `chromium/8086` 注入 IPA `Frameworks/`）→ `gh run watch` → 下载核验（版本 / UIDeviceFamily=[2] / libpdfium 存在）→ `gh release create`（`cadmus-<version>-unsigned.ipa`）
4. 桌面 E2E：`CADMUS_PDFIUM_LIB=/home/neomelt/cadmus/vendor/pdfium/lib` 启动；截图 `wmctrl -lG`（`$8=="cadmus" && NF==8` 取窗口 id）+ `import -window <id>`；E2E 临时补丁用完必须还原并 `grep __E2E__` 确认

## 环境注意

- **网络**：GitHub API / CDN 间歇性 EOF——下载与发布命令都要带重试循环；cargo 新依赖走 rsproxy
- **CI**：ios 路径过滤含 `crates/**`（引擎改动也会出包）；CI（ubuntu）无 pdfium——涉 pdfium 的测试标 `#[ignore]`，本地跑 `cargo test -- --ignored`
- **数据目录**：`~/.local/share/com.neomelt.cadmus/`（`library.json` + `books/` + `assets/<id>/` + `annotations/<id>.json`）；路径全部相对化，容器搬迁自动自愈
- **桌面测试书**：`/tmp/opencode/`（注疏.epub、图册.epub、李群重排示例.pdf）；结束后 `pkill -x cadmus`
- **iOS 细节**：`tauri ios init` 忽略 icons/ios，CI 拷贝 + 设备族 sed 成 "2"；Info.plist 开 `UIFileSharingEnabled` 让导出在「文件」App 可见

## 关键决策（别推翻）

- 批注存储用 JSON（非 SQLite，见 `annotations.rs` 模块注释）；PDF 重排缓存是**文档 JSON**（`reflow-doc-v1.json`），批注变化免重排
- 批注区间与位置锚点用 **Unicode 码点**偏移；块 id 过 JS 边界一律**字符串**（防 u64 精度丢失）
- **页码统一 1 基**（`page_range` / `data-page` / 导出 `p.N`）
- pdfium **全局单次绑定**（`OnceLock`，二次初始化会报 AlreadyInitialized）
- 导出 Markdown 为全量重写（输出确定即幂等），模板见 ARCHITECTURE §8
