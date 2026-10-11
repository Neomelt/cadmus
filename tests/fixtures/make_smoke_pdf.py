#!/usr/bin/env python3
"""生成单页双栏合成 PDF（冒烟测试用，无版权问题，无第三方依赖）。

用法：python3 make_smoke_pdf.py [输出路径，默认 smoke.pdf]
"""

import sys


def build_pdf() -> bytes:
    lines = []
    y0 = 720
    for index in range(20):
        y = y0 - index * 14
        lines.append(
            f"BT /F1 10 Tf 72 {y} Td "
            f"(Left column line {index + 1:02d} of the smoke test document.) Tj ET"
        )
        lines.append(
            f"BT /F1 10 Tf 340 {y} Td "
            f"(Right column line {index + 1:02d} of the smoke test document.) Tj ET"
        )
    content = "\n".join(lines).encode("latin-1")

    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        b"<< /Length " + str(len(content)).encode() + b" >>\nstream\n" + content + b"\nendstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]

    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for number, obj in enumerate(objects, start=1):
        offsets.append(len(out))
        out += f"{number} 0 obj\n".encode() + obj + b"\nendobj\n"

    xref_pos = len(out)
    out += f"xref\n0 {len(objects) + 1}\n".encode()
    out += b"0000000000 65535 f \n"
    for offset in offsets:
        out += f"{offset:010d} 00000 n \n".encode()
    out += (
        f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\n"
        f"startxref\n{xref_pos}\n%%EOF\n"
    ).encode()
    return bytes(out)


if __name__ == "__main__":
    target = sys.argv[1] if len(sys.argv) > 1 else "smoke.pdf"
    with open(target, "wb") as handle:
        handle.write(build_pdf())
    print(f"wrote {target}")
