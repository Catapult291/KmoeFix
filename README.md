# kmoefix — 漫画 EPUB 文件名/页序修复工具（Rust）

把 Kmoe 这类站点下载的漫画 EPUB 按**真实页码**重命名，输出符合 EPUB 规范的 `*_修正版.epub`，让按文件名排序的看图场景（NeeView、解压看图）不再乱序。附带把侧放的跨页转正（按包内参照图判定）。

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
4. **判出站点卡片页**（Kmoe 插在正文里的站点卡，见下节）并摘出正文编号，排到正文之后、theend 之前
5. 重命名：`cover.html` / `theend.html` 保留，正文页 → `html/page-{N:0width}.html`、`image/{N:0width}.jpg`，卡片页 → `html/kmoe-{K:0width}.html`、`image/kmoe-{K:0width}.<ext>`（width = max(3, 最大话数位数)）
6. 同步改写 `vol.opf` 的 href 引用、重建 `<spine>`，改写 `xml/vol.nav` 的 `src` 引用
7. 清除 `kmoetag` / `kimageraw` / `raw` 脏属性；`mimetype` 置首且 ZIP_STORED，其余 ZIP_DEFLATED
8. 写盘前**回读校验** spine 页码必须连续 `1..N`，失败即回滚（不产出文件、不留 `.tmp` 残留）
9. 默认按包内参照图（`<图名>-RAWIMAGE.<ext>`）判定并回正**所有侧放的页**（封面、正文第 1 页，以及任何带参照图的页）；判为已正立、没有参照图的图逐字节原样写出；`--no-rotate-cover` 可完全关闭

## 构建与使用

要求 Rust 1.70+（核心依赖 `zip` + `regex` + `image`；GUI 另需 eframe/egui + rfd）。

```bash
cargo build --release
# 产物: target/release/kmoefix.exe —— 命令行与图形界面是同一个程序

RUSTFLAGS="-C target-feature=+crt-static" cargo build --release
# 发布版用的静态链接构建：不依赖 VC++ 运行库，拷到任何 Windows x64 上直接跑
```

```bash
kmoefix                                   # 不带参数：打开图形界面（双击 exe 也一样）
kmoefix --gui "D:\Manga\某漫画.epub"      # 打开图形界面，并把文件装填进列表
kmoefix "D:\Manga\某漫画.epub"            # 单文件
kmoefix a.epub b.epub                     # 批量
kmoefix nonexist.epub                     # 跳过不存在: nonexist.epub
kmoefix --no-rotate-cover a.epub          # 不做任何图片处理（图片与原版逐字节一致）
kmoefix --version                         # 版本号；--help 看用法
```

- 命令行与图形界面是**同一个 exe**：不带参数（双击）开图形界面，带文件走命令行批处理，两者共用同一套核心逻辑
- Windows 下按 GUI 子系统编译：双击不弹控制台窗口。在终端里运行时接管当前控制台输出；把文件拖到 exe 图标上（有参数但没有终端）时自己开一个控制台显示日志

- 输出与源文件同目录，自动命名为 `*_修正版.epub`，已存在时递增为 `*_修正版 (1).epub` 等，**绝不覆盖原文件**
- 处理成功退出码 0；任一文件失败退出码 1，单文件失败不影响后续文件
- Windows 中文环境（GBK 控制台）下输出自动降级为替换符，不中断处理

### 侧放页回正（默认开启）

Kmoe 包里有些跨页被**横躺着存成竖版**：例如 `[Kmoe][少年犯之七人]卷01` 的正文第 1 页是 1138x2160，实为一张横版跨页逆时针转 90° 存放（顺时针转 90° 才能正常阅读）。一本里可能有多页这样（`[Kmoe][電鋸人2]話001-005[話098-102]` 有 11 页）。

判定**只有一条依据**：Kmoe 包会为**每一张**侧放的页遗留一张原始跨页图 `<图名>-RAWIMAGE.<ext>`（扩展名可能与页图不同：`[Kmoe][電鋸人2]話001-005[話098-102]` 就是页图 `.jpg` + 参照图 `.png`）。把候选页的四个朝向分别与它做归一化相关度，取「宽高比一致且最像」的朝向——方向直接判出，不靠猜。实测卷 01 顺时针 90° 相关度 0.87，卷 02 1.00，方向不对的朝向只有 0.1 上下。

