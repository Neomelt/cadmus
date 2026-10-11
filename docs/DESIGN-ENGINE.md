# cadmus 阅读引擎（M1）设计：PDF 重排与 EPUB/TXT 解析

**变更记录**

| 版本 | 日期 | 修改人 | 变更内容 |
|---|---|---|---|
| v1.0 | 2026-10-11 | Neomelt | 初始版本 |

**状态**：评审通过

## 已有设计

cadmus 无外部设计库，以下为仓库内已有文档的检索结果（本地版 `wuji-design-lookup`）：

| 已有文档 | 相关章节 | 关系 |
|---|---|---|
| `PRODUCT.md` v1.1 | §6 FR-2.1–FR-2.11、§9.1 M1 | 沿用：本设计即 M1 范围内的实现方案 |
| `ARCHITECTURE.md` v0.3 | §5 阅读引擎、§7 数据模型、§4 仓库结构 | 沿用并细化到符号级 |
| `CRITERIA.md` v0.3 | CR-1、CR-2、CR-7、CR-8 | 沿用：§8 测试用例逐条对应 |
| `RESEARCH.md` v1.0 | §3 Rust 生态、§6 U1/U2/U4 | 无冲突：本设计不依赖未验证生态 |

与已有设计的关系：沿用 `ARCHITECTURE.md` §5.1 的三段管线（PDF 重排 / EPUB / TXT）与块模型方向，本设计将其细化到数据结构、函数签名与阈值；不改变任何已注册的验收标准。

---

## 1. 背景与目标

### 1.1 现状

产品文档集已定稿（PRD v1.1、ARCHITECTURE v0.3、CRITERIA v0.3），尚未开始实现。M1 的目标是「阅读引擎在桌面成立」：`cadmus-cli` 能把三类内容转成可读输出，其中 CR-1（双栏重排成立）是 go/no-go 门。

### 1.2 关联文档

- 里程碑与范围：`PRODUCT.md` §9.1 M1、§6 FR-2.1–FR-2.11
- 验收标准：`CRITERIA.md` CR-1、CR-2、CR-8（CLI 部分）、CR-7
- 无关联工作项（个人项目，不接飞书项目）

### 1.3 目标

每条只写一件事，可以被衡量：

1. `cadmus-cli reflow` 把 F1/F2 双栏 PDF 输出为单列连续文本，CR-1 抽样协议全部命中且顺序正确。
2. 公式/图/表/代码以完整原子块输出，无跨栏截断、无乱序字符流（CR-2 的 CLI 部分）。
3. F5 EPUB 解析为块序列：章节顺序正确，图片与脚注位置正确。
4. F6（>5MB 中文 TXT）编码探测正确、章节切分可用、首次分页 < 1s。
5. 20 页双栏 PDF 首次重排 < 2s（`bench` 输出 ×5 中位数）。
6. 块模型与锚点 `(block_id, char_offset)` 定义稳定，M2 的 UI 与存储可直接复用。

### 1.4 非目标

- 不做 OCR：扫描件（F3）只走整页位图降级路径。
- 不做 UI 与渲染：WebView 排版、CSS 变量、阅读器界面属于 M2。
- 不做批注与导出：CR-4/CR-9 属于 M2。
- 不做结构性重建：公式/表格不转 LaTeX/HTML，以位图保真为上限。
- 不支持加密 PDF（明确报错），不引入 Source 抽象（本地文件直读，source 层为二期）。
- 不承诺所有版面通用：三栏以上、图文环绕、横排混排超出 M1，走降级。

## 2. 接口与行为约定

### 2.1 CLI（对外接口）

| 命令 | 关键参数 | 行为 |
|---|---|---|
| `cadmus-cli reflow <file.pdf>` | `--viewport WxH`（默认 `600x800`，CR-1 口径）、`--dump text\|json`（默认 `text`）、`--pages a-b`、`--out <dir>` | 输出重排后的块序列 |
| `cadmus-cli epub <file.epub>` | `--dump blocks\|text`、`--out <dir>` | 输出解析后的块序列 |
| `cadmus-cli txt <file.txt>` | `--encoding <enc>`（默认自动探测）、`--dump chapters\|text` | 输出章节与文本 |
| `cadmus-cli bench <file>` | `--repeat n`（默认 5） | 输出解析/重排各阶段耗时中位数 |

