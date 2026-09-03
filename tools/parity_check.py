#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
对拍脚本：用同一批样本分别喂给「Python 原版 fix_one」与「Rust kmoefix」，
逐项比对语义等价性（不比对 zip 字节，比对的是产物契约）。

用法:
    python tools/parity_check.py            # 构建 release 后自动跑全部场景
    python tools/parity_check.py <exe>      # 指定 rust 可执行文件路径

退出码: 0 = 全部一致; 1 = 有差异/异常
"""
import os
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
KMOE_SRC = os.environ.get("KMOE_PY_SRC")  # 指向原仓库（含 src/core.py）

SCENARIOS = []


def scenario(fn):
    SCENARIOS.append(fn)
    return fn


def require_py_src():
    if not KMOE_SRC:
        print("需要环境变量 KMOE_PY_SRC 指向原 Python 仓库路径（含 src/core.py）")
        sys.exit(2)
    sys.path.insert(0, KMOE_SRC)
    from src.core import fix_one as py_fix_one  # noqa
    return py_fix_one


def make_epub(path, spine_order, with_nav=True, cdata_title=False, extra_entries=None,
              relative_img=False, no_img_attr=False):
    """构造一个与 test_fix_one.py 同构的 epub。spine_order: [(href, 标题值), ...]"""
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    manifest_items, spine_refs, html_img_map = [], [], {}

    def img_for(href, title_val):
        if href.endswith("cover.html"):
            return "image/cover.jpg"
        if href.endswith("theend.html"):
            return "image/theend.jpg"
        try:
            return f"image/page-{int(title_val)}.jpg"
        except (TypeError, ValueError):
            m = re.search(r"(\d+)", href)
            return f"image/page-{m.group(1) if m else 0}.jpg"

    for idx, (href, tv) in enumerate(spine_order):
        iid = f"item{idx}"
        manifest_items.append((iid, href, "application/xhtml+xml"))
        spine_refs.append(iid)
        html_img_map[href] = img_for(href, tv)
    for idx, (href, _) in enumerate(spine_order):
        manifest_items.append((f"img{idx}", html_img_map[href], "image/jpeg"))

    opf = '<?xml version="1.0" encoding="utf-8"?>\n<package version="3.0">\n<manifest>\n'
    for iid, href, mt in manifest_items:
        opf += f'  <item id="{iid}" href="{href}" media-type="{mt}" />\n'
    opf += "</manifest>\n<spine>\n"
    for ref_ in spine_refs:
        opf += f'  <itemref idref="{ref_}" />\n'
    opf += "</spine>\n</package>"

    html_contents = {}
    for href, tv in spine_order:
        if tv == "cover":
            title = "Book Cover"
        elif tv == "theend":
            title = "THE END"
        elif isinstance(tv, int):
            title = f"第{tv}话"
        else:
            try:
                title = f"第{int(tv)}话"
            except (TypeError, ValueError):
                title = str(tv)
        if cdata_title:
            title = f"<![CDATA[{title}]]>"
        img = html_img_map[href]
        prefix = "" if relative_img else "../"
        attrs = "" if no_img_attr else ' kmoetag="dirty" kimageraw="dirty" raw="dirty"'
        html = (f'<?xml version="1.0" encoding="utf-8"?>\n'
                f'<html xmlns="http://www.w3.org/1999/xhtml">\n'
                f'<head><title>{title}</title></head>\n'
                f'<body>\n<div><img src="{prefix}{img}"{attrs} /></div>\n'
                f'<p>content for {href}</p>\n</body>\n</html>')
        html_contents[href] = html

    nav_content = None
    if with_nav:
        nav_content = '<?xml version="1.0" encoding="utf-8"?>\n<nav><ol>\n'
        for href, _ in spine_order:
            nav_content += f'  <li><a src="../{href}">{href}</a></li>\n'
        nav_content += "</ol></nav>"

    with zipfile.ZipFile(path, "w") as z:
        zi = zipfile.ZipInfo("mimetype")
        zi.compress_type = zipfile.ZIP_STORED
        z.writestr(zi, "application/epub+zip".encode("utf-8"))
        z.writestr("vol.opf", opf.encode("utf-8"))
        for href, html in html_contents.items():
            z.writestr(href, html.encode("utf-8"))
        for img in set(html_img_map.values()):
            z.writestr(img, b"\xff\xd8\xff fake jpeg content for " + img.encode("utf-8"))
        if with_nav and nav_content is not None:
            z.writestr("xml/vol.nav", nav_content.encode("utf-8"))
        z.writestr("META-INF/container.xml",
                   b'<?xml version="1.0"?><container><rootfiles><rootfile full-path="vol.opf" /></rootfiles></container>')
        for name, data in (extra_entries or {}).items():
            z.writestr(name, data)


def run_python(py_fix_one, src, dst):
    """返回 (ok, dst 路径 or None, 错误信息 or None)"""
    try:
        out = py_fix_one(src, dst)
        return True, out, None
    except Exception as e:  # noqa
        return False, None, str(e)


def run_rust(exe, src, dst):
    """调用 rust CLI 处理单个文件，解析输出。"""
    cmd = [exe, src]
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120)
    except Exception as e:  # noqa
        return False, None, f"调用 rust 失败: {e}"
    text = (proc.stdout or "") + (proc.stderr or "")
    if proc.returncode != 0 or "[OK]" not in text:
        m = re.search(r"\[FAIL\] .*?: (.*)", text)
        msg = m.group(1).strip() if m else text.strip() or "未知错误"
        return False, None, msg
    m = re.search(r"\[OK\] .*? -> (.*)", text)
    out = m.group(1).strip() if m else None
    return True, out, None


# ---------------- 场景 ----------------

@scenario
def s_sorted_with_cover_theend(py, exe, td):
    """顺序正确 + cover/theend + nav：主路径"""
    src = os.path.join(td, "sorted.epub")
    make_epub(src, [("html/cover.html", "cover"), ("html/page-1.html", 1),
                    ("html/page-2.html", 2), ("html/page-3.html", 3),
                    ("html/theend.html", "theend")])
    return src


@scenario
def s_shuffled_with_cover_theend(py, exe, td):
    """乱序 + cover/theend：能力扩展场景。

    原版 Python 从不排序 → 必然回滚；
    Rust 版按话数升序重排 → 应修复成功且通过回读校验。
    对拍结论: OK == 两者都不崩溃且各按自身语义产生一致文件集合/内容
             （不在本场景要求成败一致，见 main 中的分歧处理）。
    """
    src = os.path.join(td, "shuffled.epub")
    make_epub(src, [("html/page-3.html", 3), ("html/page-1.html", 1),
                    ("html/cover.html", "cover"), ("html/page-2.html", 2),
                    ("html/theend.html", "theend")])
    return src


@scenario
def s_no_nav(py, exe, td):
    """无 xml/vol.nav"""
    src = os.path.join(td, "nonav.epub")
    make_epub(src, [("html/page-1.html", 1), ("html/page-2.html", 2)], with_nav=False)
    return src


@scenario
def s_no_cover_theend(py, exe, td):
    """仅 page 页"""
    src = os.path.join(td, "nocent.epub")
    make_epub(src, [("html/page-1.html", 1), ("html/page-2.html", 2), ("html/page-3.html", 3)])
    return src


@scenario
def s_two_digit(py, exe, td):
    """两位数话数 → 宽度 2"""
    src = os.path.join(td, "twodigit.epub")
    make_epub(src, [("html/page-1.html", 1), ("html/page-2.html", 2),
                    ("html/page-10.html", 10), ("html/page-11.html", 11)])
    return src


@scenario
def s_hundred(py, exe, td):
    """三位数话数 → 宽度 3（需 padding.txt 挡 max_page>99 时 Python 用 %03d 而我用 width=len）"""
    src = os.path.join(td, "hundred.epub")
    make_epub(src, [("html/page-100.html", 100), ("html/page-101.html", 101)])
    return src


@scenario
def s_cdata_title(py, exe, td):
    """<title><![CDATA[第X话]]></title>：title 正则在该情形下不匹配 → 行为一致性（fallback 捕获）"""
    src = os.path.join(td, "cdata.epub")
    make_epub(src, [("html/page-1.html", 1), ("html/page-2.html", 2)], cdata_title=True)
    return src


@scenario
def s_existing_output(py, exe, td):
    """目标 _修正版.epub 已存在 → 自动 (1)"""
    src = os.path.join(td, "exist.epub")
    make_epub(src, [("html/page-1.html", 1), ("html/page-2.html", 2)])
    open(os.path.join(td, "exist_修正版.epub"), "wb").close()
    return src


@scenario
def s_gaps(py, exe, td):
    """页码不连续（1,3）：内容缺失的乱序，无法通过排序补齐 → 仍应失败回滚"""
    src = os.path.join(td, "gap.epub")
    make_epub(src, [("html/page-1.html", 1), ("html/page-3.html", 3)])
    return src


@scenario
def s_shuffled_no_cover_theend(py, exe, td):
    """乱序且无 cover/theend：同样是能力扩展场景（Rust 应修复成功）"""
    src = os.path.join(td, "shuffled_nocent.epub")
    make_epub(src, [("html/page-3.html", 3), ("html/page-1.html", 1),
                    ("html/page-2.html", 2)])
    return src


@scenario
def s_sorted_wo_nav(py, exe, td):
    """顺序正确但无 nav：本场景两侧都成功，产物应逐字节一致"""
    src = os.path.join(td, "sorted_nonav.epub")
    make_epub(src, [("html/page-1.html", 1), ("html/page-2.html", 2)], with_nav=False)
    return src


@scenario
def s_extra_entries(py, exe, td):
    """无关条目（目录、字体）应原样保留"""
    src = os.path.join(td, "extra.epub")
    make_epub(src, [("html/page-1.html", 1), ("html/page-2.html", 2)],
              extra_entries={"fonts/f.otf": b"\x00\x01OTTO", "META-INF/encryption.xml": b"<enc/>"})
    return src


# ---------------- 比对 ----------------

def compare_products(td, src, py_out, rust_out):
    """对成功产物做语义比对；返回错误列表"""
    errs = []

    def read_zip(p):
        z = zipfile.ZipFile(p)
        names = z.namelist()
        data = {n: z.read(n) for n in names}
        info = {n: z.getinfo(n) for n in names}
        return names, data, info

    pn, pd, pi = read_zip(py_out)
    rn, rd, ri = read_zip(rust_out)

    if pn != rn:
        errs.append(f"namelist 不一致:\n  py  : {pn}\n  rust: {rn}")
    # 内容比对（仅比较两侧都有的名字，缺失已在 namelist 差异报出）
    for n in pn:
        if n in rd and pd[n] != rd[n]:
            errs.append(f"文件内容不一致: {n}")

    # mimetype 首条 + STORED
    for tag, names, info in (("py", pn, pi), ("rust", rn, ri)):
        if names[0] != "mimetype":
            errs.append(f"[{tag}] 首条不是 mimetype: {names[:3]}")
        if info["mimetype"].compress_type != zipfile.ZIP_STORED:
            errs.append(f"[{tag}] mimetype 未用 STORED")
    return errs


def main():
    py_fix_one = require_py_src()
    exe = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "..", "target", "release", "kmoefix.exe")
    exe = os.path.abspath(exe)
    if not os.path.exists(exe):
        print(f"找不到 rust 可执行文件: {exe}")
        print("请先运行: cargo build --release")
        sys.exit(2)

    total = 0
    failed = 0
    for fn in SCENARIOS:
        total += 1
        td = tempfile.mkdtemp(prefix="kmoeparity_")
        try:
            src = fn(py_fix_one, exe, td)
            name = fn.__name__

            # python 与 rust 各自独立处理同一份 src 副本（两者都会写 *_修正版.epub）
            py_src = os.path.join(td, "py", os.path.basename(src))
            rust_src = os.path.join(td, "rust", os.path.basename(src))
            os.makedirs(os.path.dirname(py_src))
            os.makedirs(os.path.dirname(rust_src))
            shutil.copy2(src, py_src)
            shutil.copy2(src, rust_src)

            py_ok, py_out, py_err = run_python(py_fix_one, py_src, None)
            rust_ok, rust_out, rust_err = run_rust(exe, rust_src, None)

            errs = []
            if name in ("s_shuffled_with_cover_theend", "s_shuffled_no_cover_theend"):
                # 能力扩展场景：原版 Python 从不排序 → 乱序回滚；
                # Rust 版按话数重排 → 应修复成功且通过回读校验。
                # 成败不一致本身不算错误，只校验 Rust 侧确实能修。
                if not rust_ok:
                    errs.append(f"[能力扩展] rust 应修复成功却失败: {rust_err}")
                else:
                    with zipfile.ZipFile(rust_out) as z:
                        opf = z.read("vol.opf").decode("utf-8")
                        idrefs = re.findall(r'<itemref[^>]*idref="([^"]+)"[^>]*>', opf)
                        hrefs = []
                        for iid in idrefs:
                            m = re.search(rf'<item[^>]*id="{re.escape(iid)}"[^>]*href="([^"]+)"', opf)
                            if m:
                                hrefs.append(m.group(1))
                        pages = [int(re.search(r"page-(\d+)\.html", h).group(1))
                                 for h in hrefs if re.search(r"page-\d+\.html", h)]
                        if pages != list(range(1, len(pages) + 1)):
                            errs.append(f"[能力扩展] rust 产物 spine 未连续: {pages}")
                if py_ok:
                    errs.append(f"[能力扩展] python 原版不应成功却成功: {py_out}")
            elif py_ok != rust_ok:
                errs.append(f"成败不一致: py={py_ok} ({py_err})  rust={rust_ok} ({rust_err})")
            elif py_ok:
                # 都成功 → 比产物
                if not py_out or not rust_out:
                    errs.append(f"成功但缺少输出路径: py={py_out} rust={rust_out}")
                else:
                    errs.extend(compare_products(td, src, py_out, rust_out))
            else:
                # 都失败 → 比错误信息关键子串
                if py_err and rust_err:
                    if not re.search(r"回读校验失败|未找到|spine 为空|无法", rust_err):
                        errs.append(f"rust 错误信息异常: {rust_err!r} (py: {py_err!r})")

            if errs:
                failed += 1
                print(f"[FAIL] {name}")
                for e in errs:
                    print("   " + e.replace("\n", "\n   "))
            else:
                print(f"[PASS] {name}")
        finally:
            shutil.rmtree(td, ignore_errors=True)

    print(f"\n{total - failed}/{total} 场景一致")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