- **没有参照图（或相似度不足、参照图读不出）一律不旋转。** 2026-09-13 之前存在过一条「尺寸兜底」规则（页宽 < 全卷页宽中位数 × 0.85 且宽高比 < 0.6 就按顺时针 90° 转），它判不出顺/逆、属于猜方向，会把本来正常的窄页转坏，已删除——实测 `[Kmoe][女僕咖啡廳]卷01` 全包一张参照图都没有，正是被它误转过。
- 候选 = `cover.html` 引用的封面图 + 正文第 1 页的图 + **任何带同名参照图的页**（等于包自己声明了「这页是侧放的」）；判为已正立的页一个像素都不动，没有参照图的页一律不碰。
- `--rotate-cover=90/180/270` 只是**覆盖自动判出的方向**，不改变「是否侧放」的判断，对所有判出侧放的页生效；没有参照图、判不出方向的页不做处理。
- 回正不逐页写日志：判为已正立、没有参照图、相似度不足都静默略过，确有一页被回正时在「页序已按页码重排完成」之后汇总一行「已回正 N 张侧放页」（N = 实际旋转的图片数，同图被多页引用只算一张）。只有图读不出、无法解码、重新编码失败这类异常才逐条提示。
- JPEG 旋转后必须重编码（质量 95，一次代际损失，肉眼不可见），PNG 走无损。

```bash
kmoefix a.epub                            # 默认：按参照图自动回正
kmoefix --no-rotate-cover a.epub          # 关闭回正（图片一个像素都不动）
kmoefix --rotate-cover=90 a.epub          # 已判出侧放时按顺时针 90° 回正（覆盖自动判出的方向；180/270 同理）
kmoefix --rotate-cover=off a.epub         # 同 --no-rotate-cover
```

GUI 上没有回正开关（默认行为就是按参照图回正）。需要关掉或指定方向时，改 exe 同目录 `kmoe_fix_config.json` 里的 `rotate_cover` 字段：`"off"` 关闭，`"90"` / `"180"` / `"270"` 指定方向，留空即默认。

### 站点卡片页移到卷末（默认开启，无开关）

Kmoe 按「話」拼卷，每話首尾会插一张**站点自己的卡片**（带 Kmoe logo 的空白卡 / 写着作品名·作者·编号的信息卡）。一卷里有两話时，前一話的卡片就落在正文中间——`[Kmoe][陰陽眼見子]卷02` 的信息卡在源包 `spine` 的第 145 个位置上，`[Kmoe][進擊的巨人]卷30` 在 122/154/176 三处。

这类页**不占正文页码**：产物里它们改名为 `html/kmoe-{K}.html` + `image/kmoe-{K}.<ext>`，排到最后一张正文页之后、`theend` 之前；被它们挤掉的正文页编号前移（`陰陽眼見子`卷02 的正文由 1..164 变成连续的 1..163），页面的 `<title>` 与 `alt` 里的页码同步改，卡片页的标题换成 `Kmoe`（免得留一个不存在的「第 145 頁」）。日志汇总一行「已把 N 张站点卡片页移到卷末」。

判定：包里**没有任何标记**能认出它——文件名、`<title>`、`xml/vol.nav` 标签、html 模板都跟正文页一样，差别只在图像内容。所以只能看图：

- 把每页图片的**下部条带**与包内 theend 卡（`theend.html` 引用的那张图）的同一条带做归一化相关度；站点卡片是同一套模板渲染的，实测 15 个真实卷里卡片页 **≈1.000**、最像的正文页 **≤0.216**，阈值取 **0.9**。
- **「近白 + 体积小」这类规则不能用**：实测会把 `[Kmoe][我們的離婚]卷01` 第 2 頁（版本说明页）、`[Kmoe][今際之國的闖關者]卷01` 第 3 頁（目录页）这类合法前置页误判成卡片，所以不采用。
- 局限：①包内没有 theend 卡（拿不到参照）时一律不判；②只认「信息卡」那一类，`陰陽眼見子`卷01/卷02 第 1 頁那种**纯空白 logo 卡**页脚样式不同，认不出（它在卷首，通常不影响阅读）；③手工加到书里的、与站点卡片版面一致的页会被一起移到卷末。

