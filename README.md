# cadmus

> 希腊神话里，卡德摩斯把腓尼基字母带给了希腊。cadmus 做的事一样：把文本带到新的载体上。

一个先把书读好的平板阅读器：小说、教材、论文。本地文件进，舒服的排版、PDF 重排、离线读完；批注一键导出成 Markdown（笔记软件接入为后期可选）。

- 平台：iOS（仅 iPad，主）+ Android（平板/手机）；Rust 核心 + Tauri 2
- 差异化：**移动端 PDF 重排 × 本地优先 × 批注开放导出**

## 文档

| 文档 | 内容 | 版本 |
|---|---|---|
| [docs/PRODUCT.md](docs/PRODUCT.md) | 产品需求文档（PRD）：定位、用户、旅程、功能需求、非功能需求、度量、决策记录 | v1.1 |
| [docs/RESEARCH.md](docs/RESEARCH.md) | 研究附录：竞品格局、生态验证、合规依据、未验证假设 | v1.0 |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | 技术方案：选型、结构、阅读引擎、批注出口、构建发布 | v0.3 |
| [docs/CRITERIA.md](docs/CRITERIA.md) | 验收标准（先于实现注册） | v0.3 |
| [docs/DESIGN-ENGINE.md](docs/DESIGN-ENGINE.md) | 设计文档（M1 阅读引擎）：重排、EPUB/TXT 解析、块模型、CLI | v1.0 |
| [CHANGELOG.md](CHANGELOG.md) | 变更记录（文档与实现，自项目启动起） | — |

## 状态

PRD v1.1 · 2026-10-11 · M1 阅读引擎设计评审通过（见 docs/DESIGN-ENGINE.md）· 实现进行中
