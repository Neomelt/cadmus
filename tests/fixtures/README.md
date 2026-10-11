# 测试素材（fixtures）

对应 `docs/CRITERIA.md` 的 F1–F6。**版权受限文件不入库**（已在 `.gitignore` 排除 pdf/epub/txt），只提交来源说明与脚本。

| ID | 样本 | 来源 | 获取方式 |
|---|---|---|---|
| F1 | 双栏论文（InEKF，arXiv 1812.01537） | arXiv | `curl -L -o 1812.01537.pdf https://arxiv.org/pdf/1812.01537` |
| F2 | 双栏论文（含算法/代码，arXiv 2505.04769） | arXiv | `curl -L -o 2505.04769.pdf https://arxiv.org/pdf/2505.04769` |
| F3 | 扫描版 PDF（无文本层） | 自备 | 用户自备，不入库 |
| F4 | 中文教材页（中英混排） | 自备 | 用户自备，不入库 |
| F5 | EPUB 小说（公版优先） | 自备 | 用户自备，不入库 |
| F6 | 大体积中文 TXT（>5MB） | 自备 | 用户自备，不入库 |

## 合成样本

无版权、可入库：

- `make_smoke_pdf.py`：生成单页双栏小 PDF（冒烟测试：pdfium 加载、分栏检测、阅读顺序）。

```bash
python3 make_smoke_pdf.py /tmp/smoke.pdf
```

## 重排质量抽样协议（CR-1）

每份样本取 3 个页面（含至少 1 页含图/公式），从原文人工转录 3 个段落，检验重排结果中这 3 段文字内容一致且顺序正确。

## 开发环境：pdfium

`cadmus-reflow` 运行时需要 `libpdfium.so`（pdfium-render 动态绑定）：

- 默认查找 `./vendor/pdfium/lib/libpdfium.so`（不入库）；
- 可设 `CADMUS_PDFIUM_LIB=<目录>` 覆盖；
- 都没有时回退系统库。

获取方式示例：bblanchon/pdfium-binaries 的 `pdfium-linux-x64.tgz`，或复用已安装的 pypdfium2：

```bash
python3 -c "import pypdfium2_raw, os; print(os.path.dirname(pypdfium2_raw.__file__))"
# 然后：ln -s <该目录>/libpdfium.so vendor/pdfium/lib/libpdfium.so
```

