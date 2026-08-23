# 贡献指南 / Contributing Guide

> 感谢你对 KmoeFix 感兴趣！本指南帮助你快速完成环境搭建、本地验证与提交流程。

## 目录

- [开发环境](#开发环境)
- [分支规范](#分支规范)
- [提交规范](#提交规范)
- [本地开发与验证](#本地开发与验证)
- [代码风格](#代码风格)
- [测试说明](#测试说明)
- [PR 流程](#pr-流程)
- [Issue 模板指引](#issue-模板指引)
- [注意事项](#注意事项)

---

## 开发环境

- **Python** >= 3.10（推荐 3.10 / 3.11 / 3.13）
- **系统**：Windows 10 / 11（GUI 依赖 `tkinter` 与 `tkinterdnd2`）
- **依赖**：仅 `tkinterdnd2>=0.1.3`，其余为标准库

```bash
git clone https://github.com/KmoeFix/KmoeFix.git
cd KmoeFix

# 可选：虚拟环境
python -m venv .venv
# PowerShell
.venv\Scripts\Activate.ps1
# CMD
.venv\Scripts\activate.bat

pip install -r requirements.txt
# 或可编辑安装（注册 kmoefix 命令）
pip install -e .

# 额外开发依赖
pip install ruff black pytest
```

---

## 分支规范

- 主分支 `main` 受保护，始终保持可发布状态，禁止直接推送。
- 功能开发从 `main` 拉取分支：

| 类型 | 命名示例 | 说明 |
|---|---|---|
| 功能 | `feat/reorder-spine` | 新功能 |
| 修复 | `fix/mimetype-order` | 缺陷修复 |
| 文档 | `docs/faq-update` | 仅文档变更 |
| 重构 | `refactor/core-split` | 不改变行为的重构 |
| 杂项 | `chore/bump-deps` | 构建、依赖、工具链 |

- 分支名使用小写英文、短横线分隔，保持简短可读。
- 一个分支只做一件事，合并后及时删除。

---

## 提交规范

采用 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/)：

```
<type>(<scope>): <subject>

[可选正文]
[可选脚注]
```

- **type**：`feat` / `fix` / `docs` / `style` / `refactor` / `test` / `chore` / `perf`
- **scope** 可选，如 `core` / `gui` / `config` / `tests`
- **subject** 使用中文或英文，祈使句、首字母小写、末尾无句号，50 字以内

示例：

```text
feat(core): 支持 theend 特殊页置尾处理
fix(gui): 修复校验失败时 WinError 32 文件占用
docs(readme): 补充 NeeView 联动配置说明
test(core): 新增 make_epub 内存构造用例
chore: 升级 ruff 规则至 py310
```

- 每个 commit 保持原子性，可独立回滚与 review。
- 合并前请 `git rebase main` 或 `git merge main` 解决冲突，保持历史清晰。

---

## 本地开发与验证

### 1. 语法快速检查

提交前必跑，零依赖、最快反馈：

```bash
python -m py_compile src/*.py
python -m py_compile tests/*.py
```

或检查单个文件：

```bash
python -m py_compile src/core.py
```

### 2. 内存 EPUB 测试（无需真实文件）

项目不提交任何版权 EPUB，测试通过内存构造最小 ZIP 完成。核心辅助位于 `tests/test_fix_one.py` 的 `make_epub`：

```python
from tests.test_fix_one import make_epub

# make_epub(path, spine_order, with_nav=True)
# spine_order: list[tuple[href, title_val]]
#   href 如 "html/page-1.html"，title_val 为 int / "cover" / "theend" / 字符串
# 函数内部自动：
#   - 生成 mimetype（ZIP_STORED 首条）
#   - 构造 vol.opf 的 <manifest> / <spine>
#   - 写入 html/*.html（含 kmoetag/kimageraw 脏属性）与 image/*.jpg 占位
#   - 可选写入 xml/vol.nav
```

示例 — 构造乱序样本并验证修复：

```python
import tempfile, os
from tests.test_fix_one import make_epub
from src.core import fix_one

with tempfile.TemporaryDirectory() as td:
    src = os.path.join(td, "sample.epub")
    make_epub(src, [
        ("html/cover.html", "cover"),
        ("html/page-2.html", 2),
        ("html/page-1.html", 1),
        ("html/theend.html", "theend"),
    ])
    dst = fix_one(src)  # 输出 sample_修正版.epub，自动校验 spine 连续性
    print(dst)
```

### 3. 单元测试

```bash
# 完整测试（需 pytest）
python -m pytest tests/ -v

# 无 pytest 时可用最小兼容方式手跑（见 tests/test_fix_one.py 用例）
python -m pytest tests/test_fix_one.py -v
```

现有 4 用例覆盖：`test_sorted_epub_fix` / `test_shuffled_epub_raises` / `test_get_unique_dst` / `test_core_import_shim`。

---

## 代码风格

- **格式化**：`black`，行宽 100，目标 `py310`
- **检查**：`ruff`，行宽 100，目标 `py310`，规则 `E/F/W/C90`

配置已在 `pyproject.toml` 中声明：

```toml
[tool.ruff]
line-length = 100
target-version = "py310"

[tool.black]
line-length = 100
target-version = ["py310"]
```

本地执行：

```bash
# 检查
ruff check src/
black --check src/

# 自动修复 / 格式化
ruff check src/ --fix
black src/
```

要求：

- 提交前 `ruff` 与 `black --check` 均通过。
- 全程 **UTF-8**，文件头保留 `# -*- coding: utf-8 -*-`，禁止 GBK。
- `import` 分组按 `ruff` 规则排序，行长不超过 100。

---

## 测试说明

- 新增或修改 `fix_one` / `get_unique_dst` / 正则（`TITLE_RE` 等）行为时，必须补充或更新 `tests/test_fix_one.py` 用例。
- 优先复用 `make_epub` 构造边界样本（如缺失 `cover`、重复话数、含 `raw` 属性等），不要提交真实漫画 EPUB。
- 涉及 ZIP 结构变更需断言 `mimetype` 首条且 `ZIP_STORED`、其余 `ZIP_DEFLATED`。

---

## PR 流程

1. **Fork** 本仓库到个人账号，点击右上角 Fork。
2. **新建分支**：`git checkout -b feat/your-feature`
3. **本地开发**：完成代码与测试，确保 `py_compile` + `ruff` + `black` + `pytest` 通过。
4. **提交**：按提交规范 commit，推送到 Fork：`git push origin feat/your-feature`
5. **发起 PR**：在 GitHub 上对比 `KmoeFix:main <- your:feat/your-feature`，填写模板：
   - 变更动机与背景
   - 主要改动点
   - 测试方式（命令与结果）
   - 是否影响 `fix_one` 行为 / 是否需更新 README FAQ
6. **Code Review**：维护者会在 1–3 个工作日内 review，可能要求补充测试或文档。
7. **合并**：Squash 或 Rebase 合并，保持 `main` 历史线性。

PR 检查清单（提交前自检）：

- [ ] `python -m py_compile src/*.py` 通过
- [ ] `ruff check src/` 与 `black --check src/` 通过
- [ ] `pytest tests/ -v` 通过，新增逻辑有覆盖
- [ ] 全程 UTF-8，未引入 GBK 或硬编码个人路径（`DEFAULT_NEEVIEW` 保持 `""`）
- [ ] 未提交版权 EPUB，仅使用 `make_epub` 生成的测试样本
- [ ] 相关文档（README / FAQ）已同步更新

---

## Issue 模板指引

提交 Issue 前请搜索现有 Issue，避免重复。按类型选择：

### Bug 报告

- **标题**：`[Bug] 简要描述问题`
- **必填**：
  - 复现步骤（含命令行或 GUI 操作）
  - 期望结果 vs 实际结果（含完整日志，脱敏）
  - 环境：Python 版本 / 系统版本 / 安装方式（exe 源码）
  - 最小复现样本：优先贴 `make_epub` 构造代码或脱敏后的 `vol.opf` 片段，**不要上传版权 EPUB**
- **可选**：`zipinfo` 输出 / `vol.opf` 的 `<spine>` 片段

### 功能建议

- **标题**：`[Feature] 一句话描述需求`
- **必填**：
  - 使用场景与动机
  - 期望行为与替代方案
  - 是否愿意提交 PR

### 提问 / 讨论

- **标题**：`[Question] 问题概述`

> 仓库后续将提供 `.github/ISSUE_TEMPLATE/` 表单化模板，在此之前请按上述结构手动填写，信息越完整，定位越快。

---

## 注意事项

- **编码**：所有 `src/*.py` 与文档保持 UTF-8，编辑器请设为 UTF-8 + LF，勿用 GBK 保存。
- **版权**：严禁提交含版权的 EPUB / 图片样本，测试一律用 `tests/test_fix_one.py:make_epub` 内存生成。
- **路径**：不要将个人 `NeeView` 路径写进代码，保持 `src/config.py` 中 `DEFAULT_NEEVIEW = ""`。
- **行为变更**：改动 `fix_one` 输出结构、ZIP 打包顺序或校验逻辑时，需同步更新 README 的“效果对比”与 FAQ。

感谢你的贡献！
