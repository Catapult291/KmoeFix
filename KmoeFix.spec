# -*- mode: python ; coding: utf-8 -*-
# KmoeFix.spec - PyInstaller 6.12 打包配置
# 项目: KmoeFix v1.0.0  入口: src/kmoe_fix.py  依赖: tkinterdnd2
# 生成说明:
#   1. 安装依赖: pip install pyinstaller==6.12 tkinterdnd2
#   2. 打包: pyinstaller KmoeFix.spec  或  pyinstaller --clean --noconfirm KmoeFix.spec
#   3. 产物: dist/KmoeFix.exe  (onefile + windowed, console=False)
#   4. 图标: 若 assets/icon.ico 存在则自动用作 EXE 图标，否则 icon=None
# 兼容: PyInstaller 6.12  |  编码: UTF-8

import os

icon_path = os.path.join("assets", "icon.ico")
icon = icon_path if os.path.exists(icon_path) else None

block_cipher = None

a = Analysis(
    ["src/kmoe_fix.py"],
    pathex=[],
    binaries=[],
    datas=[],
    hiddenimports=["tkinterdnd2"],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
    optimize=0,
)

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name="KmoeFix",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=icon,
)
