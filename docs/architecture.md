# 工作原理

KmoeFix 把 EPUB 当 ZIP 打开：定位 opf 与 spine，按每页标题解析真实话数，重命名条目、改写引用，回读校验通过后才落盘。

## 处理流程

1. 读取 zip 目录，定位 `vol.opf`；解析 `<manifest>`（id → href）与 `<spine>`（idref 顺序）
2. 按 spine 顺序读每页 html，用 `<title>第N话</title>` 提取话数（无匹配时回退到 `<title>` 内任意数字）
3. 无页码的封面页记为 0、结尾页记为 max+1，按话数升序排列（cover 置首、theend 殿后）
4. 判出站点卡片页（见 [site-cards.md](site-cards.md)），摘出正文编号，排到正文之后、theend 之前
5. 重命名：`cover.html` / `theend.html` 保留原名；正文页 → `html/page-{N:0width}.html` 与 `image/{N:0width}.jpg`；卡片页 → `html/kmoe-{K:0width}.html` 与 `image/kmoe-{K:0width}.<ext>`（width = max(3, 最大话数位数)）
6. 改写引用：`vol.opf` 的 manifest href 与 `<spine>` 条目顺序、`xml/vol.nav` 的 src
7. 清除 `kmoetag` / `kimageraw` / `raw` 脏属性；`mimetype` 置首且 `ZIP_STORED`，其余条目 `ZIP_DEFLATED`
8. 回读校验：产物的 spine 顺序必须等于写入顺序、页序必须是连续的 `1..N`；不通过即回滚
9. 侧放页回正（见 [rotation.md](rotation.md)）；`--no-rotate-cover` 时图片逐字节原样写出

## 输入与输出

- 输入：Kmoe 系布局 —— opf 名为 `vol.opf`，页面为 `*.html`，nav 为 `xml/vol.nav`，spine 标签带属性（`<spine page-progression-direction="rtl" toc="ncx" …>`）
- 输出：源文件同目录的 `*_修正版.epub`；同名已存在时递增为 `*_修正版 (1).epub`。原文件不做任何修改
- 失败即回滚：不留下半成品，也不留 `.tmp` 残留

## ZIP 细节

- 条目名按**原始字节**读写（epub 内为 UTF-8）：`zip` crate 会自行解码非 ASCII 名，必须用其 raw 字段写回才能字节无损
- `mimetype` 必须是第一个条目且 `ZIP_STORED` 不压缩，其余条目 `ZIP_DEFLATED`

## 幂等性

产物再次作为输入处理时结果不变：卡片页带 `kmoe-` 前缀，重跑时按前缀识别，不会被当成正文页重新编号。

## 项目结构

```
├── Cargo.toml
├── src/
│   ├── core.rs       # 修复流程主体：解析、排序、重命名、改写引用、回读校验
│   ├── cover.rs      # 侧放页判定与像素回正
│   ├── cli.rs        # 命令行解析与分发
│   ├── console.rs    # Windows 控制台接管（GUI 子系统的 exe 跑 CLI 时的输出）
│   ├── gui.rs        # 图形界面（egui + rfd，含无头截图自检）
│   ├── lib.rs        # crate 入口
│   ├── main.rs       # 单 exe 入口：控制台接管 + 分发到 GUI / CLI
│   └── tests.rs      # 集成测试
└── docs/             # 说明文档
```

## 日志约定

- 未开启回正时固定输出「页序已按页码重排完成（不含旋转处理）」，其余情况输出「页序已按页码重排完成」
- 处理成功退出码 0；任一文件失败退出码 1，单个文件失败不影响后续文件
