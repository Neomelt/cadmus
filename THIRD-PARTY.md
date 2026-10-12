# 第三方组件

| 组件 | 用途 | 许可 |
|---|---|---|
| [PDFium](https://pdfium.googlesource.com/pdfium/)（经 [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) 预编译，`chromium/8086`） | PDF 解析与栅格化（`cadmus-reflow`）；iOS 包内嵌 `Frameworks/libpdfium.dylib` | BSD-3-Clause（含第三方组件许可，见上游包内 `licenses/`） |
| Tauri、WebKitGTK、WebKit、zip、roxmltree、chardetng、encoding_rs、image、base64、serde 等 | 运行时与解析依赖 | 各自许可（见 `Cargo.lock` 对应 crate） |
