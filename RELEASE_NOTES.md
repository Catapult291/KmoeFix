# KmoeFix v1.0.0 — 首个开源版本

> KmoeFix 首个开源版本正式发布！本版本为从 `KmoeFix.exe` 逆向重建的开源重制版，基于 PyInstaller 6.12 + Python 3.13 原始产物完整还原，已以 MIT 许可证开源。

---

## 简介

KmoeFix 是一款面向 Kmoe 漫画 EPUB 的离线一键修复工具，解决两类常见问题：

1. **章节乱序** — `vol.opf` 的 `<spine>` 顺序与 `第X话` 不一致；
2. **脏标签残留** — `kmoetag` / `kimageraw` / `raw` 等冗余属性，`mimetype` 未按规范置首条。

做法：解析 `<title>第X话</title>` → 重排 spine → 重命名 `html/page-001.html` / `image/001.jpg` → 重写 `vol.opf` / `xml/vol.nav` → 清除脏标签 → 规范重打包 → 回读校验页序连续才落盘，输出 `*_修正版.epub`。

本项目由闭源 `KmoeFix.exe` (PE32+ x64 GUI, PyInstaller, 约471行) 逆向重构而来，已去除硬编码 `本机 NeeView.exe 路径`，修复 UTF-8 乱码与 Windows 文件占用 bug。

**Repo:** Catapult291/KmoeFix | **License:** MIT | **Requires:** Python >=3.10 / Windows 10/11

---

## 功能亮点

- 按话数智能重排（TITLE_RE 主匹配 + FALLBACK 兜底，cover=0 / theend 置尾）
- 零填充重命名 `page-001` / `001.jpg`（width = max(3, len(str(max_page)))）
- 脏标签一键清除 + 同步修正 `src` 引用
- EPUB 规范重打包：`mimetype` ZIP_STORED 首条，其余 ZIP_DEFLATED
- 回读 spine 校验 + `get_unique_dst` 防覆盖（`*_修正版 (1).epub`）
- GUI 拖拽批量 + CLI 批量 + NeeView 联动（threading+queue 不卡界面）

---

## 安装

**方式 A — 直接下载（推荐）**：本页 Assets 下载 `KmoeFix.exe`（11MB），双击即用。

**方式 B — 源码运行**：
```bash
git clone https://github.com/Catapult291/KmoeFix.git
cd KmoeFix
pip install -r requirements.txt
python -m src.kmoe_fix
```

---

## 用法

**GUI：** 运行 `KmoeFix.exe` → 拖拽 `*.zip/*.epub/*.cbz` → 可选 NeeView 联动 → 开始处理 → 同目录生成 `*_修正版.epub`

**CLI：**
```bash
python -m src.kmoe_fix book.epub another.epub
# [OK] book.epub -> book_修正版.epub
```

---

## 构建

```bash
pip install pyinstaller==6.12 tkinterdnd2
pyinstaller KmoeFix.spec --clean --noconfirm
# 产物: dist/KmoeFix.exe (onefile + windowed)
```

---

## 技术栈

Python 3.13.2 / Tkinter + tkinterdnd2 / PyInstaller 6.12 / ruff+black / zipfile+re

项目结构：`src/config.py` + `src/core.py(fix_one)` + `src/gui.py(run_gui)` + `src/kmoe_fix.py` shim + `tests/test_fix_one.py` (4用例)

---

## 已知问题

- 杀软/SmartScreen 误报属 PyInstaller onefile 常见误报，可本地自构建或加白
- GBK 控制台日志已做 `_safe_print` 容错
- 回读校验失败属保护性回滚，请检查原 EPUB 是否为 Kmoe 导出

---

## 校验

| 文件 | SHA256 | 大小 |
|---|---|---|
| KmoeFix.exe | 0e000c2800eadda5b3c7d6abe2c23d9ae0a3d54c4127cbebf0f4abd8aa107f43 | 11392486 bytes |

```bash
certutil -hashfile KmoeFix.exe SHA256
Get-FileHash KmoeFix.exe -Algorithm SHA256
```

---

## 许可证

[MIT License](LICENSE) — Copyright (c) 2026 KmoeFix Contributors
