# 技术架构

> 状态：草案 v0.3 · 2026-10-11（v0.3 修订：设备范围对齐为 iOS 仅 iPad、Android 平板与手机；批注导出 Markdown 模板；性能基线设备）
> 产品定义见 `PRODUCT.md`，验收口径见 `CRITERIA.md`。
> `[待定]` 条目附默认取值。

---

## 1. 约束（不可协商）

| 约束 | 来源 | 后果 |
|---|---|---|
| 双端发布（iOS 仅 iPad，主；Android 平板/手机） | 产品目标 | UI 方案必须跨端；Rust 只能作为库/Core 出现 |
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
│  ┌────────────┐  ┌──────────────────┐  ┌──────────────────┐   │
│  │ UI (Web)   │◄►│ 核心 (Rust)       │◄►│ 存储 (SQLite)     │   │
│  │ 阅读视图    │  │ 文本引擎 (EPUB/  │  │ 文档/锚点/批注    │   │
│  │ 目录/标注   │  │          TXT)    │  │ /缓存             │   │
│  │ 设置       │  │ 重排引擎 (PDF)    │  └──────────────────┘   │
│  └────────────┘  │ 导入/分享接收     │                         │
│                  │ source 层 (二期)  │                         │
│                  └────────┬─────────┘                         │
└───────────────────────────┼───────────────────────────────────┘
                            │ 局域网 (HTTP, mdns 发现) —— 二期
┌───────────────────────────┼───────────────────────────────────┐
│  桌面伴生 cadmus-syncd (Rust CLI，第二期)                      │
│  ┌──────────────┐  ┌─────────────┐  ┌─────────────────────┐   │
│  │ 书库索引      │  │ 同步服务     │  │ 批注出口            │   │
│  │ 本地目录      │  │ 增量传输     │  │ → Markdown          │   │
│  └──────────────┘  └─────────────┘  └─────────────────────┘   │
└───────────────────────────────────────────────────────────────┘
```

第一期不依赖右侧任何组件：**App 单机可完成全部第一期功能**。

## 4. 里程碑对应的仓库结构

```
cadmus/
├─ Cargo.toml              # workspace
├─ crates/
│  ├─ cadmus-core/         # 文档模型、锚点、存储（无 UI 依赖，可单测）
│  ├─ cadmus-reflow/       # PDF 重排：布局分析 + 阅读顺序重建（M1 验证主体）
│  ├─ cadmus-epub/         # EPUB/TXT：解析与分章（M1 验证主体）
│  ├─ cadmus-source/       # LocalFile（一期）/ OPDS / WebDAV（二期）
│  ├─ cadmus-syncd/        # 桌面伴生：索引 + 同步 + 批注出口（二期）
│  └─ cadmus-app/          # Tauri 壳（M2 起）
├─ cli/                    # cadmus-cli：打开文档、dump 重排/解析、调试
├─ ui/                     # Web 前端
├─ docs/                   # 本文档集
└─ .github/workflows/      # android.yml / ios.yml / ci.yml
```

依赖方向：`cadmus-app → cadmus-source → cadmus-core ← {cadmus-reflow, cadmus-epub}`；`cadmus-syncd → cadmus-core`。`cadmus-core` 不依赖任何 UI/平台。

## 5. 阅读引擎

### 5.1 文档处理管线（三类内容）

```
PDF（论文/教材）
  → [parse]   文本对象：字形 + 坐标 + 字号 + 旋转 + 字体
  → [blocks]  几何聚类：行 → 段落；空白切割；缩进/字号启发
              → 分栏检测（x 投影谷 / 聚类）
  → [regions] 区域分类：正文 / 标题 / 图 / 表 / 公式 / 代码 / 边注
  → [order]   阅读顺序重建（栏序 + 标题层级校正）
  → [atomic]  原子块保护：图/表/公式/代码整块保留，不拆行

EPUB（小说/教材）
  → [parse]   container/spine 解析 → XHTML 章节（不重排：原生可重排）

TXT（小说）
  → [parse]   编码探测（GB18030/UTF-8/Big5 等）→ 章节切分（卷/章正则）

  → [render]  三类统一收敛为块序列：
              · 文本块 → 交付 WebView 风格化渲染（CSS 卷动/分页）
              · 非文本块 → 栅格化为位图，按块尺寸随文流动
