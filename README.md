# KmoeFix — Kmoe EPUB 顺序与脏标签一键修复 / One-Click Fix for Kmoe EPUB Ordering & Dirty Tags

> 修复 Kmoe 导出 EPUB 乱序与脏标签的一键工具 — 按 `第X话` 重排章节、重命名资源、清除冗余标签，输出符合规范的 `*_修正版.epub`。

[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Python >=3.10](https://img.shields.io/badge/python-%3E%3D3.10-3776AB.svg)](https://www.python.org/downloads/)
[![Platform: Windows](https://img.shields.io/badge/platform-Windows-0078D6.svg)](https://github.com/KmoeFix/KmoeFix/releases)
[![Version: v1.0.0](https://img.shields.io/badge/version-v1.0.0-blue.svg)](https://github.com/KmoeFix/KmoeFix/releases/tag/v1.0.0)

[English](#) | 中文

---

## 目录

- [是什么 / 为什么需要](#是什么--为什么需要)
- [功能特性](#功能特性)
- [效果对比](#效果对比)
- [安装](#安装)
- [用法](#用法)
- [构建](#构建)
- [项目结构](#项目结构)
- [常见问题 FAQ](#常见问题-faq)
- [贡献](#贡献)
- [许可证](#许可证)
- [致谢](#致谢)

---

## 是什么 / 为什么需要

**KmoeFix v1.0.0** 是一款面向 Kmoe 漫画 EPUB 的离线修复工具。

Kmoe 导出的 EPUB 常见两类问题：

1. **章节乱序** — `vol.opf` 中 `<spine>` 的 `<itemref>` 顺序与实际话数不一致，`html/page-*.html` 与 `image/*.jpg` 文件名也未按话数对齐，导致阅读器（尤其按 spine 顺序渲染的阅读器）出现“第 3 话在第 1 话前面”等错乱。
2. **脏标签残留** — HTML 中残留 `kmoetag` / `kimageraw` / `raw` 等自定义属性，影响校验、增加体积、部分阅读器解析异常；`mimetype` 未按 EPUB 规范以 `ZIP_STORED` 置于 ZIP 首条，部分严格校验器直接报错。

KmoeFix 的做法是**离线重建 ZIP**：解析每个 HTML 的 `<title>第X话</title>` 提取话数，按话数重排 spine、重命名 `html/` 与 `image/` 资源、同步重写 `vol.opf` / `xml/vol.nav` 引用、剥离脏属性，并以规范的 ZIP 结构回写，最后**回读校验**确保页序连续才落盘。

> 适用对象：从 Kmoe 批量导出后需要本地整理、再用 NeeView / 其它 EPUB 阅读器阅读的用户。

---

## 功能特性

- **按话数智能重排** — 正则 `TITLE_RE = <title>\s*第\s*(\d+)\s*话</title>` 主匹配 + `TITLE_FALLBACK` 兜底，`cover.html` 固定为 0、`theend.html` 自动置尾，缺失话数也会按实际最大值补齐宽度。
- **资源零填充重命名** — 统一重命名为 `html/page-001.html` / `image/001.jpg`（宽度 `max(3, len(str(max_page)))`），`cover.html` / `theend` 保持特殊命名，避免排序歧义。
- **脏标签一键清除** — 正则剥离 `kmoetag="..."` 与 `kimageraw="..."` / `raw="..."`，同步修正 HTML 内 `src="../image/..."` 引用。
- **EPUB 规范重打包** — `mimetype` 始终为 ZIP 第一条且 `ZIP_STORED` 不压缩，其余条目 `ZIP_DEFLATED`；同步重写 `vol.opf` 的 `<manifest>` 与 `xml/vol.nav` 的 `src`。
- **回读 spine 校验 + 防覆盖** — 写后立即回读 `vol.opf` 解析 spine 页码，校验 `nums == [1..max]`，失败则删除临时文件并抛出 `回读校验失败`；输出经 `get_unique_dst()` 自动生成 `*_修正版.epub` / `*_修正版 (1).epub`，永不覆盖原文件。
- **GUI 拖拽批量 + CLI 批量 + NeeView 联动** — Tkinter + `tkinterdnd2` 拖拽 `ZIP/EPUB/CBZ` 批量处理，后台 `threading + queue` 不卡界面；支持 CLI 批量与“完成后用 NeeView 打开最后一项”。

---

## 效果对比

以一个 3 话样本为例，修复前后 `vol.opf` 的关键差异：

**修复前（Kmoe 原始导出，spine 乱序）**

```xml
<!-- vol.opf -->
<manifest>
  <item id="p3" href="html/page-003.html" media-type="application/xhtml+xml" />
  <item id="p1" href="html/page-001.html" media-type="application/xhtml+xml" />
  <item id="p2" href="html/page-002.html" media-type="application/xhtml+xml" />
  <item id="img3" href="image/003.jpg" media-type="image/jpeg" />
  <item id="img1" href="image/001.jpg" media-type="image/jpeg" />
</manifest>
<spine toc="ncx">
  <itemref idref="p3" />
  <itemref idref="p1" />
  <itemref idref="p2" />
</spine>
```

```html
<!-- html/page-003.html 片段 -->
<title>第3话</title>
<img src="../image/003.jpg" kmoetag="xxx" kimageraw="xxx" />
```

ZIP 结构：`mimetype` 非首条或被压缩，部分阅读器报 `mimetype must be first and uncompressed`。

**修复后（KmoeFix 输出 `*_修正版.epub`）**

```xml
<!-- vol.opf -->
<manifest>
  <item id="p1" href="html/page-001.html" media-type="application/xhtml+xml" />
  <item id="p2" href="html/page-002.html" media-type="application/xhtml+xml" />
  <item id="p3" href="html/page-003.html" media-type="application/xhtml+xml" />
  <item id="img1" href="image/001.jpg" media-type="image/jpeg" />
  <item id="img2" href="image/002.jpg" media-type="image/jpeg" />
  <item id="img3" href="image/003.jpg" media-type="image/jpeg" />
</manifest>
<spine toc="ncx">
  <itemref idref="p1" />
  <itemref idref="p2" />
  <itemref idref="p3" />
</spine>
```

```html
<!-- html/page-001.html 片段 -->
<title>第1话</title>
<img src="../image/001.jpg" />
```

ZIP 结构（`zipinfo`）：

```text
  mimetype              ZIP_STORED   (首条，未压缩)
  vol.opf               ZIP_DEFLATED
  xml/vol.nav           ZIP_DEFLATED
  html/page-001.html    ZIP_DEFLATED
  html/page-002.html    ZIP_DEFLATED
  image/001.jpg         ZIP_DEFLATED
  ...
```

> 校验逻辑：回读 `vol.opf` → 提取 spine 对应 HTML 的 `<title>` 页码 → 断言 `nums == list(range(1, max+1))`，否则回滚。

---

## 安装

### 方式 A — 直接下载可用版（推荐普通用户）

1. 前往 [Releases](https://github.com/KmoeFix/KmoeFix/releases) 下载 `KmoeFix.exe`（约 12 MB，PyInstaller 单文件，Windows x64）。
2. 双击运行即可，无需安装 Python。

> 提示：首次运行若被 Windows SmartScreen 拦截，点击“更多信息 → 仍要运行”。

### 方式 B — 源码运行（开发者）

要求：**Python 3.10+**，Windows 10/11 推荐。

```bash
# 1. 克隆仓库
git clone https://github.com/KmoeFix/KmoeFix.git
cd KmoeFix

# 2. （可选）创建虚拟环境
python -m venv .venv
# Windows PowerShell
.venv\Scripts\Activate.ps1
# Windows CMD
.venv\Scripts\activate.bat

# 3. 安装依赖
pip install -r requirements.txt
# 或
pip install -e .

# 4. 启动 GUI
python -m src.kmoe_fix
# 安装后也可用命令
kmoefix
```

依赖仅一项（见 `requirements.txt` / `pyproject.toml`）：

```text
tkinterdnd2>=0.1.3
```

其余均为标准库：`zipfile / re / os / sys / json / threading / queue / subprocess / tkinter`。

---

## 用法

### GUI — 拖拽批量（最常用）

1. 运行 `KmoeFix.exe` 或 `python -m src.kmoe_fix` 打开主窗口（720×560）。
2. **拖拽** `*.zip` / `*.epub` / `*.cbz` 文件或整个文件夹到列表；或点击“添加文件…”选择。
3. 可选勾选“完成后用 NeeView 打开（仅打开最后成功项）”并在下方填入 `NeeView.exe` 路径（支持“浏览…”）。
4. 点击 **开始处理**，日志区实时显示 `▶ 处理 / ✔ 完成 / ✘ 失败`，进度条滚动；处理在后台线程执行，界面不卡死。
5. 输出位于原文件同目录，命名 `原名_修正版.epub`（已存在则 `原名_修正版 (1).epub`），原文件不变。

```
输入:  D:\Manga\某漫画.epub
输出:  D:\Manga\某漫画_修正版.epub
```

配置会持久化到 `kmoe_fix_config.json`（`src/` 或 exe 同级，字段 `neeview_path` / `open_with_neeview`，见 `src/config.py`）。

### CLI — 批量处理

```bash
# 单文件
python -m src.kmoe_fix "D:\Manga\某漫画.epub"

# 多文件批量
python -m src.kmoe_fix a.epub b.epub "C:\Manga\*.epub"

# 通配（PowerShell）
python -m src.kmoe_fix (Get-ChildItem D:\Manga\*.epub | ForEach-Object FullName)
```

输出：

```text
[OK] D:\Manga\某漫画.epub -> D:\Manga\某漫画_修正版.epub
[FAIL] D:\Manga\broken.epub: 未找到 vol.opf
```

> Windows GBK 控制台已做 `_safe_print` 容错，不会因 `✔/✘` 乱码崩溃。

### 配置 NeeView 联动

- GUI 中勾选后，处理完成会自动 `subprocess.Popen([neeview_path, last_ok])` 打开最后成功的一项。
- 路径不存在时会弹窗确认是否仍继续（仅完成处理，不启动）。
- CLI 模式不自动启动 NeeView（仅 GUI 支持）。
- 手动编辑配置：打开 `kmoe_fix_config.json`：

```json
{
  "neeview_path": "C:\\Tools\\NeeView\\NeeView.exe",
  "open_with_neeview": false
}
```

---

## 构建

从源码本地构建与 Release 一致的 `dist/KmoeFix.exe`：

```bash
pip install pyinstaller==6.12 tkinterdnd2
pyinstaller KmoeFix.spec
# 或
pyinstaller --clean --noconfirm KmoeFix.spec
```

产物：`dist/KmoeFix.exe`（`onefile + windowed`，`console=False`，`hiddenimports=tkinterdnd2`）。

`KmoeFix.spec` 要点（见文件头部注释）：

```python
# 打包入口 src/kmoe_fix.py，图标自动取 assets/icon.ico（若存在）
a = Analysis(["src/kmoe_fix.py"], hiddenimports=["tkinterdnd2"], ...)
exe = EXE(pyz, a.scripts, a.binaries, a.datas, name="KmoeFix",
          console=False, upx=True, icon=icon)
```

`pyproject.toml` 已配置 `kmoefix = "src.kmoe_fix:run_gui"`，`pip install -e .` 后可直接 `kmoefix` 启动。

---

## 项目结构

```text
KmoeFix/                         # 仓库根，Public
├── src/
│   ├── __init__.py              # 包标识
│   ├── config.py                # 配置：APP_NAME/VERSION/OUT_SUFFIX，
│   │                            #        get_config_path/load_config/save_config
│   ├── core.py                  # 核心：fix_one(src, dst, log) / get_unique_dst
│   │                            #        TITLE_RE / MANIFEST_RE / SPINE_RE 等正则
│   ├── gui.py                   # 界面：run_gui() — Tkinter + tkinterdnd2
│   └── kmoe_fix.py              # 入口 shim：GUI 与 CLI 分发，兼容 from src.kmoe_fix import *
├── tests/
│   └── test_fix_one.py          # 单测：sorted/shuffled/get_unique_dst/shim
├── assets/
│   └── icon.ico                 # （可选）EXE 图标
├── KmoeFix.spec                 # PyInstaller 6.12 单文件打包配置
├── pyproject.toml               # 项目元数据 + ruff/black 配置
├── requirements.txt             # tkinterdnd2>=0.1.3
├── LICENSE                      # MIT
├── README.md                    # 本文
└── RELEASE_NOTES.md             # 发布说明
```

核心调用链：

```python
from src.core import fix_one

# 最简调用
out = fix_one("input.epub")              # 自动生成 *_修正版.epub
out = fix_one("input.epub", dst="out.epub", log=print)
```

---

## 常见问题 FAQ

### Q1: 提示 `回读校验失败 spine页码 [3,1,2]... 期望 [1,2,3]` 怎么办？

这是**回读校验**触发的保护：输入本身的 `<title>` 页码不连续或标题缺失导致重排后仍非 `1..N` 连续。KmoeFix 会**删除临时文件并回滚**，不会污染输出。请检查原 EPUB 是否为 Kmoe 导出、是否混入非漫画 HTML（如广告页无 `第X话` 标题），或用解压工具查看 `html/*.html` 的 `<title>` 是否缺失。确认无误后可尝试重新导出再修复。

### Q2: 被杀毒软件 / SmartScreen 报毒或拦截？

PyInstaller 的 `onefile` 单文件 EXE 因自解压行为易被启发式误报，属常见误报。可：① 在 Release 页核对 SHA256；② 将 `KmoeFix.exe` 加入白名单；③ 自行源码构建 `pyinstaller KmoeFix.spec` 得到本机可信 EXE；④ 改用 `pyinstaller --onedir` 目录模式分发（需自行修改 spec 的 `console`/`onefile` 配置）。

### Q3: 中文路径 / 中文文件名会乱码或失败吗？

不会。项目全程 **UTF-8**（`src/*.py` 含 `# -*- coding: utf-8 -*-`，读写均 `encoding="utf-8"`），`第X话` / `_修正版` 等已做 UTF-8 还原。但需注意：① 控制台为 GBK 时日志已做 `_safe_print` 容错；② 勿用 GBK 编辑器保存源码，否则中文会再次乱码；③ 输出路径经 `get_unique_dst` 处理，中文与空格均支持。

### Q4: 拖拽无反应 / 提示 `未找到 vol.opf` / `spine 为空`？

- **拖拽无反应**：`tkinterdnd2` 未安装或版本过低，执行 `pip install -U tkinterdnd2` 后重试；文件夹拖拽仅扫描一层 `*.zip/*.epub/*.cbz`，深层嵌套请直接拖文件。
- **未找到 vol.opf**：输入非 Kmoe 结构 EPUB（Kmoe 固定含 `vol.opf` 或 `*/vol.opf`），请确认文件来源。
- **spine 为空**：`vol.opf` 中 `<spine>` 无 `<itemref>`，属文件损坏或非标准 EPUB，无法修复。

更多问题请提 [Issue](https://github.com/KmoeFix/KmoeFix/issues)，附上脱敏后的 `vol.opf` 片段与日志即可。

---

## 贡献

欢迎 Issue / PR！

```bash
# 开发流程
git clone https://github.com/KmoeFix/KmoeFix.git
cd KmoeFix
pip install -r requirements.txt

# 代码风格：ruff + black，行宽 100，py310
# 已在 pyproject.toml 配置 [tool.ruff] / [tool.black]
pip install ruff black
ruff check src/
black --check src/

# 单测（pytest 可选，仓库提供最小 stub 兼容）
python -m pytest tests/ -v
# 或直接
python -m py_compile src/*.py
```

提交前请确保：

- 全程 UTF-8，勿引入 GBK；
- 不提交含版权的 EPUB 原文件，测试用 `tests/` 内脚本生成的最小 ZIP；
- `DEFAULT_NEEVIEW` 保持 `""`，不写死个人路径；
- 涉及 `fix_one` 行为变更需同步更新“效果对比”与 FAQ。

详见 `CONTRIBUTING.md` / `CODE_OF_CONDUCT.md`（如有）。

---

## 许可证

[MIT License](LICENSE) — Copyright (c) 2026 KmoeFix Contributors

可自由使用、复制、修改、合并、发布、分发、再许可及销售本软件副本，需在副本中保留上述版权与许可声明。软件按“现状”提供，不附任何明示或暗示保证。

---

## 致谢

- Kmoe 漫画工具的原始导出能力
- [NeeView](https://bitbucket.org/orillas/neeview) — 轻量高性能的漫画/图片浏览器
- [tkinterdnd2](https://github.com/pmgagne/tkinterdnd2) — 让 Tkinter 支持文件拖拽
- [PyInstaller](https://pyinstaller.org/) 6.12 — 将 Python 打包为单文件 EXE
- 所有测试 EPUB 脱敏样本的贡献者

---

