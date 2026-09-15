# -*- coding: utf-8 -*-
"""校验修正版 EPUB 产物语义：mimetype 置首 STORED、条目命名、spine 页序、nav 引用、脏标签清除、侧放页朝向。

用法: python tools/check_output.py <修正版.epub> [原始输入.epub]

给出第二个参数（原始输入）时才能做朝向判定：侧放页在包里遗留的参照图 `<图名>-RAWIMAGE.<ext>`
用的是**原始图名**，产物里已重命名为 `image/001.jpg`，只靠产物自身对不上号。
"""
import hashlib
import re
import struct
import sys
import zipfile

p = sys.argv[1]
z = zipfile.ZipFile(p)
names = z.namelist()
info = z.getinfo("mimetype")
src = zipfile.ZipFile(sys.argv[2]) if len(sys.argv) > 2 else None
ok = True


def check(label, cond):
    global ok
    print(f"[{'PASS' if cond else 'FAIL'}] {label}")
    ok = ok and cond


def html_of(zf, name):
    return zf.read(name).decode("utf-8", "replace")


def title_of(zf, name):
    m = re.search(r"<title>(.*?)</title>", html_of(zf, name))
    return m.group(1) if m else None


def img_of(zf, name):
    m = re.search(r'src="\.\./(image/[^"]+)"', html_of(zf, name))
    return m.group(1) if m else None


def spine_hrefs(zf):
    opf = html_of(zf, "vol.opf")
    ids = re.findall(r'<itemref idref="(.*?)"', opf)
    hrefs = {}
    for i, href in re.findall(r'<item id="([^"]+)"[^>]*href="([^"]+)"', opf):
        hrefs[i] = href
    return [hrefs[i] for i in ids if i in hrefs]


def first_page_img(zf):
    """spine 里第一个既非封面也非结尾的页面所引用的图。"""
    for href in spine_hrefs(zf):
        if not href.endswith(".html") or href.endswith(("cover.html", "theend.html")):
            continue
        return img_of(zf, href)
    return None


check("mimetype 首条", names[0] == "mimetype")
check("mimetype STORED", info.compress_type == zipfile.ZIP_STORED)
print("--- entries ---")
for n in names:
    print(n)
opf = html_of(z, "vol.opf")
print("--- spine order ---")
print(re.findall(r'<itemref idref="(.*?)"', opf))
for href in re.findall(r'href="(.*?)"', opf):
    if href.endswith(".html") and href in names:
        print(href, "->", title_of(z, href))
print("--- nav hrefs ---")
print(re.findall(r'epub:href="(.*?)"', z.read("xml/vol.nav").decode("utf-8")))
dirty = sum(
    html_of(z, n).count(x)
    for n in names if n.endswith(".html")
    for x in ("kmoetag", "kimageraw", "raw=")
)
check(f"脏标签已清除（剩余 {dirty}）", dirty == 0)


# ---------------- 侧放页朝向 ----------------
# 复刻 `src/core.rs::plan_cover_rotation` 的判据与候选：cover.html 的封面图、正文第 1 页的图，
# 以及任何在输入包里带同名参照图 `<图名>-RAWIMAGE.<ext>` 的页（有参照图 = 包自己声明这页侧放）。
# 把候选图四个朝向分别与参照图做归一化相关度，取宽高比一致且最像的朝向；产物已回正时最像的应为 0°。
# 配对按**页序位置**（站点卡片页被移出正文后，页码会前移，按页号找参照图会错位）。


def jpeg_png_size(b):
    if b[:8] == b"\x89PNG\r\n\x1a\n":
        return struct.unpack(">II", b[16:24])
    i = 2
    while i < len(b) - 9:
        if b[i] != 0xFF:
            i += 1
            continue
        m = b[i + 1]
        if m in (0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF):
            h, w = struct.unpack(">HH", b[i + 5:i + 9])
            return w, h
        if m in (0xD8, 0xD9) or 0xD0 <= m <= 0xD7:
            i += 2
            continue
        i += 2 + struct.unpack(">H", b[i + 2:i + 4])[0]
    raise ValueError("无法解析图片尺寸")