```

### 5.2 关键设计决策

| 决策 | 内容 |
|---|---|
| 打开时构建块序列 | 首次打开生成并缓存；后续直接读缓存 |
| 文本块以"语义内容"呈现 | 输出的不是坐标，而是带结构的文本（段落/标题），由 WebView 排版 |
| 非文本块以"块位图"呈现 | 图/公式/表格不参与文字流向，按尺寸随文流动，保证语义完整 |
| EPUB/TXT 全量交付 WebView | 不做二次重排；排版参数由 CSS 变量驱动（字号/行宽/行距/主题） |
| 锚点 = 语义位置 | `anchor = (block_id, char_offset)`；block_id 稳定绑定原页坐标（PDF）或章节位置（EPUB/TXT）。字号/旋转/重启不影响锚点 |
| 回映射 | PDF 每块携带 `page_range`，锚点可回推原页；EPUB/TXT 回映射到章节与百分比进度 |
| 错误可容错 | 单页布局分析失败 → 该页降级为整页位图；单章解析失败 → 跳过并标记 |

### 5.3 性能预算（必须实测）

| 指标 | 目标 |
|---|---|
| 冷启动 → 首屏可读 | < 500ms |
| 20 页双栏论文首次重排 | < 2s（后台线程，不阻塞首屏） |
| 滚动 | 60fps |
| 内存（单文档） | < 150MB |
| 重排可取消 | 切书/关闭时立即取消 |
| 5MB TXT 首次分页 | < 1s |
| EPUB 打开（单章先出） | < 500ms |

> 基线设备：iPad 第 9 代（2021）；Android 中端机（2021 年档，4GB RAM）。

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

| 实现 | 一期 | 说明 |
|---|---|---|
| LocalFile | ✅ | 系统文件 / 隔空投送 / 分享导入 |
| OPDS | 第二期 | 标准目录协议（Gutenberg、Standard Ebooks、Calibre 服务器等） |
| WebDAV | 第二期 | 指向自建库；离线缓存策略见下 |
| RuleBased | 第二期之后 | 声明式规则（selector/JSONPath/正则/分页模板）；**不执行任意代码**；不预置任何源 |

缓存策略：source 只负责取回原始文件；阅读与重排面向本地缓存副本，保证离线可读、断点续传。

## 7. 数据模型（SQLite）

```
documents(id, hash, path, kind, title, meta_json, imported_at, last_read_at)
blocks(doc_id, block_id, kind, page_range, payload, order_key)      -- PDF: 重排缓存; EPUB/TXT: 解析产物
positions(doc_id, anchor_block, anchor_offset, updated_at)          -- 阅读位置
annotations(id, doc_id, anchor_block, anchor_offset, text, note,
            color, created_at, exported_at)                          -- 批注
sources(id, kind, config_json, credential_ref)                       -- 源配置（第二期）
```

原则：`annotations` 是本地唯一事实来源；导出是幂等投影，`exported_at` 记录水位；Markdown 是唯一交换格式（不承诺任何笔记软件的专有格式）。

## 8. 批注出口（第一期：导出；第二期：局域网同步）

第一期（App 自足）：

- 导出方式：系统分享面板 + 文件导出（Markdown），不依赖任何网络组件；
- 内容：原文引用、书名、进度/页码定位、笔记；
- 幂等：重复导出靠 `exported_at` 水位去重。

导出文件模板（文件名 `«书名»-高亮.md`；无笔记的条目省略笔记行；位置：PDF 用原书页码，EPUB/TXT 用章节 + 百分比）：

```markdown
# 《书名》标注

> 高亮原文第一处
- 位置：第 3 章 · 42% · 原书 p.57
- 笔记：我的理解……

> 高亮原文第二处
- 位置：第 4 章 · 55% · 原书 p.80
```

第二期（局域网同步时）：

- 发现：mDNS `_cadmus._tcp`；失败时手动输 IP:port（**不做账号体系**）。
- 传输：HTTP/1.1 + JSON + 文件流；增量以 `doc hash + block_id` 对齐两端。
- 链路：App 端生成批注记录 → `syncd` 渲染 Markdown 写入导出目录（如 Obsidian 仓库），文件名 `«书名»-高亮.md` 追加式；冲突以 App 端为主，`syncd` 只追加不修改。
- 失败处理：静默重试 + 指数退避；阅读界面零提示。

## 9. 构建与发布

| 目标 | 本机 (Linux) | CI |
|---|---|---|
| Android APK | ✅ 直接出包 | 可选 |
| iOS ipa | ❌ 不可 | GitHub Actions `macos-14+`，`xcodebuild` + Tauri 打包；TestFlight 上传 |

> iOS 包 device family 设为 iPad 专用（不含 iPhone）；Android 包同一代码库覆盖平板与手机。

CI 工作流清单：`ci.yml`（fmt/clippy/test ×core/reflow/epub/source）、`android.yml`、`ios.yml`。

## 10. 开放问题

| # | 问题 | 默认 |
|---|---|---|
| 1 | UI 框架（Svelte vs 原生 TS） | Svelte 5 |
| 2 | pdfium 分发方式（预编译 vs 源码构建） | 预编译 + 每平台校验 |
| 3 | 桌面端形态（纯 CLI 还是带状态页） | 纯 CLI + 本地 web 状态页 |
| 4 | 云同步（跨 iPad/手机） | 不做，局域网足够 |
| 5 | TXT 编码探测（GB18030/UTF-8/Big5） | charset 探测 + 手动覆盖入口 |
| 6 | 分页模式（左右翻页）vs 滚动 | 小说默认滚动；教材/论文以滚动为主，分页作为可选 |
