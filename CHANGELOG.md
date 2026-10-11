# Changelog

> cadmus 变更记录：只记「改了什么」，方便日后追溯。分类参考 [Keep a Changelog](https://keepachangelog.com/)。
> App 版本发布前，条目以文档集版本为锚；发布后改用语义化版本号。

## Unreleased

### 新增
- CHANGELOG.md
- DESIGN-ENGINE.md：M1 阅读引擎设计 v1.0（评审通过）
- crates/cadmus-core：块模型、锚点与稳定块 id（M1 第 1 步）
- crates/cadmus-epub：TXT 解析（编码探测、章节切分、块序列生成）

### 变更
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
