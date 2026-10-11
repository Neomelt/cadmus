# 研究附录（RESEARCH）

> 本文是 `PRODUCT.md` 的附录 A，记录产品决策所依据的调研证据与日期。
> 状态：v1.0 · 2026-10-11 · 所有结论标注证据来源；未能验证的假设单列（§6）。
> 下一轮复核建议：2027-Q1（生态与竞品状态每半年可能显著变化）。

---

## 1. 调研范围

| 问题 | 结论位置 |
|---|---|
| 现有阅读器格局与缺口 | §2 |
| 开源与 Rust 生态机会验证 | §3 |
| 移动端技术路线可行性（Rust 双端） | §4 |
| 商店与合规 | §5 |
| 未能验证的假设 | §6 |

## 2. 竞品格局（截至 2026-10）

### 2.1 分平台现状

| 平台/类别 | 代表 | 关键事实（来源） |
|---|---|---|
| iPad 生态阅读 | Apple Books、Kindle | 商店/云优先，自有文件与批注导出受限（厂商文档与产品界面核对） |
| 云阅读服务 | Readwise Reader | 官方定位 "read-it-later for power readers"，PDF/EPUB 上传、统一下划线、TTS、spaced repetition 复习；云优先、订阅制（readwise.io/read，2026-10 抓取） |
| 开源阅读 | KOReader | 30.2k★，支持 PDF/EPUB/DjVu 等，**内置 K2pdfopt 重排**、字典、OPDS；官方支持设备列表：Cervantes/Kindle/Kobo/PocketBook/reMarkable/Android/Linux，**不含 iPad/iOS**（GitHub README，2026-10 抓取） |
| 开源阅读（参考对象） | Kazumi | 35.1k★，Flutter，GPL-3.0，Android/iOS/桌面全平台；官方描述是"基于自定义规则的番剧采集 APP"；**"源/规则"模式在 iOS 有上架先例**（GitHub，2026-10 抓取） |
| 专业标注 | MarginNote、LiquidText | 纸张/图像模型＋私有标注体系，无重排（产品公开文档） |
| 中文生态 | 微信读书、掌阅 | 云书城逻辑，本地库与第三方笔记格式不互通（产品界面核验） |

### 2.2 缺口结论

1. **iPad 上没有开源的强本地阅读器**（KOReader 缺席 iOS 是最明确的证据）；
2. **"移动端 PDF 重排"在商业产品中普遍缺失**，KOReader 的重排（K2pdfopt 型）是唯一成熟实现，且不在平板生态；
3. **批注出口**是商业产品的付费点（Readwise 订阅），本地化、免费、开放格式（Markdown）的组合空缺。

## 3. 开源与 Rust 生态验证（GitHub 仓库搜索，2026-10-11）

| 搜索式 | 结果 |
|---|---|
| `pdf reflow language:rust` | 最高 0★（`angristan/paprika`：WASM PDF 重排，停更；`timothyandrew/reflow`：去连字符工具） |
| `epub reader language:rust` | 1712★ `baskerville/plato`（墨水屏设备）、1216★ `bugzmanov/bookokrat`（终端）、360★ `hygg`（TUI）、340★ `bk`（终端）；**均为终端/墨水屏，无移动端** |
| `reader ios android language:rust` | 仅 1 个 0★ 仓库（novel reader FFI 实验） |

**结论**：Rust + 移动端阅读器是空白生态位；现有 Rust 阅读项目集中在终端/墨水屏，说明该语言在此类产品中被验证可行，但移动端没有先行者。

## 4. 移动端技术路线（2026-10 文档核对）

### 4.1 Tauri 2

- 官方支持移动端：`tauri [android|ios] dev`，iOS 调试需 `TAURI_DEV_HOST`、真机需 Xcode 连接（v2.tauri.app/develop，2026-10 抓取）；
- 移动端前置依赖：`rustup target add aarch64-apple-ios` 等 + **macOS/Xcode**（v2.tauri.app/start/prerequisites）；
- **推论（重要约束）**：Linux 工作站可完整开发 Android，**iOS 打包必须经 macOS（本地 Mac 或 CI runner）**。

### 4.2 UniFFI（备选路线）

- Mozilla 官方工具，为 Rust 库生成 Kotlin/Swift 绑定，目标是"业务逻辑集中于单一 Rust 库，服务多平台"（mozilla.github.io/uniffi-rs，2026-10 抓取）；
- 作为"共享核心 + 双端原生 UI"路线的后备方案保留；代价是维护两套 UI。

### 4.3 结论

主路线：**Tauri 2（Rust 核心 + WebView UI）**；iOS 出包走 macOS CI。决策记录 D7。

## 5. 商店与合规（2026-10 抓取）

- **Apple App Store 审核指南 3.1.3(a)「Reader Apps」**："Apps may allow a user to access previously purchased content or content subscriptions (specifically: magazines, newspapers, books, audio, music, and video)… 该条款允许阅读器应用访问用户在别处获得的内容，并提供免费层账号功能"；
- 对本产品的含义：**本地文件导入 + 标准协议（OPDS/WebDAV）的一期形态没有内购义务**，属低风险上架品类；
- **风险项**是"第三方规则源"（网页采集）。与 Kazumi 同类的模式在 iOS 有上架先例（35.1k★、GPL 应用已上架），但为规避审核不确定性，规则源不进商店包（决策 D6、§9.3）。

## 6. 未能验证的假设（带风险标签）

| # | 假设 | 状态 | 验证计划 |
|---|---|---|---|
| U1 | 布局分析可达到"连续可读"质量（CR-1） | 未验证 | M1 桌面验证，go/no-go |
| U2 | 系统 WebView 的中文排版（禁则/悬挂/中英间距）足够好 | 未验证 | M1/M2 实测，必要时用 CSS 补 |
| U3 | 用户（作者本人）在平板上持续 30 分钟以上阅读的耐受 | 未验证 | 自用狗粮（M2 起） |
| U4 | pdfium 预编译二进制在 iOS/Android 的分发与包体影响 | 未验证 | M2 打包实测 |
| U5 | OPDS 生态对目标用户的实际价值 | 未验证 | 二期再评估 |

## 7. 参考链接

- KOReader：https://github.com/koreader/koreader （30.2k★，AGPL-3.0）
- Kazumi：https://github.com/Predidit/Kazumi （35.1k★，GPL-3.0）
- Readwise Reader：https://readwise.io/read
- Tauri 2 文档：https://v2.tauri.app/start/prerequisites/ 、https://v2.tauri.app/develop/
- UniFFI：https://mozilla.github.io/uniffi-rs/
- Apple 审核指南：https://developer.apple.com/app-store/review/guidelines/ （§3.1.3(a) Reader Apps）
- Rust RFC 模板：https://github.com/rust-lang/rfcs/blob/master/0000-template.md
- Kubernetes KEP 模板：https://github.com/kubernetes/enhancements/blob/master/keps/NNNN-kep-template/README.md
- PRD 最佳实践：https://github.com/KhazP/vibe-coding-prompt-template （3.1k★）
