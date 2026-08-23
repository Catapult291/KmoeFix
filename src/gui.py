# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import queue
import subprocess
import threading
from typing import Any


def run_gui() -> None:
    import tkinter as tk
    from tkinter import filedialog, messagebox, ttk

    try:
        from tkinterdnd2 import DND_FILES as _DND_FILES
        from tkinterdnd2 import TkinterDnD as _TkinterDnD

        has_dnd: bool = True
        Root: Any = _TkinterDnD.Tk
        DND_FILES: Any = _DND_FILES
    except Exception:
        has_dnd = False
        Root = tk.Tk
        DND_FILES = None

    from src.config import APP_NAME, VERSION, load_config, save_config
    from src.core import fix_one, get_unique_dst

    cfg: dict[str, Any] = load_config()
    root: Any = Root()
    root.title(f"{APP_NAME} v{VERSION} - Kmoe漫画包顺序修正")
    root.geometry("720x560")
    try:
        root.minsize(680, 520)
    except Exception:
        pass

    files: list[str] = []

    style: Any = ttk.Style()
    try:
        style.theme_use("vista")
    except Exception:
        pass

    top: Any = ttk.Frame(root, padding=10)
    top.pack(fill="x")

    ttk.Label(top, text="拖拽 ZIP/EPUB/CBZ 到窗口，或点击添加", font=("", 9)).pack(anchor="w")

    list_frame: Any = ttk.Frame(root, padding=(10, 0, 10, 0))
    list_frame.pack(fill="both", expand=True)

    lb: Any = tk.Listbox(list_frame, selectmode="extended", font=("Consolas", 9))
    sb: Any = ttk.Scrollbar(list_frame, orient="vertical", command=lb.yview)
    lb.configure(yscrollcommand=sb.set)
    lb.pack(side="left", fill="both", expand=True)
    sb.pack(side="right", fill="y")

    if has_dnd:

        def on_drop(event: Any) -> None:
            data: tuple[str, ...] = root.tk.splitlist(event.data)
            added: int = 0
            for p in data:
                p = p.strip("{}")
                if os.path.isfile(p) and p.lower().endswith((".zip", ".epub", ".cbz")):
                    if p not in files:
                        files.append(p)
                        lb.insert("end", p)
                        added += 1
                elif os.path.isdir(p):
                    import glob as _g

                    for ext in (".zip", ".epub", ".cbz"):
                        for fp in _g.glob(os.path.join(p, f"*{ext}")):
                            if fp not in files:
                                files.append(fp)
                                lb.insert("end", fp)
                                added += 1
            if added:
                log(f"已添加 {added} 个文件（拖拽）", "info")

        try:
            lb.drop_target_register(DND_FILES)
            lb.dnd_bind("<<Drop>>", on_drop)
        except Exception:
            pass

    def add_files() -> None:
        paths: tuple[str, ...] = filedialog.askopenfilenames(
            title="选择 Kmoe 漫画包", filetypes=[("漫画包", "*.zip *.epub *.cbz"), ("所有文件", "*.*")]
        )
        cnt: int = 0
        for p in paths:
            if p not in files:
                files.append(p)
                lb.insert("end", p)
                cnt += 1
        if cnt:
            log(f"已添加 {cnt} 个文件", "info")

    def remove_sel() -> None:
        sel: tuple[int, ...] = lb.curselection()
        for idx in reversed(sel):
            lb.delete(idx)
            del files[idx]
        if sel:
            log(f"已移除 {len(sel)} 项", "info")

    def clear_all() -> None:
        files.clear()
        lb.delete(0, "end")
        log("已清空", "info")

    btn_row: Any = ttk.Frame(root, padding=(10, 5, 10, 5))
    btn_row.pack(fill="x")

    ttk.Button(btn_row, text="添加文件…", command=add_files).pack(side="left")
    ttk.Button(btn_row, text="移除选中", command=remove_sel).pack(side="left", padx=5)
    ttk.Button(btn_row, text="清空", command=clear_all).pack(side="left")

    opts: Any = ttk.LabelFrame(root, text="选项", padding=10)
    opts.pack(fill="x", padx=10, pady=5)

    open_var: Any = tk.BooleanVar(value=cfg.get("open_with_neeview", False))
    path_var: Any = tk.StringVar(value=cfg.get("neeview_path", ""))

    chk: Any = ttk.Checkbutton(opts, text="完成后用 NeeView 打开（仅打开最后成功项）", variable=open_var)
    chk.grid(row=0, column=0, columnspan=3, sticky="w")

    ttk.Label(opts, text="NeeView 路径:").grid(row=1, column=0, sticky="w", pady=4)
    ent: Any = ttk.Entry(opts, textvariable=path_var, width=54)
    ent.grid(row=1, column=1, sticky="ew", padx=5)

    def browse() -> None:
        p: str = filedialog.askopenfilename(title="选择 NeeView.exe", filetypes=[("exe", "*.exe"), ("所有", "*.*")])
        if p:
            path_var.set(p)

    ttk.Button(opts, text="浏览…", command=browse).grid(row=1, column=2)
    opts.columnconfigure(1, weight=1)

    prog: Any = ttk.Progressbar(root, mode="indeterminate")
    prog.pack(fill="x", padx=10, pady=(5, 0))

    log_frame: Any = ttk.Frame(root, padding=(10, 0, 10, 10))
    log_frame.pack(fill="both", expand=True)
    ttk.Label(log_frame, text="日志:").pack(anchor="w")
    txt: Any = tk.Text(log_frame, height=12, font=("Consolas", 9), wrap="word")
    sb2: Any = ttk.Scrollbar(log_frame, orient="vertical", command=txt.yview)
    txt.configure(yscrollcommand=sb2.set)
    sb2.pack(side="right", fill="y")
    txt.pack(side="left", fill="both", expand=True)
    txt.tag_configure("ok", foreground="#1E9E5A")
    txt.tag_configure("err", foreground="#D64545")
    txt.tag_configure("info", foreground="#2B7DE9")

    def log(msg: str, tag: str = "info") -> None:
        txt.insert("end", msg + "\n", tag)
        txt.see("end")
        root.update_idletasks()
        print(msg)

    q: queue.Queue[tuple[str, object]] = queue.Queue()

    def worker() -> None:
        last_ok: str | None = None
        ok_cnt: int = 0
        fail_cnt: int = 0

        def qlog(s: str) -> None:
            q.put(("info", "  " + s))

        for src in list(files):
            q.put(("info", f"▶ 处理: {os.path.basename(src)}"))
            dst: str = get_unique_dst(src)
            try:
                res: str = fix_one(src, dst, log=qlog)
                last_ok = res
                ok_cnt += 1
                q.put(("ok", f"✔ 完成: {os.path.basename(res)}"))
            except Exception as e:
                fail_cnt += 1
                import traceback

                q.put(("err", f"✘ 失败 {os.path.basename(src)}: {e}"))
                q.put(("err", traceback.format_exc().splitlines()[-1]))
        q.put(("done", (ok_cnt, fail_cnt, last_ok)))

    def poll() -> None:
        try:
            while True:
                tag, msg = q.get_nowait()
                if tag == "done":
                    prog.stop()
                    btn_start.configure(state="normal")
                    ok_cnt, fail_cnt, last_ok = msg  # type: ignore[misc]
                    log(f"全部完成: 成功 {ok_cnt} / 失败 {fail_cnt}", "ok" if fail_cnt == 0 else "err")
                    cfg["neeview_path"] = path_var.get()
                    cfg["open_with_neeview"] = bool(open_var.get())
                    save_config(cfg)
                    if open_var.get() and last_ok:
                        nee: str = path_var.get()
                        if not os.path.exists(nee):
                            if not messagebox.askyesno("提示", f"NeeView 路径不存在:\n{nee}\n是否仍继续（仅处理完成）？"):
                                return
                        else:
                            try:
                                subprocess.Popen([nee, last_ok])
                                log(f"已用 NeeView 打开: {os.path.basename(last_ok)}", "ok")  # type: ignore[arg-type]
                            except Exception as e:
                                log(f"NeeView 启动失败: {e}", "err")
                    return
                else:
                    log(str(msg), tag)
        except queue.Empty:
            pass
        except Exception as e:
            log(f"NeeView 启动失败: {e}", "err")
        root.after(100, poll)

    def start() -> None:
        if not files:
            messagebox.showwarning("提示", "请先添加文件（支持拖拽）")
            return
        txt.delete("1.0", "end")
        prog.start(12)
        btn_start.configure(state="disabled")
        threading.Thread(target=worker, daemon=True).start()
        poll()

    btn_start: Any = ttk.Button(btn_row, text="开始处理", command=start)
    btn_start.pack(side="right")

    root.mainloop()
