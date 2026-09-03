#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成一个接近真实 Kmoe 导出结构的 EPUB 样本，供端到端验证。"""
import os, re, sys, zipfile

def build(path, label):
    # 模拟真实目录：OEBPS 风格 + 稀疏/重复 title 的变体
    manifest = []
    spine_refs = []
    html_img = {}
    order = []  # (href, num或None)
    def add_html(iid, href, img, num):
        manifest.append((iid, href, "application/xhtml+xml"))
        spine_refs.append(iid)
        html_img[href] = img
        order.append((href, num))
    add_html("cover", "html/cover.html", "image/cover.jpg", 0)
    for i in [1, 2, 3]:
        add_html(f"p{i}", f"html/page-{i}.html", f"image/pg{i}.jpg", i)
    add_html("end", "html/theend.html", "image/theend.png", None)
    # 额外图片 manifest
    for href, img in html_img.items():
        manifest.append(("img_" + re.sub(r"\W", "", img), img, "image/jpeg" if img.endswith("jpg") else "image/png"))

    opf = '<?xml version="1.0" encoding="utf-8"?>\n<package xmlns="http://www.idpf.org/2007/opf" version="3.0">\n<manifest>\n'
    for iid, href, mt in manifest:
        opf += f'<item id="{iid}" href="{href}" media-type="{mt}"/>\n'
    opf += "</manifest>\n<spine>\n"
    for r in spine_refs:
        opf += f'<itemref idref="{r}"/>\n'
    opf += "</spine>\n</package>"

    nav = '<?xml version="1.0" encoding="utf-8"?>\n<nav xmlns:epub="http://www.idpf.org/2007/ops">\n<ol>\n'
    for href, _ in order:
        nav += f'<li><a epub:href="../{href}">{href}</a></li>\n'
    nav += "</ol></nav>"

    with zipfile.ZipFile(path, "w") as z:
        zi = zipfile.ZipInfo("mimetype")
        zi.compress_type = zipfile.ZIP_STORED
        z.writestr(zi, "application/epub+zip")
        z.writestr("META-INF/container.xml", '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="vol.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
        z.writestr("vol.opf", opf.encode("utf-8"))
        z.writestr("xml/vol.nav", nav.encode("utf-8"))
        for href, num in order:
            if num == 0:
                t = "Book Cover"
            elif num is None:
                t = "THE END"
            else:
                t = f"第{num}话"
            img = html_img[href]
            html = (f'<?xml version="1.0" encoding="utf-8"?>\n'
                    f'<html xmlns="http://www.w3.org/1999/xhtml"><head><title>{t}</title></head>'
                    f'<body><div><img src="../{img}" kmoetag="a" kimageraw="b"/></div>'
                    f'<img src="{img}" raw="c"/></body></html>')
            z.writestr(href, html.encode("utf-8"))
        for img in set(html_img.values()):
            z.writestr(img, b"\xff\xd8\xff\xe0 fake image data " + img.encode("utf-8") + b"\xff\xd9")

    return path

if __name__ == "__main__":
    out = sys.argv[1]
    build(out, "样本")
    print(out)