约定：

- **stdout 只放结果，stderr 放诊断**（降级页数、耗时、警告）。`--dump text` 的输出必须可以直接用于 CR-1 人工对照。
- 退出码：`0` 成功；`1` 输入错误（文件不存在、无法解析、加密）；`2` 成功但存在降级（输出仍完整）。
- **确定性**：同一输入 + 同一参数必须产生逐字节一致的输出（含块 id）。重排算法不得引入随机性或线程调度差异；页级并行不得影响最终顺序。
- `--viewport` 在 M1 只记录不参与计算（重排输出的是流式文本，目标阅读宽度由 M2 的 WebView 承担）。
- `--dump text` 中原子块以占位行呈现：`[图 p.3]`、`[公式 p.3]`、`[表格 p.3]`、`[代码 p.3]`、`[整页位图 p.3（降级）]`；`--dump json` 给出完整结构（`format_version: 1`）。

### 2.2 块模型与锚点（cadmus-core）

```rust
pub enum BlockKind { Heading { level: u8 }, Paragraph, Figure, Table, Formula, Code, Footnote, PageBitmap }

pub struct Block {
    pub id: BlockId,            // 稳定标识，见下
    pub kind: BlockKind,
    pub order: u32,             // 块序列内的阅读顺序（0 起连续）
    pub page_range: Option<(u32, u32)>, // PDF：原页范围；EPUB/TXT：None
    pub payload: Payload,       // Text(String) | Bitmap(BitmapRef)
}

pub struct Anchor { pub block_id: BlockId, pub char_offset: u32 }

pub struct BitmapRef { pub path: String, pub width_px: u32, pub height_px: u32 }
```

约定：

- **块 id 必须稳定**：由 `stable_block_id(doc_hash, page_or_chapter, seq)` 纯函数派生（默认 `hash64(doc_hash, page, seq)`）。同文件、同参数下多次运行 id 一致，这是锚点与幂等的基础。
- **锚点语义**：`(block_id, char_offset)`；`char_offset` 为该块文本中的 Unicode 标量序号（0 起）[待定：默认 Unicode 标量]。字号、旋转、重启不影响锚点。
- **非文本块**必须携带像素尺寸，随文流动不拆行；文本块以语义内容（段落/标题）呈现，不携带坐标。
- EPUB/TXT 的 `page_range` 为 `None`，回映射用章节索引 + 百分比（M2 实现）。

### 2.3 错误语义

- 加密 PDF：不尝试解密，退出码 `1`，stderr 明确报错。
- 无文本层、布局分析失败：该页降级为 `PageBitmap`，退出码 `2`，stderr 报告降级页数。
- EPUB 单章解析失败：跳过并标记，不阻断其余章节。
- TXT 解码遇到非法字节：lossy 替换后继续，stderr 警告。

### 2.4 取消与并发

- 引擎 API 接受 `Cancel` token：取消后立即返回，不落半成品（CLI 可不暴露，M2 UI 使用）。
- 页级/章级处理可以并行；**阅读顺序重建必须串行且确定**（见 §2.1 确定性约定）。

## 3. 方案与取舍

### 3.1 重排总体路线

| 方案 | 改动范围 | 风险 | 成本 | 结论 |
|---|---|---|---|---|
| A 文本对象 + 几何分析（选定） | cadmus-reflow 全部模块 | 布局分析质量（CR-1） | 中 | 选这个：文字进入流式重排，可调字号/主题；公式以位图保真 |
| B 栅格裁剪缩放（K2pdfopt 型） | 低 | 低 | 低 | 不选：文字不可重排，「字号可调、无水平拖动」目标不可达 |
| C 云端重排 | 高 | 隐私与合规 | 高 | 不选：违背本地优先（D9） |

### 3.2 文本抽取后端

| 方案 | 结论 |
|---|---|
| A pdfium（`pdfium-render`）（选定） | 选这个：跨平台、字符级坐标 + 字体信息、成熟度最高 |
| B 纯 Rust（`lopdf`/`pdf-extract`） | 不选：坐标与字体信息不足以支撑布局分析 |
| C MuPDF | 不选：AGPL，与 MIT OR Apache-2.0 冲突（R3） |

