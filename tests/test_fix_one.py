# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import sys
import zipfile
import tempfile

import pytest

_HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.abspath(os.path.join(_HERE, ".."))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

from src.core import fix_one as core_fix_one
from src.core import get_unique_dst


def make_epub(path: str, spine_order: list[tuple[str, object]], with_nav: bool = True) -> None:
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)

    manifest_items: list[tuple[str, str, str]] = []
    spine_refs: list[str] = []
    html_img_map: dict[str, str] = {}

    for idx, (href, title_val) in enumerate(spine_order):
        iid = f"item{idx}"
        manifest_items.append((iid, href, "application/xhtml+xml"))
        spine_refs.append(iid)
        if href.endswith("cover.html"):
            img = "image/cover.jpg"
        elif href.endswith("theend.html"):
            img = "image/theend.jpg"
        else:
            if isinstance(title_val, int):
                img = f"image/page-{int(title_val)}.jpg"
            else:
                try:
                    n = int(str(title_val))
                    img = f"image/page-{n}.jpg"
                except Exception:
                    import re as _re

                    m = _re.search(r"(\d+)", href)
                    img = f"image/page-{m.group(1) if m else idx}.jpg"
        html_img_map[href] = img

    for idx, (href, _) in enumerate(spine_order):
        img = html_img_map[href]
        iid = f"img{idx}"
        manifest_items.append((iid, img, "image/jpeg"))

    opf = '<?xml version="1.0" encoding="utf-8"?>\n<package version="3.0">\n<manifest>\n'
    for iid, href, mt in manifest_items:
        opf += f'  <item id="{iid}" href="{href}" media-type="{mt}" />\n'
    opf += '</manifest>\n<spine>\n'
    for ref in spine_refs:
        opf += f'  <itemref idref="{ref}" />\n'
    opf += '</spine>\n</package>'

    html_contents: dict[str, str] = {}
    for href, title_val in spine_order:
        if title_val == "cover":
            title = "Book Cover"
        elif title_val == "theend":
            title = "THE END"
        elif isinstance(title_val, int):
            title = f"第{int(title_val)}话"
        else:
            try:
                n = int(str(title_val))
                title = f"第{n}话"
            except Exception:
                title = str(title_val)
        img = html_img_map[href]
        html = f'''<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{title}</title></head>
<body>
<div><img src="../{img}" kmoetag="dirty" kimageraw="dirty" raw="dirty" /></div>
<p>content for {href}</p>
</body>
</html>'''
        html_contents[href] = html

    nav_content: str | None = None
    if with_nav:
        nav_content = '<?xml version="1.0" encoding="utf-8"?>\n<nav><ol>\n'
        for href, _ in spine_order:
            nav_content += f'  <li><a src="../{href}">{href}</a></li>\n'
        nav_content += '</ol></nav>'

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
        z.writestr("META-INF/container.xml", b'<?xml version="1.0"?><container><rootfiles><rootfile full-path="vol.opf" /></rootfiles></container>')


