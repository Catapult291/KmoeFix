<!--
适用场景:从零新建、要公开发布的开源项目(不是 fork 别人的仓库)。
用法:复制本文件到项目根目录并命名 AGENTS.md(Claude Code 项目可命名 CLAUDE.md),
     先让 AI 逐项填写下方「项目事实」,再开始开发。
-->

# 项目规则:开源新项目

## 项目事实(新建时先与 AI 逐项确认填写)

- 项目一句话定位:KmoeFix —— 把漫画 EPUB 按真实话数重命名页面/图片、重排 spine、清除脏标签,输出符合 EPUB 规范的 `*_修正版.epub`(Rust 实现,含 CLI 与 GUI)。
- 仓库来源:**从零新建**(GitHub 网页 New repository),不是 fork(`gh repo view` 确认 isFork=false)。
- 目标远程地址:https://github.com/Catapult291/KmoeFix.git(origin,分支 main 跟踪 origin/main)。
- 是否要公开:是(仓库当前 visibility=PUBLIC)。
- 开源许可证(发布前必选):MIT(根目录 LICENSE 已放好,Copyright (c) 2026 Catapult291;Cargo.toml 已写 `license = "MIT"`)。
- 主要语言 / 构建命令:Rust(edition 2021);构建 `cargo build --release`(产物 `target/release/KmoeFix.exe` —— 命令行与 GUI 是同一个 exe;发布版另加 `RUSTFLAGS="-C target-feature=+crt-static"` 静态链接);测试 `cargo test`(27 个用例)。
- 是否已有 .gitignore(若无,建仓库第一步先补上):已有。根目录 .gitignore 覆盖 `/target`、`*_修正版.epub|zip|cbz`、`*.tmp`、`/*.epub`、`/*.exe`、`.grok/`、`/testdata/`、`.DS_Store`、`Thumbs.db`、`Desktop.ini`。

## 铁律(违反任何一条,先停下来说明并等我确认)

### 1. 仓库与 remote

- 这是"自己新建的独立项目",**不是 fork**。禁止以"先 fork 一个参考仓库再改名"的方式起步。
- 只有"想给上游仓库提 PR"才允许 fork;如果做了一半发现想贡献给上游,先停下来问我,而不是继续在本仓库堆提交。
- 换 remote(`git remote set-url` / 改 origin)之前必须说明原因并等我同意。

### 2. 提交纪律

- 禁止 `git add .` / `git add -A` / `git add --all`。只 add 与本次改动直接相关的文件。
- commit 前必须自检:`git status` + `git diff --stat`,逐条确认——新增文件是不是项目需要?有没有 .env、密钥、token、本机路径(如 `C:\Users\<用户名>`)、账号信息、调试产物、无关文件?有疑问先列出清单问我。
- 默认只做本地 commit(存档);**push 和开 PR 必须等我明确说"可以推送/可以提 PR"**。
- 批量完成一个阶段后再提交;不要改一处提一次。提交前把变更说明和 `git diff --stat` 汇总给我 review。

### 3. 流程纪律

- 涉及多文件的功能,先给方案/文件清单,我确认后才动手写。
- 阶段收尾(尤其发布前):必须回头 diff 检查 README、CHANGELOG、示例、注释是否与代码同步,过时内容先更新再提交。
- 面向公众的任何页面/描述定稿前,把完整文案拿给我 review,不要想到哪写到哪直接发布。

### 4. 文案纪律(所有面向公众的文字)

- README、项目描述(description)、commit message、issue/PR 文本:简洁、正式、准确。
- commit message 说清"改了什么、为什么",不用"update/fix"这种没信息的词。
- 不写空话套话;不夸大功能;README 用短句、明确分节、给真实可复制的命令。

## 发布前检查清单(倒数第一步)

1. git 历史里没有密钥 / .env / 个人路径 / 调试文件(必要时用 `git log -p` 抽查,不放心就问我)。
2. `.gitignore` 已覆盖 .env、node_modules、构建产物等。
3. LICENSE 文件已放好,README 顶部写明"项目是什么、怎么跑、许可证"。
4. 项目描述与 README 已按文案纪律定稿,并给我看过。

## 常用操作速查(供 AI 参考,小白向)

- 新建独立项目:GitHub 网页 New repository(空仓库即可)→ 本地 `git remote add origin <地址>` → 不要 fork。
- 首次提交前:先确认 .gitignore,再看 `git status` 有没有不该出现的文件。
