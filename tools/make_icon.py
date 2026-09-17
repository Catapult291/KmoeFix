# -*- coding: utf-8 -*-
"""生成 KmoeFix 的图标资源：assets/kmoefix.ico（多档）与 assets/kmoefix.png（窗口图标）。

用法：python tools/make_icon.py
依赖：Pillow（与 tools/ 下其他脚本一样，只是维护工具，不进构建流程）。

设计：近黑圆角方块 #0D1116 + 白色小写 e，方块圆角与字母大小/笔画按参考图实测比例
（圆角 = 23% 方块边长，字母 x-height = 0.52 方块边长，笔画 ≈ 0.05）。16 / 24 px 两档
换成 Segoe UI Semilight：Light 的笔画在 16px 下会糊成灰块，实测无纯白像素。
"""

import os
import struct
import sys

from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ICO_PATH = os.path.join(ROOT, "assets", "kmoefix.ico")
PNG_PATH = os.path.join(ROOT, "assets", "kmoefix.png")

CANVAS = 512                       # 母版画布
INSET = 36                         # 四周留白，圆角方块 440x440
RADIUS = 104                       # 圆角半径
DARK = (13, 17, 22, 255)           # #0D1116
GLYPH_SIZE = 437                   # 使 x-height 落在 0.52 * 440

FONT_DIR = os.path.join(os.environ.get("SystemRoot", r"C:\Windows"), "Fonts")
FONT_LIGHT = os.path.join(FONT_DIR, "segoeuil.ttf")
FONT_SEMILIGHT = os.path.join(FONT_DIR, "segoeuisl.ttf")

# 档位 -> 字体：小尺寸加粗一档
SIZES = {16: FONT_SEMILIGHT, 24: FONT_SEMILIGHT, 32: FONT_LIGHT,
         48: FONT_LIGHT, 64: FONT_LIGHT, 128: FONT_LIGHT, 256: FONT_LIGHT}


def render(font_path):
    """画 512 母版：圆角方块 + 居中白色小写 e。"""
    if not os.path.exists(font_path):
        sys.exit("缺少字体文件: " + font_path)
    im = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle([INSET, INSET, CANVAS - INSET, CANVAS - INSET], radius=RADIUS, fill=DARK)
    font = ImageFont.truetype(font_path, GLYPH_SIZE)
    left, top, right, bottom = d.textbbox((0, 0), "e", font=font)
    d.text(((CANVAS - (right - left)) / 2 - left, (CANVAS - (bottom - top)) / 2 - top),
           "e", font=font, fill=(255, 255, 255, 255))
    return im


def dib_frame(im):
    """32bpp DIB（BITMAPINFOHEADER + BGRA + AND 掩码），ICO 里 <=128px 用这档最兼容。"""
    w, h = im.size
    px = im.load()
    header = struct.pack("<IiiHHIIiiII", 40, w, h * 2, 1, 32, 0, w * h * 4, 0, 0, 0, 0)
    pixels = bytearray()
    for y in range(h - 1, -1, -1):          # DIB 行自下而上
        for x in range(w):
            r, g, b, a = px[x, y]
            pixels += bytes((b, g, r, a))
    mask_row = (w + 31) // 32 * 4           # AND 掩码按 4 字节对齐
    mask = bytearray()
    for y in range(h - 1, -1, -1):
        row = bytearray(mask_row)
        for x in range(w):
            if px[x, y][3] < 128:
                row[x // 8] |= 0x80 >> (x % 8)
        mask += row
    return bytes(header) + bytes(pixels) + bytes(mask)


def png_frame(im):
    import io

    buf = io.BytesIO()
    im.save(buf, format="PNG", optimize=True)
    return buf.getvalue()


def write_ico(path, frames):
    """frames: [(size, image_bytes)]，256px 用 PNG，其余用 DIB。"""
    out = bytearray(struct.pack("<HHH", 0, 1, len(frames)))
    offset = 6 + 16 * len(frames)
    for size, blob in frames:
        out += struct.pack("<BBBBHHII", size if size < 256 else 0, size if size < 256 else 0,
                           0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    for _, blob in frames:
        out += blob
    with open(path, "wb") as f:
        f.write(bytes(out))


def main():
    os.makedirs(os.path.dirname(ICO_PATH), exist_ok=True)
    masters = {}
    frames = []
    for size in sorted(SIZES):
        font = SIZES[size]
        if font not in masters:
            masters[font] = render(font)
        small = masters[font].resize((size, size), Image.LANCZOS)
        frames.append((size, png_frame(small) if size == 256 else dib_frame(small)))
    write_ico(ICO_PATH, frames)
    masters[FONT_LIGHT].resize((256, 256), Image.LANCZOS).save(PNG_PATH)
    print("wrote", ICO_PATH, os.path.getsize(ICO_PATH), "bytes")
    print("wrote", PNG_PATH, os.path.getsize(PNG_PATH), "bytes")


main()
