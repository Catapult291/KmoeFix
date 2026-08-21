# kmoe-epub-order-fixer

> Kmoe 漫画包顺序修正工具 — 按真实页码重命名，重建 EPUB，使按文件名排序的看图场景也不再乱序。

![Python](https://img.shields.io/badge/Python-3.8%2B-3776AB)
![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)
![License](https://img.shields.io/badge/License-MIT-green)
![No Dependencies](https://img.shields.io/badge/Dependencies-零依赖-brightgreen)

正规 EPUB 阅读器按 `spine` 阅读不会乱；但解压后直接看图、或用 NeeView 等**按文件名排序**的看图器，就会乱序。本工具读取 `vol.opf` 的 `spine` 顺序与每页 `<title>第 N 頁</title>` 的真实页码，把 `html/` 与 `image/` 重命名为有序文件名，并同步改写 `vol.opf` / `xml/vol.nav`，输出 `*_修正版.epub`，默认不依赖 NeeView。

## 痛点

Kmoe 下载的漫画包（EPUB 实为 ZIP）为防扒图，将所有页面与图片重命名为随机文件名：

- `html/page-XXXXXX.html`
- `image/moe-XXXXX.jpg`

真实页码藏在每页的 `<title>第 N 頁</title>` 与 `<img alt="第 N 頁">` 中，正确阅读顺序在 `vol.opf` 的 `<spine>` 里。按文件名排序即乱序。

## 原理

1. 解析 `vol.opf`：`manifest` 的 `id -> html` 映射 + `spine` 的 `idref` 顺序
2. 按 `spine` 顺序读取每页 `html`，提取 `<title>第 N 頁</title>` 的真实页码与 `<img src="../image/...">`
3. 无页码的封面/结束页自动补为 `0` / `max+1`
4. 重命名：`cover.html` / `theend.html` 保留，其余 `html/page-{N}.html` 与 `image/{N}.jpg`（零填充宽度 `max(3, len(str(max)))`）
5. 同步改写 `vol.opf` 与 `xml/vol.nav` 中的路径引用
6. 重建 ZIP/EPUB，原地输出校验：回读 `spine` 页码需严格 `1..N`

未被任何页面引用的 `image/*` 原样保留，仅在日志中提示。

## 效果对比

| 修正前（随机文件名，按文件名排序即乱序） | 修正后（按真实页码命名，文件名即顺序） |
|---|---|
| `html/page-a3f9c1.html` → 第 27 頁 | `html/page-027.html` → 第 27 頁 |
| `html/page-7b2e00.html` → 第 3 頁 | `html/page-003.html` → 第 3 頁 |
| `image/moe-x9k2p.jpg` | `image/027.jpg` |

输出文件：`原名_修正版.epub`（与原文件同目录，不覆盖原文件）

## 功能特性

- GUI 批量处理：多选 `*.epub / *.zip / *.cbz`，文件列表增删、进度与日志
- CLI 无界面批处理：`--cli` 模式
- 有序重命名：`html/page-001.html` / `image/001.jpg`，零填充对齐
- 自动改写 `vol.opf` / `xml/vol.nav`，重建后自动校验 `spine 1..N`
- 可选“完成后用 NeeView 打开”（路径可配置，默认留空）
- Windows DPI 感知（`PER_MONITOR_V2`，在 `import tkinter` 之前生效）与 Tk 主题黑块闪烁修复

## 环境要求

- Python 3.8+（推荐 3.10+）
- 零三方依赖，仅标准库：`tkinter` / `zipfile` / `re` / `json` / `threading` 等
- `tkinter`：Windows 官方 Python 已自带；Linux 需 `sudo apt install python3-tk`

## 快速开始

### 方式一：GUI（推荐）

```bash
# Windows 双击或命令行
python kmoe_fix_gui.pyw

# macOS / Linux
python3 kmoe_fix_gui.pyw
# 若提示 No module named '_tkinter'，先安装 python3-tk
```

1. 点击「添加文件…」选择一个或多个漫画包（`*.epub / *.zip / *.cbz`）
2. （可选）勾选「完成后用 NeeView 打开」并配置 `NeeView.exe` 路径
3. 点击「开始处理」，日志区查看进度
4. 同目录生成 `*_修正版.epub`，用任意看图器按文件名排序即为正确顺序

> Windows 上若双击 `.pyw` 无控制台、看不到报错，可用 `python kmoe_fix_gui.pyw` 启动以便查看日志。

### 方式二：命令行（无 GUI）

```bash
python kmoe_fix_gui.pyw --cli "漫画A.epub" "漫画B.zip" "漫画C.cbz"
# 输出：
# [OK] .../漫画A_修正版.epub
#      spine=32 页, 页码严格 1..32
```

- 支持一次传入多个文件
- 退出码：全部成功 `0`，任一失败 `1`
- 失败时会打印 `[FAIL] <原文件> -> <原因>`

## 选项说明

### NeeView 打开

- GUI 中「完成后用 NeeView 打开」默认关闭
- 路径保存在程序同目录的 `kmoe_fix_config.json`（`{"neeview_path": "..."}`），该文件已加入 `.gitignore`，不会提交
- 未勾选或路径不存在时，仅跳过打开，不影响修正流程
- 批量处理时仅自动打开最后一个成功输出的文件

## 常见问题

**Q: 修正后原文件会被覆盖吗？**
不会。输出为 `原名_修正版.epub`，原文件保留。

**Q: 支持哪些输入格式？**
本质是 ZIP，扩展名 `*.epub / *.zip / *.cbz` 均可，只要内部含 `vol.opf` 且页面含 `第 N 頁`。非 Kmoe 包会报错：`未找到 vol.opf` / `未找到「第 N 頁」页码`。

**Q: 为什么有些包 `image/` 里有未引用的图片？**
可能是封面缩略图等未被 `spine` 页面引用的资源，工具会原样保留，并在日志中列出 `未引用图片(已保留原样)`。

**Q: 修正后校验失败怎么办？**
工具会在重建后回读 `spine` 校验页码是否严格 `1..N`，失败会抛出 `输出验证失败` 并保留错误信息，请提 Issue 并附上脱敏后的日志（勿上传原漫画包）。

**Q: Linux/macOS 上 GUI 起不来？**
确认已安装 `python3-tk`，或直接使用 `--cli` 模式（无需 GUI）。

## 项目结构

```
kmoe-epub-order-fixer/
├─ kmoe_fix_gui.pyw      # 主程序（GUI + CLI）
├─ .gitignore            # Python / 本工具产物忽略规则
├─ LICENSE               # MIT
└─ README.md
```

## 免责声明

- 本工具仅用于修正**已合法获取**的个人藏品的阅读顺序与文件命名，不提供任何内容获取、解密或分发功能
- 请遵守当地法律法规与平台服务条款，尊重版权
- 输出文件仅供个人学习与整理使用

## 许可证

[MIT](LICENSE) © 2026 Catapult291