```bash
kmoefix a.epub                            # 默认：判出卡片页就移位
```

### GUI（双击 exe，或 `kmoefix --gui`）

egui + rfd 实现的原生单 exe 图形界面（与命令行共用同一个 exe），**界面按原 Python 版 `src/gui.py`（Tkinter + ttk vista 主题）逐控件复刻**：窗口 720x560（最小 680x520），窗口标题、各控件的位置/尺寸/配色/字号均按原版 exe 在 150% 缩放下的实测像素对齐。

自上而下与原版一致：

- 提示标签「拖拽 ZIP/EPUB/CBZ 到窗口，或点击添加」
- Listbox（Consolas 9pt，extended 多选，右侧垂直滚动条）+ 拖拽添加 zip/epub/cbz（目录拖入收集其中的支持文件）
- 按钮行：添加文件… / 移除选中 / 清空，右端「开始处理」
- LabelFrame「选项」：复选框「完成后用 NeeView 打开（仅打开最后成功项）」+「NeeView 路径:」+ 输入框 +「浏览…」（与原版一致；侧放回正是默认行为，界面上没有开关）
- 进度条（indeterminate，静止时左侧常驻绿色块——与原版 ttk 表现一致；处理中滑动）
- 「日志:」+ 只读文本区（Consolas 9pt，成功绿 / 失败红 / 信息蓝三种着色，自动吸底）

行为同样对齐原版：添加去重；空列表点「开始处理」弹警告；开始后清空日志、启动进度条、禁用「开始处理」、后台线程逐个跑 `core::fix_one`、队列轮询回填日志；结束时汇总「全部完成: 成功 X / 失败 Y」并回写配置；勾选 NeeView 且路径不存在时按原版文案弹确认框。配置持久化到 exe 同目录 `kmoe_fix_config.json`，与 Python 版字段同构。Windows 下自动加载系统 Segoe UI / Consolas / 微软雅黑，保证西文、等宽与中文都与原版观感一致。

GUI 无头自检（环境变量驱动，正常使用不受影响）：

```powershell
$env:KMOEFIX_GUI_SHOT = "shot.png"        # 截图输出路径；进程截图后自动退出
$env:KMOEFIX_GUI_AUTOTEST = "a.epub;b.zip" # 可选：启动即装填并自动处理，Done 后延迟截图
.\target\release\kmoefix.exe               # 不带参数即 GUI
```

## 与原版 Python 的关系

本仓库是对原 Python 版 `core.py` + CLI 的 Rust 重写（原版逻辑随本仓库替代而移除）。重命名/清脏/重打包语义与原版逐行对齐，唯一差异是上述"能力扩展"：

| 输入 | 原版 Python | 本 Rust 版 |
|---|---|---|
| 文件名乱序、spine 正确（初衷场景） | 重命名修复 | 相同，产物语义一致 |
| spine 也乱序 | 回读校验失败回滚 | 按话数重排修复 |
| 缺页/话数不连续（如只有 1,3） | 回读校验失败回滚 | 相同（排序无法补齐缺页） |
| 含站点卡片页 | 卡片照原样占着页码 | 移出正文编号、排到卷末（见上节） |

侧放页回正与站点卡片页移位都是纯新增能力，不参与上述对拍。没有卡片页的包，加 `--no-rotate-cover` 时产物与原版**逐字节一致**（已用真实样本对拍验证）；有卡片页的包，`--no-rotate-cover` 只保证图片像素不变，页序仍会按上节调整。

**一致性验证**：`tools/parity_check.py` 用同一批样本分别喂给原版 Python 与 Rust 版，逐项比对产物语义（文件集合、条目内容、opf/nav 引用、mimetype 首条 STORED）。12 个场景：9 个要求两侧逐字节一致，2 个为上述能力扩展（断言 Rust 修复成功且 spine 连续 1..N），1 个为两侧都失败回滚。

## 测试