### 3.3 EPUB 处理

| 方案 | 结论 |
|---|---|
| A 现成 crate 解析容器/OPF + 自研 XHTML→块序列（选定） | 选这个：容器解析无差异化价值，块序列是锚点/搜索的公共基础 |
| B 全自研（zip + XML） | 不选：无必要成本 |
| C 直接交给 WebView 处理 | 不选：M1 无 WebView；且块模型是 §7 数据模型的前提 |

### 3.4 TXT 章节切分

| 方案 | 结论 |
|---|---|
| A 内置默认规则集（常见卷/章模式）+ 兜底分段（选定） | 选这个：F6 类无结构文本也能得到可用分段 |
| B 仅内置规则，无兜底 | 不选：不匹配规则的文件会退化为整本单章 |
| C 交给用户手动分章 | 不选：违背零配置启动 |

### 3.5 关键技术决策记录

1. **为什么自研布局分析**：pdfium 只提供字符坐标，版面理解（分栏、段落、区域、顺序）必须自己做；Rust 生态无成熟替代（RESEARCH §3）。更换条件：出现成熟的、许可兼容的 Rust 重排库。
2. **为什么原子块用位图**：公式/表格的结构重建成本高且错误率高，保真优先。更换条件：公式需要可编辑/可检索时（二期评估），届时以块为单位增量替换。
3. **为什么锚点是 `(block_id, char_offset)`**：页码依赖版式，字号/旋转/重启会漂移（CR-3）。更换条件：块模型整体替换时，批注坐标需要迁移重算。
4. **为什么 pdfium 隔离为可替换 crate**：二进制分发与许可风险（R3/U4）。更换条件：pdfium 在 iOS/Android 的分发不可行时，切备选后端，接口不变。

## 4. 详细设计

### 4.1 cadmus-core — 文档模型与锚点（`crates/cadmus-core/`）

- 文件：`src/model.rs`（`Block`/`BlockKind`/`Payload`/`BitmapRef`）、`src/anchor.rs`（`Anchor`、`stable_block_id`）、`src/error.rs`（错误类型）。
- `stable_block_id(doc_hash, page_or_chapter, seq) -> u64` 必须是纯函数；`doc_hash` 取源文件内容哈希。
- M1 只做内存模型与 dump；SQLite 持久化在 M2（schema 见 `ARCHITECTURE.md` §7，本设计不改动）。
- 单元测试覆盖 id 稳定性（同输入多次运行、并行下一致）。

### 4.2 cadmus-reflow — PDF 重排管线（`crates/cadmus-reflow/`）

```
crates/cadmus-reflow/src/
├─ lib.rs        # pub fn reflow(input, opts, cancel) -> Result<Document>
├─ parse.rs      # pdfium 适配：页 → RawPage { glyphs, paths, images, size }
├─ lines.rs      # 字形 → 文本行（基线聚类）
├─ columns.rs    # 分栏检测（x 投影谷 + 置信度）
├─ paragraphs.rs # 行 → 段落（行距/缩进/标点启发）
├─ regions.rs    # 区域分类：正文/标题/图/表/公式/代码
├─ order.rs      # 阅读顺序重建（全宽元素切带 + 栏序）
├─ atomic.rs     # 原子块保护（不拆行、不重组）
├─ raster.rs     # 区域栅格化（2–3× 目标尺寸 [待定]）→ PNG
└─ fallback.rs   # 页级降级：整页位图
```

关键步骤：

1. **parse**：逐字符提取 `GlyphBox { ch, x, y, w, h, size, font, rotation }`；跳过不可见字形（render mode 3）；同时保留矢量路径与图像区域（供表格/图形判定）。
2. **lines**：按 y 容差分组、组内按 x 排序，得到 `TextLine`（含 bbox、基线、中位字号）。
3. **columns**：把行 bbox 投影到 x 轴求谷；`cut` 的宽度阈值 ≥ 2× 中位字高且两侧文本量显著 [待定]。结论为 Single/Double/Multi + 置信度；置信度低时走 fallback。
4. **paragraphs**：同栏内相邻行合并为段落，新段条件（满足其一）：行距 > 1.2× 中位行距、首行缩进、上一行以句末标点结尾且填充率 < 80% [待定三者默认值]。
5. **regions**：分类规则（M1 最小集）：
   - 标题：字号 ≥ 正文 ×1.2 或加粗，且为短行；
   - 公式：居中 + 数学字体（`CMMI`/`CMSY`/`CMEX`/`STIX`/`Symbol` 等，名单可扩展 [待定]）；
   - 代码：等宽字体或带背景矩形；
   - 图/表：图像区域、矢量框线网格、大面积非文本区。
