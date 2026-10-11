# 技术架构

> 状态：草案 v0.1 · 2026-10-11
> 产品定义见 `PRODUCT.md`，验收口径见 `CRITERIA.md`。
> `[待定]` 条目附默认取值。

---

## 1. 约束（不可协商）

| 约束 | 来源 | 后果 |
|---|---|---|
| 双端发布（iOS 主 + Android） | 产品目标 | UI 方案必须跨端；Rust 只能作为库/Core 出现 |
| 开发机为 Linux | 现实 | **iOS 无法本地出包**，必须走 macOS CI（`macos-14+` runner 出 ipa） |
| 阅读为主场景、离线可用 | 产品原则 | 重排与渲染不能依赖网络 |
| 长期维护（>3 年） | 目标 | 依赖尽量少；PDF 渲染层隔离成可替换 crate |

## 2. 技术选型

| 层 | 选择 | 理由 | 备选/风险 |
|---|---|---|---|
| 应用框架 | **Tauri 2**（Rust core + WebView UI） | 唯一同时满足：Rust 为核心、iOS/Android 官方支持、UI 迭代快 | 依赖系统 WebView（iOS 上为 WKWebView，行为需实测） |
| UI | Web 前端（Svelte/TS 或原生 TS）`[待定：默认 Svelte 5]` | 复杂交互与排版由 Web 引擎承担 | — |
| PDF 渲染 | **隔离 crate** 封装渲染后端：`pdfium-render` 优先 | 跨平台、内置文本抽取；成熟度最高 | pdfium 二进制需随包分发；MuPDF 为 AGPL，仅作备选 |
| 文本抽取 | 同渲染后端提取文本对象（坐标 + 字形） | 重排输入必须是带坐标的文本树 | 纯 Rust `pdf` 生态尚不足以替代 |
| 中文排版 | 系统 WebView 文本引擎（字体、禁则、悬挂、中英间距） | iOS/Android 内核已实现高质量 CJK 排版；自研不值得 | 特殊排版需求后续用 CSS 细化 |
| 存储 | SQLite（`rusqlite`，WAL） | 批注、缓存、索引、锚点 | — |
| 网络 | `reqwest` + `rustls` | OPDS/WebDAV | — |
| 同步 | 局域网直连（HTTP）+ `mdns-sd` 发现 | 不引入账号体系；离线优先 | 后期可选 WebDAV |
| 构建/发布 | GitHub Actions：Linux（Android）、macOS（iOS） | Linux 本地产 Android 包，Mac runner 出 ipa | Actions 费用与时长 |

## 3. 系统结构

```
┌─ iPad / Android ──────────────────────────────────────────────┐
│  App (Tauri 2)                                                │
│  ┌────────────┐  ┌──────────────┐  ┌──────────────────────┐   │
│  │ UI (Web)   │◄►│ 核心 (Rust)   │◄►│ 存储 (SQLite)         │   │
│  │ 阅读视图    │  │ 重排引擎      │  │ 文档/锚点/批注/缓存    │   │
│  │ 目录/标注   │  │ source 层    │  └──────────────────────┘   │
│  │ 设置       │  │ 同步客户端    │                             │
│  └────────────┘  └──────┬───────┘                             │
└─────────────────────────┼─────────────────────────────────────┘
                          │ 局域网 (HTTP, mdns 发现)
┌─────────────────────────┼─────────────────────────────────────┐
│  桌面伴生 reader-syncd (Rust CLI，可选)                        │
│  ┌──────────────┐  ┌─────────────┐  ┌─────────────────────┐   │
│  │ 书库索引      │  │ 同步服务     │  │ 批注回流            │   │
│  │ lunwen/Zotero│  │ 增量传输     │  │ → 知识库 Inbox (.md) │   │
│  └──────────────┘  └─────────────┘  └─────────────────────┘   │
└───────────────────────────────────────────────────────────────┘
```

## 4. 里程碑对应的仓库结构

```
reader/
├─ Cargo.toml              # workspace
├─ crates/
│  ├─ core/                # 文档模型、锚点、存储（无 UI 依赖，可单测）
│  ├─ reflow/              # 重排引擎（M1 的验证主体）
│  ├─ source/              # LocalFile / OPDS / WebDAV（统一 trait）
│  ├─ syncd/               # 桌面伴生：索引 + 同步 + 回流
│  └─ app/                 # Tauri 壳（M2 起）
├─ cli/                    # reader-cli：打开发文档、dump 重排、调试
├─ ui/                     # Web 前端
├─ docs/                   # 本文档集
└─ .github/workflows/      # android.yml / ios.yml / ci.yml
```

依赖方向：`app → source → core ← reflow`；`syncd → core`。`core` 不依赖任何 UI/平台。

## 5. 重排引擎（差异化核心）

### 5.1 管线

