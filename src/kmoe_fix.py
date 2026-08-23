# -*- coding: utf-8 -*-
# shim - 保留原 CLI 行为
from __future__ import annotations

import os
import sys

from src.config import (
    APP_NAME,
    CONFIG_NAME,
    DEFAULT_NEEVIEW,
    OUT_SUFFIX,
    VERSION,
    get_config_path,
    load_config,
    save_config,
)
from src.core import (
    IMG_SRC_RE,
    KMOETAG_RE,
    MANIFEST_RE,
    MANIFEST_RE2,
    RAW_RE,
    SPINE_RE,
    TITLE_FALLBACK,
    TITLE_RE,
    fix_one,
    get_unique_dst,
)
from src.gui import run_gui

__all__ = [
    "APP_NAME",
    "VERSION",
    "CONFIG_NAME",
    "OUT_SUFFIX",
    "DEFAULT_NEEVIEW",
    "get_config_path",
    "load_config",
    "save_config",
    "TITLE_RE",
    "TITLE_FALLBACK",
    "MANIFEST_RE",
    "MANIFEST_RE2",
    "SPINE_RE",
    "IMG_SRC_RE",
    "KMOETAG_RE",
    "RAW_RE",
    "get_unique_dst",
    "fix_one",
    "run_gui",
]

if __name__ == "__main__":
    # Windows GBK 控制台无法直接打印 ✔/✘，需容错
    def _safe_print(s: str) -> None:
        try:
            print(s)
        except UnicodeEncodeError:
            print(s.encode("utf-8", errors="replace").decode("utf-8", errors="replace"))

    if len(sys.argv) > 1:
        for arg in sys.argv[1:]:
            if os.path.isfile(arg):
                try:
                    out = fix_one(arg)
                    _safe_print(f"[OK] {arg} -> {out}")
                except Exception as e:
                    _safe_print(f"[FAIL] {arg}: {e}")
                    import traceback

                    traceback.print_exc()
            else:
                _safe_print(f"跳过不存在: {arg}")
    else:
        run_gui()
