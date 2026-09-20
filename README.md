# KmoeFix

把漫画 EPUB 的页面与图片按**真实页码**重命名、重排章节顺序、清除脏标签，输出符合 EPUB 规范的 `*_修正版.epub`。面向按文件名排序看图的场景（NeeView、解压看图），让文件名顺序等于阅读顺序。

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue)](LICENSE)
![Rust](https://img.shields.io/badge/Rust-2021-orange)

## 解决什么问题

漫画站点导出的 EPUB 为防扒图，会把页面与图片改成随机文件名（`html/page-XXXXXX.html`、`image/moe-XXXXX.jpg`），真实页码只藏在每页 `<title>第N话</title>` 里。按 `<spine>` 阅读的阅读器不受影响，但按文件名排序的看图器读出来就是乱的。

KmoeFix 按 spine 顺序解析每页话数，把 html / image 重命名成 `page-001.html` / `001.jpg`，同步改写 `vol.opf` 与 `xml/vol.nav`，输入本身 spine 乱序时一并重排。写盘前回读校验页序必须连续 `1..N`，不通过就回滚。

## 下载与运行

从 [Releases](https://github.com/Catapult291/KmoeFix/releases) 下载 `KmoeFix.exe`：Windows x64 单文件，不依赖 VC++ 运行库，双击即用。图形界面与命令行是同一个 exe。

```bash
KmoeFix.exe                              # 不带参数 = 图形界面
KmoeFix.exe "D:\Manga\某漫画.epub"        # 命令行单文件
KmoeFix.exe a.epub b.epub                # 命令行批量
KmoeFix.exe --gui "D:\Manga\某漫画.epub"  # 图形界面 + 预装文件
KmoeFix.exe --help                       # 全部用法
```

Windows 下按 GUI 子系统编译：双击不弹控制台；在终端里运行会接管当前终端输出；把文件拖到 exe 图标上时自开一个控制台显示日志。

## 命令行参数

| 参数 | 说明 |
|---|---|
| （无参数） | 打开图形界面 |
| `--gui [文件…]` | 打开图形界面并预装文件 |
| `--rotate-cover=90\|180\|270` | 判出侧放页后按指定方向回正（覆盖自动判定的方向） |
| `--rotate-cover=off`、`--no-rotate-cover` | 不做任何图片处理，图片逐字节原样写出 |
| `--version` / `-V` | 版本号 |
| `--help` / `-h` | 用法 |

## 输出

- 写在源文件同目录，命名为 `*_修正版.epub`；同名已存在时递增为 `*_修正版 (1).epub`，**绝不覆盖原文件**
- 全部成功退出码 0；任一文件失败退出码 1，单个文件失败不影响后续文件
- GBK 控制台下输出自动降级为替换符，不中断处理

## 专项能力

三项能力都默认开启，细节见对应文档。

| 能力 | 说明 | 文档 |
|---|---|---|
| 页序修复 | 文件名乱序、spine 也乱序的输入都能修到连续 `1..N` | [docs/architecture.md](docs/architecture.md) |
| 侧放页回正 | 横躺存放的跨页按包内参照图判定方向后转正；判不出方向时不动 | [docs/rotation.md](docs/rotation.md) |
| 站点卡片页移位 | 站点插在正文中间的卡片页移出正文编号、排到卷末 | [docs/site-cards.md](docs/site-cards.md) |

图形界面的布局、交互与配置文件字段见 [docs/gui.md](docs/gui.md)。

## 从源码构建

要求 Rust 1.70+。依赖 `zip`、`regex`、`image`，图形界面另需 `eframe` / `rfd`。

```bash
cargo build --release
# 产物 target/release/KmoeFix.exe

RUSTFLAGS="-C target-feature=+crt-static" cargo build --release
# 发布用静态链接构建：不依赖 VC++ 运行库
```

构建时 `build.rs` 把 `assets/kmoefix.ico` 与版本信息编进 exe，这一步用的是 MSVC 工具链自带 Windows SDK 的 `rc.exe`。图标资源已入库，只有改图标才需要 `python tools/make_icon.py` 重新生成（该脚本依赖 Pillow）。

## 测试

```bash
cargo test    # 27 个用例：核心 20 + 命令行分发 6 + GUI 配置映射 1
```

覆盖乱序修复（含 spine 乱序、缺 cover/theend、无 `xml/vol.nav`、已存在同名前缀输出）、侧放页回正的七类场景、站点卡片页的三类场景、命令行分发与 GUI 配置映射。

## 已知边界

- 只面向站点导出的 epub 布局：opf 名为 `vol.opf`（标准 `content.opf` 不支持）、页面为 `*.html`（`*.xhtml` 不支持）、nav 为 `xml/vol.nav`
- 侧放页回正只认包内参照图 `<图名>-RAWIMAGE.<ext>`，参照图缺失或相似度不足时不旋转
- 站点卡片页判定依赖包内 theend 卡作参照，且只认「信息卡」版面
- `xml/vol.nav` 的条目顺序不重排；标准阅读器按 spine 读，实际阅读不受影响
- 只在 Windows x64 构建与验证，macOS / Linux 未测试

## Roadmap

- [ ] `--check` 只读预检：不写文件，只报告文件名与页序状态
- [ ] 通用 EPUB 支持：经 `META-INF/container.xml` 定位 opf、支持 `*.xhtml`、改写 nav 的 `epub:href`

## 许可证

[GPL-3.0-or-later](LICENSE) — GNU 通用公共许可证第 3 版或更高版本，Copyright (C) 2026 Catapult291