6. **order**：全宽元素把页面切成带（band），带内按栏序（左→右），整页按带上→下；标题层级做一次校正。输出为块的确定顺序。
7. **atomic + raster**：所有非文本区域整体栅格化为位图块，按原尺寸比例随文流动，绝不拆行。
8. **fallback**：无文本层或分栏置信度低 → 整页位图（`PageBitmap`），stderr 警告。
9. **并行与取消**：页级并行（`rayon` [待定：默认]），顺序重建串行；入口检查 `Cancel`。

### 4.3 cadmus-epub — EPUB 解析（`crates/cadmus-epub/`）

- 文件：`src/container.rs`（zip → `container.xml` → OPF 路径）、`src/opf.rs`（metadata/manifest/spine）、`src/content.rs`（XHTML → 块序列）。
- XHTML 解析用现成 XML 解析器（`quick-xml` [待定：默认]），转换规则：`h1–h6` → `Heading`，`p` → `Paragraph`，`img` → `Figure`（位图块），`table` → `Table`（位图或文本块 [待定：默认文本]），`pre/code` → `Code`。
- 脚注：`epub:type="noteref"` 的行内标记保留为文本标记，脚注正文作为 `Footnote` 块紧随其后。
- 阅读顺序 = spine 顺序；单章失败跳过并 stderr 警告。

### 4.4 cadmus-epub — TXT 解析（`crates/cadmus-epub/`）

- 文件：`src/encoding.rs`、`src/chapters.rs`。
- 编码探测：`chardetng` 探测 + `--encoding` 手动覆盖 [待定：默认]。BOM 优先。解码失败 lossy 替换。
- 章节切分：内置默认规则集（`第X章/节/卷/回/篇`、`番外`、大小写中文数字 [待定：具体正则]）+ 兜底：按空行聚合到长度上限 [待定：默认 10KB]。
- 性能：单遍扫描，`memchr` 找行边界，正则只作用于短行候选（CR-8 要求 5MB < 1s）。

### 4.5 cadmus-cli（`cli/`）

- 参数解析用 `clap`；子命令与参数见 §2.1。
- 文件类型分发（按扩展名）在 CLI 层完成，不下沉到 cadmus-core（依赖方向：`cli → {reflow, epub} → core`）。
- `bench` 输出：解析、布局、顺序、栅格化各阶段耗时中位数（对应 CR-7 口径 ×5 中位数）。

## 5. 接口变更清单

| 类型 | 标识 | 名称 / 路径 | 定义 | 动作 |
|---|---|---|---|---|
| CLI | `cadmus-cli reflow\|epub\|txt\|bench` | `cli/` | §2.1、§4.5 | 新增 |
| crate API | `Block` / `BlockKind` / `Anchor` / `stable_block_id` | `crates/cadmus-core/src/model.rs`、`anchor.rs` | §2.2 | 新增 |
| crate API | `reflow(input, opts, cancel) -> Result<Document>` | `crates/cadmus-reflow/src/lib.rs` | §4.2 | 新增 |
| crate API | `parse_epub(input, opts) -> Result<Document>`、`parse_txt(input, opts) -> Result<Document>` | `crates/cadmus-epub/src/lib.rs` | §4.3、§4.4 | 新增 |
| 数据格式 | dump JSON `format_version: 1` | §2.1 | 块序列的序列化格式 | 新增 |
| 错误码 | CLI 退出码 `0 / 1 / 2` | §2.1 | 成功 / 输入错误 / 降级成功 | 新增 |
| SQLite schema | — | — | 不适用（M2 引入，见 ARCHITECTURE §7） | — |

## 6. 兼容性与影响面

### 6.1 使用方影响