```
PDF 文件
  → [parse]   文本对象：字形 + 坐标 + 字号 + 旋转 + 字体
  → [blocks]  几何聚类：行 → 段落；空白切割；缩进/字号启发
              → 分栏检测（x 投影谷 / 聚类）
  → [regions] 区域分类：正文 / 标题 / 图 / 表 / 公式 / 代码 / 边注
  → [order]   阅读顺序重建（栏序 + 标题层级校正）
  → [atomic]  原子块保护：图/表/公式/代码整块保留，不拆行
  → [render]  每块：文本块交付 WebView 排版；非文本块栅格化为位图（按块裁剪）
```

### 5.2 关键设计决策

| 决策 | 内容 |
|---|---|
| 重排发生在打开时 | 首次打开生成块序列并缓存；后续直接读缓存 |
| 文本块以"语义内容"呈现 | 输出的不是坐标，而是带结构的文本（段落/标题），由 WebView 重排 |
| 非文本块以"块位图"呈现 | 图/公式/表格不参与文字流向，按尺寸随文流动，保证语义完整 |
| 锚点 = 语义位置 | `anchor = (block_id, char_offset)`；block_id 稳定绑定原页坐标。字号变化/旋转不影响锚点 |
| 回映射 | 每块携带 `page_range`，锚点可回推原页（用于批注导出、与桌面端互通） |
| 错误可容错 | 单页布局分析失败 → 该页降级为整页位图，其余照常 |

### 5.3 性能预算（必须实测）

| 指标 | 目标 |
|---|---|
| 冷启动 → 首屏可读 | < 500ms |
| 20 页双栏论文首次重排 | < 2s（后台线程，不阻塞首屏） |
| 滚动 | 60fps |
| 内存（单文档） | < 150MB |
| 重排可取消 | 切书/关闭时立即取消 |

## 6. source 层（统一接口）

```rust
trait Source {
    async fn search(&self, q: &str) -> Vec<BookRef>;
    async fn book(&self, r: &BookRef) -> BookMeta;
    async fn files(&self, r: &BookRef) -> Vec<DocumentRef>;   // 章节/文件
    async fn fetch(&self, d: &DocumentRef, cancel: Cancel) -> ByteStream;
}
```

实现：

| 实现 | 第一期 | 说明 |
|---|---|---|
| LocalFile | ✅ | 系统文件 / 隔空投送 / 文件共享 |
| WebDAV | ✅ | 指向自建库；离线缓存策略见下 |
| OPDS | ✅ | 标准目录协议（Gutenberg、Standard Ebooks、Calibre 服务器等） |
| RuleBased | 第二期 | 声明式规则（selector/JSONPath/正则/分页模板）；**不执行任意代码**；不预置任何源 |

缓存策略：source 只负责取回原始文件；阅读与重排面向本地缓存副本，保证离线可读、断点续传。

## 7. 数据模型（SQLite）

```
documents(id, hash, path, kind, title, meta_json, imported_at, last_read_at)
blocks(doc_id, block_id, kind, page_range, payload, order_key)      -- 重排缓存
positions(doc_id, anchor_block, anchor_offset, updated_at)          -- 阅读位置
annotations(id, doc_id, anchor_block, anchor_offset, text, note,
            color, created_at, exported_at)                          -- 批注
sources(id, kind, config_json, credential_ref)                       -- 源配置
```

原则：`annotations` 是本地唯一事实来源；导出（Markdown/回流）是幂等投影，`exported_at` 记录水位。

## 8. 同步协议（M4）

- 发现：mDNS `_reader._tcp`；失败时手动输 IP:port（**不做账号体系**）。
- 传输：HTTP/1.1 + JSON + 文件流；增量以 `doc hash + block_id` 对齐两端。
- 批注回流（核心链路）：
  1. App 端生成批注记录（原文引用、note、anchor、页码）；
  2. `syncd` 接收后渲染为 Markdown，写入 `知识库/00 Inbox/`，文件名 `«书名»-高亮.md` 追加式；
  3. 冲突：以 App 端为主（唯一写者原则在 App 侧），`syncd` 只追加不修改。
- 失败处理：静默重试 + 指数退避；阅读界面零提示。

## 9. 构建与发布

| 目标 | 本机 (Linux) | CI |
|---|---|---|
| Android APK | ✅ 直接出包 | 可选 |
| iOS ipa | ❌ 不可 | GitHub Actions `macos-14+`，`xcodebuild` + Tauri 打包；TestFlight 上传 |

CI 工作流清单：`ci.yml`（fmt/clippy/test ×core/reflow/source）、`android.yml`、`ios.yml`。

## 10. 开放问题

| # | 问题 | 默认 |
|---|---|---|
| 1 | UI 框架（Svelte vs 原生 TS） | Svelte 5 |
| 2 | pdfium 分发方式（预编译 vs 源码构建） | 预编译 + 每平台校验 |
| 3 | 桌面端 UI（是否需要，还是纯 CLI） | 纯 CLI + 本地 web 状态页 |
| 4 | 云同步（跨 iPad/手机） | 不做，局域网足够 |
| 5 | 是否收录 EPUB（第二期） | 是 |
