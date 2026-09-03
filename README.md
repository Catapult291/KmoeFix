# kmoefix — 漫画 EPUB 文件名/页序修复工具（Rust）

把 Kmoe 这类站点下载的漫画 EPUB 按**真实页码**重命名，输出符合 EPUB 规范的 `*_修正版.epub`，让按文件名排序的看图场景（NeeView、解压看图）不再乱序。

[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)
![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)

## 解决什么问题

### 初衷：文件名乱序

Kmoe 下载的漫画 EPUB（实为 ZIP）为防扒图，把所有页面与图片重命名为随机文件名（`html/page-XXXXXX.html`、`image/moe-XXXXX.jpg`）。真实页码藏在每页 `<title>第N话</title>` 里，正确阅读顺序在 `vol.opf` 的 `<spine>` 里。

正规 EPUB 阅读器按 spine 读不会乱；但 **NeeView 这类看图器按文件名排序**，读出来就是乱的。本工具按 spine 顺序读每页标题的真实话数，把 `html/`、`image/` 重命名为有序文件名（`page-001.html` / `001.jpg`），并同步改写 `vol.opf` / `xml/vol.nav` —— 修复后文件名排序 = 话数顺序。

### 能力扩展：spine 也乱序的输入

开发初衷场景里 spine 顺序是对的；本工具额外支持 **spine 也乱序**的输入：按话数把 `<spine>` 一并重排（cover 置首 → 页面升序 → theend 殿后），产物同样通过 `1..N` 连续校验。对初衷场景（spine 已有序）这是恒等操作，零影响。

> 说明：早期 README 曾写"解决 spine 乱序"，那并非开发初衷的准确表述。准确能力是——**修复文件名乱序（核心），顺带支持 spine 乱序输入（扩展）**。

### 当前适用边界

本工具面向 Kmoe 系导出的 EPUB 布局：

- opf 定位：查找 `vol.opf`（标准 EPUB 的 `content.opf` 暂不支持）
- 页面文件：仅处理 `*.html`（`*.xhtml` 暂不支持，不会识别为封面/结尾页）
- nav：查找 `xml/vol.nav`

超出这些假设的通用 EPUB 支持见 [Roadmap](#roadmap)。所以"任意 EPUB 修复工具"还不是它的定位。

## 工作原理

1. 读取 zip 目录，定位 `vol.opf`；解析 `manifest`（id → href）与 `spine`（idref 顺序）
2. 按 spine 顺序读每页 html，用 `<title>第N话</title>` 提取真实话数（无匹配时回退到 `<title>` 内任意数字）
3. 无页码的封面页记为 0、结尾页记为 max+1；按话数升序重排（cover 置首 / theend 殿后）
4. 重命名：`cover.html` / `theend.html` 保留，其余 → `html/page-{N:0width}.html`、`image/{N:0width}.jpg`（width = max(3, 最大话数位数)）
5. 同步改写 `vol.opf` 的 href 引用、重建 `<spine>`，改写 `xml/vol.nav` 的 `src` 引用
6. 清除 `kmoetag` / `kimageraw` / `raw` 脏属性；`mimetype` 置首且 ZIP_STORED，其余 ZIP_DEFLATED
7. 写盘前**回读校验** spine 页码必须连续 `1..N`，失败即回滚（不产出文件、不留 `.tmp` 残留）

## 构建与使用

要求 Rust 1.70+（依赖 `zip` + `regex`，无 GUI 依赖）。

```bash
cargo build --release
# 产物: target/release/kmoefix(.exe)
```

```bash
kmoefix "D:\Manga\某漫画.epub"            # 单文件
kmoefix a.epub b.epub                     # 批量
kmoefix nonexist.epub                     # 跳过不存在: nonexist.epub
```

- 输出与源文件同目录，自动命名为 `*_修正版.epub`，已存在时递增为 `*_修正版 (1).epub` 等，**绝不覆盖原文件**
- 处理成功退出码 0；任一文件失败退出码 1，单文件失败不影响后续文件
- Windows 中文环境（GBK 控制台）下输出自动降级为替换符，不中断处理

## 与原版 Python 的关系

本仓库是对原 Python 版 `core.py` + CLI 的 Rust 重写（原版逻辑随本仓库替代而移除）。重命名/清脏/重打包语义与原版逐行对齐，唯一差异是上述"能力扩展"：

| 输入 | 原版 Python | 本 Rust 版 |
|---|---|---|
| 文件名乱序、spine 正确（初衷场景） | 重命名修复 | 相同，产物语义一致 |
| spine 也乱序 | 回读校验失败回滚 | 按话数重排修复 |
| 缺页/话数不连续（如只有 1,3） | 回读校验失败回滚 | 相同（排序无法补齐缺页） |

**一致性验证**：`tools/parity_check.py` 用同一批样本分别喂给原版 Python 与 Rust 版，逐项比对产物语义（文件集合、条目内容、opf/nav 引用、mimetype 首条 STORED）。12 个场景：9 个要求两侧逐字节一致，2 个为上述能力扩展（断言 Rust 修复成功且 spine 连续 1..N），1 个为两侧都失败回滚。

## 测试

```bash
cargo test                 # Rust 测试（7 个用例）
# 与原版 Python 对拍（可选，需先安装原仓库）
$env:KMOE_PY_SRC = "D:\path\to\python版仓库"   # 指向含 src/core.py 的目录
python tools/parity_check.py                   # 12 场景，0 失败为通过
```

`src/tests.rs` 场景继承自原仓库 `tests/test_fix_one.py`，并补了原测试没覆盖的缺口：乱序修复、乱序且无 cover/theend、无 `xml/vol.nav`、已存在 `(N)` 输出文件等。

## 项目结构

```
├── Cargo.toml            # 依赖：zip + regex
├── src/
│   ├── core.rs           # fix_one / get_unique_dst 核心逻辑
│   ├── lib.rs            # crate 入口
│   ├── main.rs           # CLI 批量入口
│   └── tests.rs          # 集成测试
├── tools/
│   ├── parity_check.py   # Python 原版 vs Rust 对拍器
│   └── make_sample.py    # 测试样本生成
```

## Roadmap

- [ ] GUI（推荐 egui + rfd：纯 Rust、单 exe；替代原版 Tkinter 拖拽批量 + NeeView 联动）
- [ ] `--check` 只读预检模式（不写文件，报告文件名/页序状态）
- [ ] 通用 EPUB 支持：经 `META-INF/container.xml` 定位 opf、支持 `*.xhtml`、改写 nav 的 `epub:href` 引用

## 许可证

[MIT](LICENSE) — Copyright (c) 2026 Catapult291（原版逻辑作者）；本仓库为其 Rust 移植与替代。
