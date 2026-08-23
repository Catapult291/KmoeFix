# assets

存放 `KmoeFix` 打包所需静态资源。

## icon.ico

- 若提供 `assets/icon.ico`，`KmoeFix.spec` 会自动将其作为 `EXE(icon=...)` 打包为程序图标。
- 若不存在，spec 中 `icon` 自动为 `None`，使用 PyInstaller 默认图标，不影响打包。
- 建议尺寸：256x256，包含 16/32/48/256 多分辨率，格式 ICO。

> 当前目录仅为占位，首次提交保留结构。添加 `icon.ico` 后无需修改 spec。
