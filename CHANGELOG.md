# Changelog

> cadmus 变更记录：只记「改了什么」，方便日后追溯。分类参考 [Keep a Changelog](https://keepachangelog.com/)。
> App 版本发布前，条目以文档集版本为锚；发布后改用语义化版本号。

## Unreleased

### 新增
- CHANGELOG.md
- DESIGN-ENGINE.md：M1 阅读引擎设计 v1.0（评审通过）
- crates/cadmus-core：块模型、锚点与稳定块 id（M1 第 1 步）
- crates/cadmus-epub：TXT 解析（编码探测、章节切分、块序列生成）
- cadmus-cli：命令面（reflow/epub/txt/bench）；TXT 已接通
- crates/cadmus-reflow：PDF 重排（pdfium 适配、分栏检测、段落/区域/阅读顺序、原子块栅格化、整页降级）
- crates/cadmus-epub：EPUB 解析（容器/OPF/spine、XHTML→块序列、图片导出）
- cadmus-cli：reflow/epub 接通（reflow 支持 --dump json 与占位符输出）
- tests/fixtures：F1–F6 来源说明与合成双栏 PDF 脚本；CI（fmt/clippy/test）
- cadmus-cli：`--dump html` 预览输出（浏览器直接阅读，含公式/图位图）
- 预览：阅读器体验升级（主题/字号/行宽/目录/封面作者）
- cadmus-epub：dc:creator 与封面（EPUB2/3）解析导出
- src-tauri：Tauri 2 空壳（iOS 出包链路验证）；ios.yml 产出未签名 IPA（SideStore 侧载用，D12）
- ios：未签名 IPA 流水线跑通（macos-15 + `tauri ios build --no-sign`；Release/artifact 下载）
- app：引擎接入（内置示例书可读：EPUB → 块序列 → 阅读 HTML；主题/字号/行宽/目录）；App 图标/设备族修正（iPad 专用）；官方阅读渲染器抽为 cadmus-preview crate
- app v0.1.2：书库（导入 EPUB/TXT、删除、阅读进度记忆）；阅读设置面板升级（字号/行距/行宽滑条，字体 宋/黑/楷，主题 白/米黄/护眼/深灰/纯黑）
- ios：IPA 产物带版本号（cadmus-<version>-unsigned.ipa）；workflow 摘要标注版本
- app v0.1.3：修复导入失败（iOS 沙箱外文件需开启 security-scoped 访问）与阅读进度不更新（一屏读完记 100%、滚动位置多来源兼容）；真书实测通过（《浮士德》9.6MB：元数据/封面/阅读/进度/设置面板）
- app v0.1.4：阅读页正文图片与封面内联显示（此前图片只导出不显示）；修复 `<p>`/列表/SVG 内嵌图片丢失；超限（>2.5MB）或缺失图片整体省略（不裂图）；导出资产统一到 `assets/<id>/`，已存在不重写
- app v0.1.5：脚注显示（noteref 标记规范为 `[n]` 保留在正文，脚注正文渲染为小号脚注块，剔除回链箭头）
- app v0.2.0：PDF 导入与重排阅读（重排一次缓存为 `assets/<id>/index.html`，二次打开秒开）；iOS 包内嵌 pdfium 动态库（chromium/8086，运行时按应用包路径加载）；新增 THIRD-PARTY.md
- app v0.2.1：PDF 断字连字符合并（行尾连字符 + 小写续行去连字符；缺 Unicode 映射的连字符字形按控制符/非字符识别后同样处理）；重排缓存带版本号（`reflow-v1.html`，算法升级自动失效重排）
- app v0.2.2：脚注配对（`noteref` 目标解析：章节末注/独立注释文件的脚注移动到引用段落之后；未引用的独立注释不进入正文）
- app v0.2.3：修复 iOS 更新/重装后此前导入的书打不开、封面丢失（索引改存相对路径；旧失效绝对路径启动时自动自愈）；发布含 v0.2.2 全部内容
- app v0.3.0：一期功能齐套——批注系统（多色高亮/附着笔记/书签/标注面板/导出 Markdown）；全文搜索；位置锚点（换字号/行宽不漂移）；书库排序（最近阅读/导入时间/标题）与筛选；阅读设置补边距/对齐；PDF 元数据与原文页⇄重排切换；修复 pdfium 二次初始化；文档模型 serde 化（PDF 重排缓存改存文档 JSON）
- app v0.3.1：修复评审发现——原页页码 0/1 基混用（重排 page_range 统一 1 基，原文页/导出 p.N 全对齐）；搜索大小写回退越界风险；HTML 属性引号转义（笔记含引号不截断）；删书清理批注文件（防旧标注复活）；原文↔重排切换定时器泄漏
- CI：产出 cadmus-cli Linux 发布产物（Actions artifact）

### 变更
- DESIGN-ENGINE v1.1：脚注配对语义（noteref 目标解析、注释跟随引用段落、未引用独立注释丢弃）；PDF 行拼接断字连字符规则
- PRODUCT v1.1：设备范围明确（iOS 仅 iPad、手机仅 Android，D11）；FR-2.11 提为 M；补问题④；非目标格式与 DRM；性能基线
- ARCHITECTURE v0.3：设备范围对齐；批注导出模板；性能基线
- CRITERIA v0.3：CR-1 视口修正；CR-3/4/9 对齐 M2；性能基线
- README：平台范围与文档版本同步
- 全文档：去 AI 味（humanizer）：改写破折号衔接句、口号式对比句、装饰性加粗
- PRODUCT §2.2：陈旧交叉引用修正（调研证据指向 RESEARCH §3）

## 2026-10-11 · PRD v1.0 + 研究附录

### 新增
- RESEARCH v1.0：研究附录（竞品、生态、合规、未验证假设）

### 变更
- PRODUCT v1.0：重写为正式 PRD（画像、旅程、FR/NFR、度量、决策记录）

## 2026-10-11 · v0.2 阅读器优先

### 变更
- PRODUCT v0.2：定位收敛为阅读器优先（小说/教材/论文）；笔记接入降二期
- ARCHITECTURE v0.2：三类管线 PDF/EPUB/TXT；source/syncd 降二期
- CRITERIA v0.2：新增 CR-8/CR-9；CR-5/6 源部分降二期

## 2026-10-11 · 命名 cadmus

### 变更
- 产品命名 cadmus（D1）

## 2026-10-11 · v0.1 项目启动

### 新增
- README、PRODUCT v0.1、ARCHITECTURE v0.1、CRITERIA v0.1