| 使用方 | 影响 |
|---|---|
| 对外 API（CLI） | 新增命令与参数；无既有使用方 |
| 协议或数据格式 | 新增 dump JSON，首版 `format_version: 1` |
| 工具链和脚本（CI） | 新增 fmt/clippy/test 检查任务（`ci.yml`） |
| 版本互操作 | 不适用（首个实现版本） |

**用户能感知到的行为变化**：无（M1 为引擎里程碑，不发布用户版本；内部 CHANGELOG 正常记录）。

### 6.2 变更范围（Blast Radius）

| 变更对象 | 变更类型 | 影响范围 | 风险等级 |
|---|---|---|---|
| `crates/cadmus-core` | 新增 | 全部后续 crate 的公共依赖 | 中（接口将长期使用） |
| `crates/cadmus-reflow` | 新增 | CR-1/CR-2 所在 | 高 |
| `crates/cadmus-epub` | 新增 | CR-8 所在 | 中 |
| `cli/` | 新增 | 验收检验入口 | 低 |
| `.github/workflows/ci.yml` | 新增 | 无 | 低 |
| `tests/fixtures/`（脚本与来源，版权件不入库） | 新增 | 无 | 低 |

## 7. 风险评估与应对

| # | 风险 | 概率 | 影响 | 应对措施 |
|---|---|---|---|---|
| 1 | 重排质量达不到 CR-1（go/no-go） | 中 | 高 | 按 CR-1 失败处理：停止后续开发，回到布局分析；M1 只验证引擎，不叠加 UI |
| 2 | 启发式阈值过拟合 F1/F2 | 中 | 中 | 阈值全部外置为配置并有默认值 [待定]；抽样协议人工核对；后续持续追加样本 |
| 3 | pdfium 二进制分发与许可（R3/U4） | 低 | 中 | 锁定版本 + 每平台校验；隔离为可替换 crate；MuPDF 仅作许可评估后的备选 |
| 4 | 中文 PDF 字形提取异常（子集字体缺 ToUnicode） | 中 | 中 | 检测到乱码特征 → 该页整页位图降级，不输出错乱文本 |
| 5 | 复杂版面（三栏以上、图文环绕、横排） | 中 | 中 | 降级位图；M1 不承诺，列入后续样本 |
| 6 | 公式/代码误分类为正文（CR-2 失败） | 中 | 高 | 数学/等宽字体与独立区域默认原子块；CR-2 人工抽查 3 公式 2 图 |
| 7 | 性能不达标（CR-7） | 低 | 中 | 页级并行 + 取消 + bench 持续跟踪；栅格化按需（仅原子块） |

## 8. 测试用例

| 测试层级 | 用例 | 预期 |
|---|---|---|
| 单元测试 | 合成双栏页面（含通栏标题）分栏检测 | `cut` 正确、置信度高 |
| 单元测试 | 段落聚类（缩进/行距/标点变化） | 段落边界正确 |
| 单元测试 | 阅读顺序（通栏标题 + 双栏正文） | 顺序正确、确定 |
| 单元测试 | 章节正则（章/节/卷/回/番外/中文数字） | 边界全部命中 |
| 单元测试 | 编码探测（GB18030/UTF-8/Big5 + 手动覆盖） | 正确解码 |
| 集成测试 | CR-1：`reflow F1 --viewport 600x800 --dump text` | 抽样 3 页 ×3 段全命中且顺序正确 |
| 集成测试 | CR-2：F1/F2 原子块计数匹配 + 人工查 3 公式 2 图 | 无截断、无乱序；降级页 ≤1 且不连续 |
| 集成测试 | F3 扫描件 | 整页位图降级，退出码 2，不阻断 |
| 集成测试 | F4 中文页（标点/中英混排） | 不乱序、标点正确 |
| 集成测试 | CR-8：`epub F5` / `txt F6` | 章节数/顺序/图片位置正确；F6 分页 < 1s（界面项 M2 检验） |
| 性能 | CR-7：F1 20 页 reflow `bench` ×5 中位数 | < 2s |
| 性能 | F6（5MB）首次分页、单文档内存 | < 1s；< 150MB |
| 真机 / 端到端 | — | M2 起（同 CRITERIA 检验纪律） |

## 9. review

- [x] Neomelt（产品负责人）
