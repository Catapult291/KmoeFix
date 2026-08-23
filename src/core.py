# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import re
import zipfile
from collections.abc import Callable

from src.config import OUT_SUFFIX

TITLE_RE: re.Pattern[str] = re.compile(r"<title>\s*第\s*(\d+)\s*话</title>")
TITLE_FALLBACK: re.Pattern[str] = re.compile(r"<title[^>]*>.*?(\d+).*?</title>", re.S)
MANIFEST_RE: re.Pattern[str] = re.compile(r'<item[^>]*\sid="([^"]+)"[^>]*href="([^"]+)"[^>]*>', re.S)
MANIFEST_RE2: re.Pattern[str] = re.compile(r'<item[^>]*href="([^"]+)"[^>]*id="([^"]+)"[^>]*>', re.S)
SPINE_RE: re.Pattern[str] = re.compile(r'<itemref[^>]*idref="([^"]+)"[^>]*>', re.S)
IMG_SRC_RE: re.Pattern[str] = re.compile(r'<img[^>]+src="\.\./(image/[^"]+)"', re.S)
KMOETAG_RE: re.Pattern[str] = re.compile(r'\s*kmoetag\s*=\s*"[^"]*"')
RAW_RE: re.Pattern[str] = re.compile(r'\s*kimageraw\s*=\s*"[^"]*"|\s*raw\s*=\s*"[^"]*"')


def get_unique_dst(src: str) -> str:
    base: str
    ext: str
    base, ext = os.path.splitext(src)
    dst: str = base + OUT_SUFFIX + ext
    if not os.path.exists(dst):
        return dst
    i: int = 1
    while True:
        cand: str = f"{base}{OUT_SUFFIX} ({i}){ext}"
        if not os.path.exists(cand):
            return cand
        i += 1


