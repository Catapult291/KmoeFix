# 图形界面

egui + rfd 的原生单 exe 界面，与命令行共用同一套核心逻辑。窗口 720x560，最小 680x520。

## 布局

自上而下：

- 提示标签「拖拽 ZIP/EPUB/CBZ 到窗口，或点击添加」
- 文件列表（Consolas 9pt，extended 多选，右侧垂直滚动条）+ 拖拽添加（目录拖入时收集其中支持的文件）
- 按钮行：添加文件… / 移除选中 / 清空，右端「开始处理」
- 选项框：复选框「完成后用 NeeView 打开」+「NeeView 路径:」+ 输入框 +「浏览…」
- 进度条（indeterminate，处理中滑动）
- 「日志:」+ 只读文本区（Consolas 9pt，成功绿 / 失败红 / 信息蓝三色，自动吸底）

Windows 下自动加载系统 Segoe UI / Consolas / 微软雅黑，保证西文、等宽与中文显示正常。

## 行为

- 添加去重；目录拖入时收集其中直接包含的 zip / epub / cbz
- 空列表点「开始处理」弹警告
- 开始后：清空日志 → 启动进度条 → 禁用「开始处理」→ 后台线程逐个处理 → 轮询队列回填日志
- 结束时汇总「全部完成: 成功 X / 失败 Y」，并回写配置
- 结束时把处理成功的项从列表移除，失败项保留，避免对同一批文件重复处理
- 勾选 NeeView 时打开列表里选中那一项的产物；没有选中项则打开最后一个成功项
- 勾选 NeeView 且路径不存在时弹确认框
- 侧放页回正是默认行为，界面上没有开关（见 [rotation.md](rotation.md)）

## 配置文件

`KmoeFix.json`，写在 exe 同目录：

| 字段 | 含义 |
|---|---|
| `neeview_path` | NeeView 可执行文件路径 |
| `open_with_neeview` | 是否在处理完成后用 NeeView 打开产物（优先列表里选中的那一项，其次最后成功项） |
| `rotate_cover` | 回正策略：留空/`"auto"` 自动判定，`"off"` 关闭，`"90"` / `"180"` / `"270"` 指定方向；非法值按默认 |

## 无头自检

供自动化验证用，正常使用不受影响：

```powershell
$env:KMOEFIX_GUI_SHOT = "shot.png"          # 截图输出路径；截图后进程自动退出
$env:KMOEFIX_GUI_AUTOTEST = "a.epub;b.zip"  # 启动即装填并自动处理，完成后延迟截图
.\target\release\KmoeFix.exe                 # 不带参数即 GUI
```