def ncc(a, b):
    n = len(a)
    if n == 0 or n != len(b):
        return 0.0
    ma, mb = sum(a) / n, sum(b) / n
    va = sum((x - ma) ** 2 for x in a) ** 0.5
    vb = sum((y - mb) ** 2 for y in b) ** 0.5
    if va < 1e-6 or vb < 1e-6:
        return 0.0
    return sum((x - ma) * (y - mb) for x, y in zip(a, b)) / (va * vb)


def gray_grid(img, w, h):
    return list(img.convert("L").resize((w, h)).tobytes())


def page_no_of(zf, href):
    """页面 <title> 里的页码（`第 3 頁` / `第3话` 都取第一个数字）。"""
    m = re.search(r"(\d+)", title_of(zf, href) or "")
    return int(m.group(1)) if m else None


def raw_ref(src, img):
    """输入包里 `img` 的同名参照图条目名（没有则 None）。

    参照图的扩展名可能与页图不同（`[Kmoe][電鋸人2]` 是页图 `.jpg` + 参照图 `.png`），
    先按原扩展名精确找，找不到再接受任意扩展名的同名参照图——与 `core.rs::raw_reference` 一致。
    """
    if not img:
        return None
    names = src.namelist()
    base, _, ext = img.rpartition(".")
    want = f"{base}-RAWIMAGE.{ext}".lower()
    for n in names:
        if n.lower() == want:
            return n
    prefix = f"{base}-RAWIMAGE.".lower()
    for n in names:
        if n.lower().startswith(prefix):
            return n
    return None


def is_card(href):
    """站点卡片页（产物里命名为 `html/kmoe-NNN.html`）。"""
    return href.rsplit("/", 1)[-1].startswith("kmoe-")


def story_hrefs(zf):
    """spine 里的正文页（不含 cover / theend）。"""
    return [
        h
        for h in spine_hrefs(zf)
        if h.endswith(".html") and not h.endswith(("cover.html", "theend.html"))
    ]


def src_story(src):
    """输入包的正文页（按 spine 顺序）：[(页号, 图名, 参照图或 None)]。"""
    out = []
    for href in story_hrefs(src):
        img = img_of(src, href)
        out.append((page_no_of(src, href), img, raw_ref(src, img)))
    return out


def card_source_indices(src, dst_cards):
    """产物里卡片图对应的**源正文页下标**。

    站点卡片页不参与回正，产物里的图与源图逐字节相同，据此把源里的卡片位置认出来——
    它本身也是源包的一个正文页，配对时必须先摘掉，否则卡片之后的页会整体错位一格。
    """
    by_hash = {}
    for i, (_no, img, _ref) in enumerate(src_story_list):
        if img and img in src_names:
            by_hash.setdefault(hashlib.sha256(src.read(img)).digest(), []).append(i)
    out = []
    for href in dst_cards:
        img = img_of(z, href)
        if not img or img not in names:
            continue
        d = hashlib.sha256(z.read(img)).digest()
        if by_hash.get(d):
            out.append(by_hash[d].pop(0))
    return sorted(out)


src_names = set(src.namelist()) if src is not None else set()
src_story_list = src_story(src) if src is not None else []
dst_story = story_hrefs(z)
cards = [h for h in dst_story if is_card(h)]
dst_pages = [h for h in dst_story if not is_card(h)]
src_cards = card_source_indices(src, cards) if src is not None else []
src_pair = [x for i, x in enumerate(src_story_list) if i not in src_cards]

if src is not None:
    check(
        f"正文页数与源一致（{len(dst_pages)} 页 + 卡片 {len(cards)}，源 {len(src_story_list)} 页）",
        len(dst_pages) == len(src_pair) and len(cards) == len(src_cards),
    )
    check(
        "站点卡片页排在正文之后、theend 之前",
        dst_story == dst_pages + cards and spine_hrefs(z)[-1].endswith("theend.html"),
    )