```bash
cargo test                 # Rust 测试（27 个用例：核心 20 + 命令行分发 6 + GUI 配置映射 1）
# 与原版 Python 对拍（可选，需先安装原仓库）
$env:KMOE_PY_SRC = "D:\path\to\python版仓库"   # 指向含 src/core.py 的目录
python tools/parity_check.py                   # 12 场景，0 失败为通过
```

`src/tests.rs` 场景继承自原仓库 `tests/test_fix_one.py`，并补了原测试没覆盖的缺口：乱序修复、乱序且无 cover/theend、无 `xml/vol.nav`、已存在 `(N)` 输出文件；侧放页回正的七类场景（参照图定方向、非封面/第 1 页的页同样回正、已正立不动、指定角度覆盖且不误伤、无参照图不转、相似度不足不转、默认开启与 `--no-rotate-cover` 保持原字节）；站点卡片页的三类场景（卡片改名移位且后续页编号前移、重跑产物结果不变、包内无 theend 卡时不动任何页）。`src/cli.rs` 另有 6 个用例覆盖命令行分发（无参数走 GUI、`--gui` 预装、带文件走 CLI、回正开关映射、`--help`/`--version` 优先、只有开关无文件时打印用法）；`src/gui.rs` 末尾有 1 个用例覆盖「配置字段 `rotate_cover` → 回正策略」的映射（缺省与非法值按默认、`off` 关闭、角度生效）。

产物语义校验（含侧放页朝向）：

```bash
python tools/check_output.py "某漫画_修正版.epub" "某漫画.epub"   # 第二个参数用于定位参照图，做朝向判定
```

```bash
python tools/check_output.py "某漫画_修正版.epub"                # 不带输入时只查语义，不做朝向判定
```

## 项目结构

```
├── Cargo.toml            # 依赖：zip + regex + image + eframe/egui + rfd
├── src/
│   ├── core.rs           # fix_one / get_unique_dst 核心逻辑
│   ├── cover.rs          # 侧放页判定与像素回正（参照图定方向）
│   ├── cli.rs            # 命令行解析与分发（无参数=GUI、--gui、--version、用法文本）
│   ├── console.rs        # Windows 控制台接管（GUI 子系统的 exe 跑 CLI 时的输出）
│   ├── gui.rs            # 图形界面（egui + rfd，含无头截图自检）
│   ├── lib.rs            # crate 入口
│   ├── main.rs           # 单 exe 入口：控制台接管 + 分发到 GUI / CLI
│   └── tests.rs          # 集成测试
├── tools/
│   ├── parity_check.py   # Python 原版 vs Rust 对拍器
│   ├── make_sample.py    # 测试样本生成
│   └── check_output.py   # 修正版 EPUB 产物语义校验
```

## Roadmap

- [x] GUI（egui + rfd：纯 Rust、单 exe；替代原版 Tkinter 拖拽批量）
- [ ] `--check` 只读预检模式（不写文件，报告文件名/页序状态）
- [ ] 通用 EPUB 支持：经 `META-INF/container.xml` 定位 opf、支持 `*.xhtml`、改写 nav 的 `epub:href` 引用

## 已知边界

- `xml/vol.nav` 改写目前沿用原版行为，页码重命名后 nav 中部分 `epub:href` 可能仍指向旧文件名；标准 EPUB 阅读器按 spine 阅读，实际阅读不受影响。彻底对齐 nav 引用归入 Roadmap「通用 EPUB 支持」一项。
- 侧放页回正只认包内参照图 `<图名>-RAWIMAGE.<ext>`（扩展名可与页图不同），覆盖封面、正文第 1 页与任何带参照图的页；参照图缺失或相似度不足时**不旋转**——宁可不动，不猜方向。
- JPEG 回正是一次重编码（质量 95），有轻微代际损失；PNG 无损。
- 站点卡片页判定依赖包内的 theend 卡做参照（`theend.html` 引用的图），且只认「信息卡」版面——纯空白 logo 卡页脚样式不同，不予识别；判定阈值按 15 个真实卷标定（卡片 ≈1.000、正文 ≤0.216）。

## 许可证

[MIT](LICENSE) — Copyright (c) 2026 Catapult291（原版逻辑作者）；本仓库为其 Rust 移植与替代。