def fix_one(src: str, dst: str | None = None, log: Callable[[str], None] | None = None) -> str:
    def _log(s: str) -> None:
        if log:
            log(s)

    if dst is None:
        dst = get_unique_dst(src)
    elif os.path.exists(dst):
        if OUT_SUFFIX in dst:
            dst = get_unique_dst(dst.replace(OUT_SUFFIX, "").rsplit(".", 1)[0] + os.path.splitext(dst)[1])
        else:
            dst = get_unique_dst(src)

    with zipfile.ZipFile(src, "r") as zin:
        namelist: list[str] = zin.namelist()

        if "vol.opf" in namelist:
            opf_name: str | None = "vol.opf"
        else:
            opf_name = next((n for n in namelist if n.endswith("vol.opf")), None)
        if not opf_name:
            raise RuntimeError("未找到 vol.opf")

        opf_raw: str = zin.read(opf_name).decode("utf-8", errors="replace")

        manifest: dict[str, str] = dict(MANIFEST_RE.findall(opf_raw))
        if not manifest:
            for href, iid in MANIFEST_RE2.findall(opf_raw):
                manifest[iid] = href

        spine: list[str] = SPINE_RE.findall(opf_raw)
        if not spine:
            raise RuntimeError("spine 为空")

        entries: list[dict[str, object]] = []
        raw_map: dict[str, str] = {}
        title_nums: list[int] = []

        for ref in spine:
            href: str | None = manifest.get(ref)
            if not href:
                continue
            if not href.endswith(".html"):
                continue
            try:
                html_bytes: bytes = zin.read(href)
            except KeyError:
                continue
            html: str = html_bytes.decode("utf-8", errors="replace")
            m: re.Match[str] | None = TITLE_RE.search(html)
            if not m:
                m = TITLE_FALLBACK.search(html)
                if m and ("THE END" in html or "Book Cover" in html):
                    m = None
            if m:
                num: int | None = int(m.group(1))
            else:
                num = None
            if href.endswith("cover.html"):
                num = 0
            elif href.endswith("theend.html"):
                num = None

            img_m: re.Match[str] | None = IMG_SRC_RE.search(html)
            img_src: str | None = img_m.group(1) if img_m else None

            entries.append({"ref": ref, "href": href, "num": num, "img": img_src, "html": html})

        nums: list[int] = [e["num"] for e in entries if e["num"] not in (None, 0)]  # type: ignore[misc]
        max_n: int = max(nums) if nums else 0

        for e in entries:
            if str(e["href"]).endswith("theend.html"):
                e["num"] = max_n + 1

        nums_all: list[int] = [  # type: ignore[misc]
            e["num"]
            for e in entries
            if e["num"] is not None
            and not str(e["href"]).endswith("cover.html")
            and not str(e["href"]).endswith("theend.html")
        ]
        max_page: int = max(nums_all) if nums_all else max_n
        width: int = max(3, len(str(max_page))) if max_page else 3

        html_map: dict[str, str] = {}
        img_map: dict[str, str] = {}

        for e in entries:
            old_href: str = str(e["href"])
            num = e["num"]
            if old_href.endswith("cover.html"):
                new_href: str = "html/cover.html"
                if e["img"]:
                    img_map[str(e["img"])] = str(e["img"])
            elif old_href.endswith("theend.html"):
                new_href = "html/theend.html"
                if e["img"]:
                    ext: str = os.path.splitext(str(e["img"]))[1] or ".png"
                    new_img: str = f"image/theend{ext}"
                    img_map[str(e["img"])] = new_img
            else:
                new_href = f"html/page-{int(num):0{width}d}.html"  # type: ignore[arg-type]
                if e["img"]:
                    ext = os.path.splitext(str(e["img"]))[1] or ".jpg"
                    new_img = f"image/{int(num):0{width}d}{ext}"  # type: ignore[arg-type]
                    if str(e["img"]) not in img_map:
                        img_map[str(e["img"])] = new_img
            html_map[old_href] = new_href

        new_opf: str = opf_raw
        for old, new in html_map.items():
            new_opf = new_opf.replace(f'href="{old}"', f'href="{new}"')
        for old_img, new_img in img_map.items():
            if old_img != new_img:
                new_opf = new_opf.replace(f'href="{old_img}"', f'href="{new_img}"')

        new_nav: str | None = None
        if "xml/vol.nav" in namelist:
            nav_name: str | None = "xml/vol.nav"
        else:
            nav_name = next((n for n in namelist if n.endswith("vol.nav")), None)

        if nav_name:
            nav_raw: str = zin.read(nav_name).decode("utf-8", errors="replace")
            new_nav = nav_raw
            for old, new in html_map.items():
                new_nav = new_nav.replace(f'src="../{old}"', f'src="../{new}"')
                new_nav = new_nav.replace(f'src="{old}"', f'src="{new}"')

        exclude: set[str] = set(html_map.keys()) | set(img_map.keys())

        tmp_dst: str = dst + ".tmp"
        with zipfile.ZipFile(tmp_dst, "w", compression=zipfile.ZIP_DEFLATED) as zout:
            if "mimetype" in namelist:
                data: bytes = zin.read("mimetype")
                zi: zipfile.ZipInfo = zipfile.ZipInfo("mimetype")
                zi.compress_type = zipfile.ZIP_STORED
                zout.writestr(zi, data)

            for name in namelist:
                if name == "mimetype":
                    continue
                if name in exclude:
                    continue
                if name == nav_name or name == opf_name:
                    continue
                data = zin.read(name)
                zout.writestr(name, data)

            zout.writestr(opf_name, new_opf.encode("utf-8"))
            if nav_name and new_nav is not None:
                zout.writestr(nav_name, new_nav.encode("utf-8"))

            for e in entries:
                old: str = str(e["href"])
                new: str = html_map[old]
                html = str(e["html"])
                old_img: str | None = str(e["img"]) if e["img"] else None
                if old_img and old_img in img_map:
                    new_img: str = img_map[old_img]
                    html = html.replace(f'src="../{old_img}"', f'src="../{new_img}"')
                    html = html.replace(f'src="{old_img}"', f'src="{new_img}"')
                html = KMOETAG_RE.sub("", html)
                html = RAW_RE.sub("", html)
                zout.writestr(new, html.encode("utf-8"))

            written_images: set[str] = set()
            for old_img, new_img in img_map.items():
                if new_img in written_images:
                    continue
                written_images.add(new_img)
                try:
                    data = zin.read(old_img)
                except KeyError:
                    continue
                zout.writestr(new_img, data)

        with zipfile.ZipFile(tmp_dst, "r") as zcheck:
            opf2: str = zcheck.read(opf_name).decode("utf-8", errors="replace")
            spine2: list[str] = SPINE_RE.findall(opf2)
            manifest2: dict[str, str] = dict(MANIFEST_RE.findall(opf2))
            if not manifest2:
                for href, iid in MANIFEST_RE2.findall(opf2):
                    manifest2[iid] = href
            nums2: list[int] = []
            for ref in spine2:
                href = manifest2.get(ref)
                if not href or not href.endswith(".html"):
                    continue
                if href.endswith("cover.html") or href.endswith("theend.html"):
                    continue
                try:
                    h: str = zcheck.read(href).decode("utf-8", errors="replace")
                except KeyError:
                    continue
                m = TITLE_RE.search(h)
                if not m:
                    m = TITLE_FALLBACK.search(h)
                if not m:
                    continue
                nums2.append(int(m.group(1)))
            if nums2:
                expected: list[int] = list(range(1, max(nums2) + 1))
                if nums2 != expected:
                    need_remove: bool = True
                    need_raise: RuntimeError | None = RuntimeError(f"回读校验失败 spine页码 {nums2[:10]}... 期望 {expected[:10]}")
                else:
                    need_remove = False
                    need_raise = None
            else:
                need_remove = False
                need_raise = None

        if need_remove:
            try:
                os.remove(tmp_dst)
            except Exception:
                pass
            raise need_raise  # type: ignore[misc]

        os.replace(tmp_dst, dst)
        _log("  页序已按页码重排完成（不含旋转处理）")
        return dst