# 产物里的候选：产物图名 -> (角色, 参照图名或 None)
cands = {}
cover_img = img_of(z, "html/cover.html")
if cover_img:
    cover_ref = raw_ref(src, img_of(src, "html/cover.html")) if src is not None else None
    cands[cover_img] = ("封面", cover_ref)
for i, href in enumerate(dst_pages):
    img = img_of(z, href)
    if not img:
        continue
    ref = src_pair[i][2] if src is not None and i < len(src_pair) else None
    if ref or i == 0:
        cands[img] = (f"第 {i + 1} 页", ref)

if not cands:
    print("(未找到侧放页候选图，跳过朝向检查)")
else:
    try:
        from io import BytesIO

        from PIL import Image

        print("--- sideways orientation ---")
        for out_img, (role, ref) in cands.items():
            if out_img not in names:
                print(f"  {role} {out_img} 不在产物中，跳过")
                continue
            w, h = jpeg_png_size(z.read(out_img))
            if ref is None or src is None:
                print(f"  {role} {out_img}: {w}x{h}（未定位到参照图，本脚本不判方向）")
                continue
            refer = Image.open(BytesIO(src.read(ref)))
            rw, rh = refer.size
            grid = (64, 32) if rw >= rh else (32, 64)
            ref_grid = gray_grid(refer, *grid)
            cand = Image.open(BytesIO(z.read(out_img)))
            scores = {}
            for deg in (0, 90, 180, 270):
                r = cand.rotate(-deg, expand=True) if deg else cand
                ar_ok = abs((r.width / r.height) / (rw / rh) - 1.0) <= 0.15
                scores[deg] = ncc(gray_grid(r, *grid), ref_grid) if ar_ok else float("-inf")
            best = max(scores, key=scores.get)
            shown = " ".join(f"{d}°={scores[d]:+.2f}" for d in (0, 90, 180, 270))
            print(f"  {role} {out_img}: {w}x{h}，参照 {ref} {rw}x{rh} -> {shown}")
            check(f"{role} {out_img} 朝向与参照图一致（最像 {best}°）", best == 0)
    except ImportError:
        print("（未安装 Pillow，只报尺寸不判朝向）")
        for out_img, role in cands.items():
            if out_img in names:
                print(f"  {role} {out_img}: {jpeg_png_size(z.read(out_img))}")

# ---------------- 与原始输入的差异（可选） ----------------
if src is not None:
    snames = src_names
    print("--- 与原始输入的差异（页图与元数据以外应逐字节一致）---")
    changed = [n for n in names if n in snames and z.read(n) != src.read(n)]
    dst_imgs = {img_of(z, h) for h in spine_hrefs(z)} | {"image/cover.jpg"}
    dst_imgs = {i for i in dst_imgs if i}
    unexpected = [
        n for n in changed
        if n not in cands
        and n not in dst_imgs
        and not n.endswith(".html")
        and n != "vol.opf"
        and not n.endswith("vol.nav")
    ]
    print("  内容有变化的条目:", changed)
    check(f"页图与元数据（html/opf/nav）以外无变化（{len(unexpected)} 条）", not unexpected)

    # 正文页图只允许在「有参照图 → 被回正」时变化；页序配对（源里的卡片先摘掉）
    changed_pages = []
    for i, href in enumerate(dst_pages):
        if i >= len(src_pair):
            break
        s_img, d_img = src_pair[i][1], img_of(z, href)
        if not s_img or not d_img or s_img not in snames or d_img not in names:
            continue
        if src.read(s_img) != z.read(d_img):
            changed_pages.append((i + 1, d_img))
    stray = [(i, img) for i, img in changed_pages if img not in cands]
    print(f"  正文页图有变化的：{len(changed_pages)} 张 -> {changed_pages}")
    check(f"正文页图只在有参照图时变化（异常 {len(stray)} 张）", not stray)

print("RESULT:", "PASS" if ok else "FAIL")
sys.exit(0 if ok else 1)
