# -*- coding: utf-8 -*-
from __future__ import annotations

import json
import os
import sys
from typing import Any

APP_NAME: str = "KmoeFix"
VERSION: str = "1.0.0"
CONFIG_NAME: str = "kmoe_fix_config.json"
OUT_SUFFIX: str = "_修正版"
DEFAULT_NEEVIEW: str = ""


def get_config_path() -> str:
    if getattr(sys, "frozen", False):
        base: str = os.path.dirname(sys.executable)
    else:
        base = os.path.dirname(os.path.abspath(__file__))
    return os.path.join(base, CONFIG_NAME)


def load_config() -> dict[str, Any]:
    cfg: dict[str, Any] = {"neeview_path": DEFAULT_NEEVIEW, "open_with_neeview": False}
    p: str = get_config_path()
    if os.path.exists(p):
        try:
            with open(p, "r", encoding="utf-8") as f:
                d: dict[str, Any] = json.load(f)
                cfg.update({k: d[k] for k in cfg if k in d})
        except Exception:
            pass
    return cfg


def save_config(cfg: dict[str, Any]) -> None:
    p: str = get_config_path()
    try:
        with open(p, "w", encoding="utf-8") as f:
            json.dump(cfg, f, ensure_ascii=False, indent=2)
    except Exception:
        pass