def test_sorted_epub_fix():
    with tempfile.TemporaryDirectory() as td:
        src = os.path.join(td, "a.epub")
        spine = [
            ("html/cover.html", "cover"),
            ("html/page-1.html", 1),
            ("html/page-2.html", 2),
            ("html/page-3.html", 3),
            ("html/theend.html", "theend"),
        ]
        make_epub(src, spine, with_nav=True)

        dst = core_fix_one(src)
        assert os.path.exists(dst)
        assert dst.endswith("_修正版.epub")
        assert os.path.exists(src)

        tmp_path = dst + ".tmp"
        assert not os.path.exists(tmp_path)

        with zipfile.ZipFile(dst, "r") as z:
            namelist = z.namelist()
            assert namelist[0] == "mimetype"
            info = z.getinfo("mimetype")
            assert info.compress_type == zipfile.ZIP_STORED
            data = z.read("mimetype")
            assert data == b"application/epub+zip"

            assert "html/cover.html" in namelist
            assert "html/theend.html" in namelist
            assert "html/page-001.html" in namelist
            assert "html/page-002.html" in namelist
            assert "html/page-003.html" in namelist
            assert "html/page-1.html" not in namelist
            assert "html/page-2.html" not in namelist
            assert "html/page-3.html" not in namelist

            assert "image/001.jpg" in namelist
            assert "image/002.jpg" in namelist
            assert "image/003.jpg" in namelist
            assert "image/cover.jpg" in namelist
            assert "image/theend.jpg" in namelist

            for name in namelist:
                if name.endswith(".html"):
                    html = z.read(name).decode("utf-8")
                    assert "kmoetag" not in html
                    assert "kimageraw" not in html
                    assert ' raw=' not in html

            opf = z.read("vol.opf").decode("utf-8")
            assert 'href="html/page-001.html"' in opf
            assert 'href="html/page-002.html"' in opf
            assert 'href="html/page-003.html"' in opf
            assert 'href="html/cover.html"' in opf
            assert 'href="html/theend.html"' in opf
            assert 'href="image/001.jpg"' in opf

            if "xml/vol.nav" in namelist:
                nav = z.read("xml/vol.nav").decode("utf-8")
                assert "html/page-001.html" in nav or "page-001" in nav

        # get_unique_dst 行为：已存在 dst 再次调用应返回 (1) 后缀
        nxt = get_unique_dst(src)
        assert nxt.endswith("_修正版 (1).epub")
        assert nxt != dst
        # 写入一个占位文件验证递增
        open(nxt, "wb").close()
        nxt2 = get_unique_dst(src)
        assert nxt2.endswith("_修正版 (2).epub")


def test_shuffled_epub_raises():
    with tempfile.TemporaryDirectory() as td:
        src = os.path.join(td, "b.epub")
        spine = [
            ("html/page-3.html", 3),
            ("html/page-1.html", 1),
            ("html/cover.html", "cover"),
            ("html/page-2.html", 2),
            ("html/theend.html", "theend"),
        ]
        make_epub(src, spine, with_nav=True)

        expected_dst = get_unique_dst(src)
        expected_tmp = expected_dst + ".tmp"

        with pytest.raises(RuntimeError, match="回读校验失败"):
            core_fix_one(src)

        assert not os.path.exists(expected_dst)
        assert not os.path.exists(expected_tmp)
        # 额外确保目录下无残留 .tmp
        for name in os.listdir(td):
            assert not name.endswith(".tmp")
        assert os.path.exists(src)


def test_get_unique_dst():
    with tempfile.TemporaryDirectory() as td:
        a = os.path.join(td, "a.epub")
        open(a, "wb").close()
        fixed = os.path.join(td, "a_修正版.epub")
        open(fixed, "wb").close()

        result = get_unique_dst(a)
        assert result == os.path.join(td, "a_修正版 (1).epub")

        # 仅存在原文件时应返回 _修正版
        with tempfile.TemporaryDirectory() as td2:
            b = os.path.join(td2, "b.epub")
            open(b, "wb").close()
            r2 = get_unique_dst(b)
            assert r2 == os.path.join(td2, "b_修正版.epub")

        # 已存在 (1) 时递增到 (2)
        open(result, "wb").close()
        result2 = get_unique_dst(a)
        assert result2 == os.path.join(td, "a_修正版 (2).epub")


def test_core_import_shim():
    from src.core import fix_one as core_fix
    import src.kmoe_fix as shim

    assert hasattr(shim, "fix_one")
    assert hasattr(shim, "get_unique_dst")
    assert shim.fix_one is core_fix
    assert shim.get_unique_dst is get_unique_dst

    from src.core import TITLE_RE, SPINE_RE

    assert hasattr(shim, "TITLE_RE")
    assert shim.TITLE_RE is TITLE_RE
    assert shim.SPINE_RE is SPINE_RE
