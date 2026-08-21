# -*- coding: utf-8 -*-
"""
Kmoe 漫画包顺序修正工具
=======================
Kmoe 下载的漫画包(EPUB)为防扒图，把所有 html / 图片重命名为随机文件名
(html/page-XXXXXX.html、image/moe-XXXXX.jpg)，真实页码藏在
<title>第 N 頁</title> 与 <img alt="第 N 頁"> 里，阅读顺序在 vol.opf 的 spine 里。

正规 EPUB 阅读器按 spine 读，不会乱；但按文件名排序的场景
(解压后看图、NeeView 等按文件名排序的看图器)就会乱序。

本工具：读取 vol.opf 的 spine 顺序 + 每页真实页码，把 html 和图片
重命名为有序文件名(cover / 000.jpg / page-001 / 001.jpg ...)，同步改写
vol.opf 与 xml/vol.nav，重建输出到原文件同目录，默认不调用 NeeView，
也可勾选"完成后用 NeeView 打开"。

命令行(无 GUI)用法:
    python kmoe_fix_gui.pyw --cli <文件1> [文件2 ...]
"""

# ── 关键修复：DPI 感知必须在 import tkinter 之前生效 ──────────────
# 必须用 2=PER_MONITOR_V2，且在任何 Tk 调用之前。
import sys
if sys.platform == "win32":
    try:
        import ctypes as _ctypes
        try:
            _ctypes.windll.shcore.SetProcessDpiAwareness(2)
        except Exception:
            _ctypes.windll.user32.SetProcessDPIAware()
    except Exception:
        pass

import os
import re
import json
import queue
import threading
import subprocess
import zipfile
import tkinter as tk
from tkinter import ttk, filedialog, messagebox

APP_TITLE = "Kmoe 漫画包顺序修正"
DEFAULT_NEEVIEW = ""  # 例: r"C:\Program Files\NeeView\NeeView.exe"，留空则需用户在界面中配置
CONFIG_NAME = "kmoe_fix_config.json"
OUT_SUFFIX = "_修正版"

# 界面配色
ACCENT = "#2B7DE9"
BG = "#F4F6FA"
CARD_BG = "#FFFFFF"
BORDER = "#E3E8EF"
TEXT = "#2C3E50"
MUTED = "#7A8699"

TITLE_RE = re.compile(r"<title>\s*第\s*(\d+)\s*頁</title>")
IMG_SRC_RE = re.compile(r'<img[^>]+src="\.\./(image/[^"]+)"')


# --------------------------------------------------------------------------
# 核心：修正一个 Kmoe EPUB
# --------------------------------------------------------------------------
def fix_kmoe_epub(src, dst, log=None):
    """重建 Kmoe 漫画包，使文件名 = 真实页码。返回摘要 dict。"""
    def L(msg):
        if log:
            log(msg)

    L("读取: %s" % os.path.basename(src))
    with zipfile.ZipFile(src) as z:
        names = z.namelist()
        if "vol.opf" not in names:
            raise ValueError("压缩包内未找到 vol.opf，可能不是 Kmoe 下载的漫画包")

        def dec(data):
            try:
                return data.decode("utf-8")
            except UnicodeDecodeError:
                return data.decode("utf-8", errors="replace")

        opf = dec(z.read("vol.opf"))

        # manifest 里 id -> html 路径
        items = dict(re.findall(r'<item\s+id="([^"]+)"\s+href="(html/[^"]+)"', opf))
        spine_m = re.search(r"<spine.*?</spine>", opf, re.S)
        spine_ids = re.findall(r'<itemref\s+idref="([^"]+)"', spine_m.group(0)) if spine_m else []

        # 按 spine 顺序收集每个页面
        entries = []
        for pid in spine_ids:
            href = items.get(pid)
            if not href:
                continue
            hname = href.split("/")[-1]
            body = dec(z.read(href))
            m = TITLE_RE.search(body)
            num = int(m.group(1)) if m else None
            im = IMG_SRC_RE.search(body)
            img = im.group(1) if im else None  # 形如 image/moe-XXXXX.jpg
            entries.append({"pid": pid, "html": href, "hname": hname,
                            "body": body, "num": num, "img": img})

        if not entries:
            raise ValueError("spine 为空，无法处理")
        titled = [e for e in entries if e["num"] is not None]
        if not titled:
            raise ValueError("页面中未找到「第 N 頁」页码，无法排序")

        # 编号：有页码的用真实页码；无页码的首尾条目(封面/结束页)补齐
        first_i = entries.index(titled[0])
        last_i = entries.index(titled[-1])
        maxnum = max(e["num"] for e in titled)
        width = max(3, len(str(maxnum)))
        for i, e in enumerate(entries):
            if e["num"] is None:
                e["num"] = 0 if i < first_i else maxnum + 1

        # 生成新文件名
        html_new = {}   # 旧 html 名 -> {"path": 新路径, "body": 新内容}
        img_new = {}    # 旧图片名 -> [目标文件名...] (同一图被多页引用时复制多份)
        for e in entries:
            n = e["num"]
            if e["hname"] == "cover.html":
                new_html = "html/cover.html"
            elif e["hname"] == "theend.html":
                new_html = "html/theend.html"
            else:
                new_html = "html/page-%0*d.html" % (width, n)

            body = e["body"]
            if e["img"]:
                src_img = os.path.basename(e["img"])
                ext = os.path.splitext(src_img)[1] or ".jpg"
                new_img = "%0*d%s" % (width, n, ext)
                body = body.replace("../" + e["img"], "../image/" + new_img)
                img_new.setdefault(src_img, []).append(new_img)

            html_new[e["hname"]] = {"path": new_html, "body": body}
            e["html_new"] = new_html
            e["img_new"] = e["img"] and os.path.basename(e["img"])

        # 改写 vol.opf / vol.nav 中的路径引用
        for old, v in html_new.items():
            opf = opf.replace("html/" + old, v["path"])
        for old, targets in img_new.items():
            opf = opf.replace("image/" + old, "image/" + targets[0])
        nav_path = "xml/vol.nav"
        nav = None
        if nav_path in names:
            nav = dec(z.read(nav_path))
            for old, v in html_new.items():
                nav = nav.replace("../html/" + old, "../" + v["path"])

        # 未被任何页面引用的图片(保留原样，仅提示)
        unreferenced = sorted(
            n.split("/")[-1] for n in names
            if n.startswith("image/") and os.path.basename(n) not in img_new
        )

        # 重建压缩包
        dst_tmp = dst + ".tmp"
        try:
            with zipfile.ZipFile(dst_tmp, "w") as zout:
                for info in z.infolist():
                    name = info.filename
                    data = z.read(name)
                    zi = zipfile.ZipInfo(name, date_time=info.date_time)
                    zi.compress_type = info.compress_type or zipfile.ZIP_DEFLATED

                    if name == "vol.opf":
                        data = opf.encode("utf-8")
                    elif name == nav_path and nav is not None:
                        data = nav.encode("utf-8")
                    elif os.path.basename(name) in html_new:
                        h = html_new[os.path.basename(name)]
                        zi = zipfile.ZipInfo(h["path"], date_time=info.date_time)
                        zi.compress_type = info.compress_type or zipfile.ZIP_DEFLATED
                        data = h["body"].encode("utf-8")
                        name = h["path"]
                    elif os.path.basename(name) in img_new:
                        for tgt in img_new[os.path.basename(name)]:
                            zt = zipfile.ZipInfo("image/" + tgt, date_time=info.date_time)
                            zt.compress_type = info.compress_type or zipfile.ZIP_DEFLATED
                            zout.writestr(zt, data)
                        continue
                    zout.writestr(zi, data)
            os.replace(dst_tmp, dst)
        finally:
            if os.path.exists(dst_tmp):
                try:
                    os.remove(dst_tmp)
                except OSError:
                    pass

        # 回读验证：spine 顺序应严格 1..N
        verify_ok, detail = verify_epub(dst)
        if not verify_ok:
            raise ValueError("输出验证失败: %s" % detail)

    summary = {
        "pages": len(titled),
        "entries": len(entries),
        "output": dst,
        "unreferenced": unreferenced,
        "verify": detail,
    }
    L("完成: %s (共 %d 页，spine %d 项%s)" %
      (os.path.basename(dst), len(titled), len(entries),
       "，保留未引用文件 %d 个" % len(unreferenced) if unreferenced else ""))
    if unreferenced:
        L("  未引用图片(已保留原样): %s" % ", ".join(unreferenced))
    return summary


def verify_epub(path):
    """回读重建后的包，确认 spine 页码严格递增。返回 (bool, 说明)。"""
    with zipfile.ZipFile(path) as z:
        opf = z.read("vol.opf").decode("utf-8", errors="replace")
        items = dict(re.findall(r'<item\s+id="([^"]+)"\s+href="(html/[^"]+)"', opf))
        spine_m = re.search(r"<spine.*?</spine>", opf, re.S)
        spine_ids = re.findall(r'<itemref\s+idref="([^"]+)"', spine_m.group(0)) if spine_m else []
        seq = []
        for pid in spine_ids:
            href = items.get(pid)
            if not href:
                continue
            body = z.read(href).decode("utf-8", errors="replace")
            m = TITLE_RE.search(body)
            seq.append(int(m.group(1)) if m else None)
        core = [n for n in seq if n is not None]
        ok = bool(core) and core == list(range(1, len(core) + 1))
        return ok, "spine=%d 页, 页码严格 1..%d" % (len(seq), len(core))


def make_output_path(src):
    stem, ext = os.path.splitext(src)
    return stem + OUT_SUFFIX + ext


# --------------------------------------------------------------------------
# GUI
# --------------------------------------------------------------------------
def setup_style(root):
    style = ttk.Style(root)
    # Windows 上 clam 主题在最大化时必现左上角黑块，强制用原生主题
    try:
        if sys.platform == "win32":
            for name in ("vista", "winnative", "xpnative"):
                try:
                    style.theme_use(name)
                    break
                except tk.TclError:
                    continue
            else:
                style.theme_use("clam")
        else:
            style.theme_use("clam")
    except tk.TclError:
        pass
    font = ("Microsoft YaHei UI", 9)
    style.configure(".", font=font)
    style.configure("TFrame", background=BG)
    style.configure("Card.TFrame", background=CARD_BG)
    style.configure("TLabel", background=BG, foreground=TEXT)
    style.configure("Card.TLabel", background=CARD_BG, foreground=TEXT)
    style.configure("Muted.TLabel", background=CARD_BG, foreground=MUTED)
    style.configure("Header.TLabel", background=ACCENT, foreground="#FFFFFF",
                    font=("Microsoft YaHei UI", 14, "bold"))
    style.configure("HeaderSub.TLabel", background=ACCENT, foreground="#DCE9FB",
                    font=("Microsoft YaHei UI", 9))
    style.configure("TButton", padding=(12, 6))
    style.configure("Accent.TButton", background=ACCENT, foreground="#FFFFFF",
                    padding=(14, 8), font=("Microsoft YaHei UI", 10, "bold"))
    style.map("Accent.TButton",
              background=[("active", "#1F6FD0"), ("disabled", "#9DBDE8")],
              foreground=[("disabled", "#F0F5FD")])
    style.configure("TLabelframe", background=CARD_BG, bordercolor=BORDER,
                    relief="solid", borderwidth=1)
    style.configure("TLabelframe.Label", background=CARD_BG, foreground="#5B6770",
                    font=("Microsoft YaHei UI", 9, "bold"))
    style.configure("TEntry", fieldbackground="#FFFFFF", bordercolor="#C7D0DA",
                    padding=(6, 4))
    style.configure("TCheckbutton", background=CARD_BG, foreground=TEXT)
    style.configure("Horizontal.TProgressbar", background=ACCENT,
                    troughcolor="#E3E8EF", borderwidth=0)
    style.configure("Status.TLabel", background="#E9EDF2", foreground="#5B6770",
                    relief="sunken", padding=(8, 3))


class App:
    def __init__(self, root):
        self.root = root
        self.files = []
        self.log_q = queue.Queue()

        root.title(APP_TITLE)
        root.geometry("860x640")
        root.minsize(720, 560)
        setup_style(root)
        # style 可能改回背景，再强制一次
        try:
            root.configure(bg=BG, bd=0, highlightthickness=0)
        except Exception:
            pass

        self.cfg = self.load_config()
        self.build_ui()

        root.protocol("WM_DELETE_WINDOW", self.on_close)
        self.root.after(120, self.drain_log)

    # -- 界面 --
    def build_ui(self):
        # 顶部标题栏 — 显式指定 bd/highlight，避免系统边框在缩放时露缝
        header = tk.Frame(self.root, bg=ACCENT, bd=0, highlightthickness=0)
        header.pack(fill="x")
        tk.Label(header, text=APP_TITLE, bg=ACCENT, fg="#FFFFFF",
                 font=("Microsoft YaHei UI", 14, "bold"), bd=0).pack(
            anchor="w", padx=18, pady=(14, 2))
        tk.Label(header, text="按真实页码重命名 · 自动重建 EPUB 包", bg=ACCENT,
                 fg="#DCE9FB", font=("Microsoft YaHei UI", 9), bd=0).pack(
            anchor="w", padx=18, pady=(0, 12))

        # ★ 修复1：body 用 tk.Frame 而非 ttk.Frame
        # ttk.Frame(vista) 背景是主题透明绘制，最大化时最慢；tk.Frame 直接填色无闪
        body = tk.Frame(self.root, bg=BG, bd=0, highlightthickness=0)
        body.pack(fill="both", expand=True, padx=14, pady=14)

        # 文件卡片 — 用 tk.Frame 避免 Card.TFrame 主题重绘延迟
        file_card = tk.Frame(body, bg=CARD_BG, bd=1, relief="solid",
                             highlightthickness=0)
        # 用 highlight 模拟 BORDER 颜色，tk.Frame 的 relief 颜色不可控，这里外包一层
        file_card.pack(fill="x")
        # 内边距用一个内部 Frame 实现
        file_inner = tk.Frame(file_card, bg=CARD_BG)
        file_inner.pack(fill="both", expand=True, padx=12, pady=12)

        top = tk.Frame(file_inner, bg=CARD_BG)
        top.pack(fill="x")
        tk.Label(top, text="漫画包文件", bg=CARD_BG, fg=TEXT,
                 font=("Microsoft YaHei UI", 10, "bold"), bd=0).pack(side="left")
        tk.Label(top, text="支持多选 EPUB / ZIP / CBZ", bg=CARD_BG, fg=MUTED,
                 font=("Microsoft YaHei UI", 9), bd=0).pack(side="left", padx=10)
        self.count_var = tk.StringVar(value="已选 0 个")
        tk.Label(top, textvariable=self.count_var, bg=CARD_BG, fg=MUTED,
                 font=("Microsoft YaHei UI", 9), bd=0).pack(side="right")

        btns = tk.Frame(file_inner, bg=CARD_BG)
        btns.pack(fill="x", pady=(8, 6))
        ttk.Button(btns, text="添加文件…", command=self.pick_files).pack(side="left")
        ttk.Button(btns, text="移除选中", command=self.remove_selected).pack(
            side="left", padx=6)
        ttk.Button(btns, text="清空", command=self.clear_files).pack(side="left")

        listfrm = tk.Frame(file_inner, bg=CARD_BG)
        listfrm.pack(fill="both", expand=True)
        self.lb = tk.Listbox(
            listfrm, selectmode="extended", height=7,
            bg="#FFFFFF", fg=TEXT, selectbackground="#D6E9FF",
            selectforeground="#1F4E8C", relief="solid", bd=1,
            highlightthickness=0, highlightbackground="#FFFFFF",
            highlightcolor="#FFFFFF", activestyle="none",
            font=("Microsoft YaHei UI", 10))
        sb = ttk.Scrollbar(listfrm, orient="vertical", command=self.lb.yview)
        self.lb.configure(yscrollcommand=sb.set)
        sb.pack(side="right", fill="y")
        self.lb.pack(side="left", fill="both", expand=True)

        # 选项卡片
        opt_card = tk.Frame(body, bg=CARD_BG, bd=1, relief="solid")
        opt_card.pack(fill="x", pady=(10, 0))
        opt_inner = tk.Frame(opt_card, bg=CARD_BG)
        opt_inner.pack(fill="both", expand=True, padx=12, pady=12)
        tk.Label(opt_inner, text="选项", bg=CARD_BG, fg=TEXT,
                 font=("Microsoft YaHei UI", 10, "bold"), bd=0).pack(anchor="w")
        self.open_neeview = tk.BooleanVar(value=False)
        # Checkbutton 仍用 ttk，但父容器是 tk.Frame，避免主题透黑
        ttk.Checkbutton(opt_inner, text="完成后用 NeeView 打开",
                        variable=self.open_neeview).pack(anchor="w", pady=(8, 4))
        pathrow = tk.Frame(opt_inner, bg=CARD_BG)
        pathrow.pack(fill="x")
        tk.Label(pathrow, text="NeeView：", bg=CARD_BG, fg=TEXT,
                 font=("Microsoft YaHei UI", 9), bd=0).pack(side="left")
        self.neeview_var = tk.StringVar(
            value=self.cfg.get("neeview_path") or DEFAULT_NEEVIEW)
        ent = ttk.Entry(pathrow, textvariable=self.neeview_var)
        ent.pack(side="left", fill="x", expand=True, padx=6)
        ttk.Button(pathrow, text="浏览…", command=self.browse_neeview).pack(side="left")

        # 开始按钮 + 进度条 — 父容器是 tk.Frame
        # 用 tk.Button 替代 ttk.Button：vista/xpnative 主题会忽略 ttk 的 background/foreground，
        # 导致白字白底/透明，只有 hover 时才可见；tk.Button 直接填色始终直观可见
        self.btn_start = tk.Button(
            body, text="开始处理",
            bg=ACCENT, fg="#FFFFFF",
            activebackground="#1F6FD0", activeforeground="#FFFFFF",
            disabledforeground="#F0F5FD",
            font=("Microsoft YaHei UI", 10, "bold"),
            bd=0, relief="flat", highlightthickness=0,
            padx=14, pady=8, cursor="hand2",
            command=self.start)
        self.btn_start.pack(fill="x", pady=(10, 0), ipady=2)
        # hover 时加深，未禁用才生效（避免 disabled 时仍变色）
        self.btn_start.bind("<Enter>", lambda e: self.btn_start.config(bg="#1F6FD0") if str(self.btn_start["state"]) != "disabled" else None)
        self.btn_start.bind("<Leave>", lambda e: self.btn_start.config(bg=ACCENT) if str(self.btn_start["state"]) != "disabled" else None)
        self.progress = ttk.Progressbar(body, mode="indeterminate")

        # 日志
        logfrm = ttk.LabelFrame(body, text="日志", padding=6)
        logfrm.pack(fill="both", expand=True, pady=(10, 0))
        self.logfrm = logfrm
        self.txt = tk.Text(logfrm, height=8, state="disabled", wrap="word",
                           bg="#FFFFFF", fg=TEXT, relief="flat", bd=0,
                           highlightthickness=0, highlightbackground="#FFFFFF",
                           highlightcolor="#FFFFFF",
                           font=("Consolas", 9), padx=6, pady=4)
        self.txt.tag_configure("ok", foreground="#1E9E5A")
        self.txt.tag_configure("err", foreground="#D64545")
        self.txt.tag_configure("info", foreground="#2B7DE9")
        tsb = ttk.Scrollbar(logfrm, orient="vertical", command=self.txt.yview)
        self.txt.configure(yscrollcommand=tsb.set)
        tsb.pack(side="right", fill="y")
        self.txt.pack(side="left", fill="both", expand=True)

        # 状态栏
        self.status = tk.StringVar(value="就绪")
        ttk.Label(self.root, textvariable=self.status, style="Status.TLabel").pack(
            fill="x", side="bottom")

    # -- 文件列表 --
    def pick_files(self):
        paths = filedialog.askopenfilenames(
            title="选择漫画包",
            filetypes=[("漫画包", "*.zip *.epub *.cbz"), ("所有文件", "*.*")])
        for p in paths:
            if p not in self.files:
                self.files.append(p)
        self.refresh_list()

    def remove_selected(self):
        sel = set(self.lb.curselection())
        self.files = [f for i, f in enumerate(self.files) if i not in sel]
        self.refresh_list()

    def clear_files(self):
        self.files = []
        self.refresh_list()

    def refresh_list(self):
        self.lb.delete(0, "end")
        for f in self.files:
            self.lb.insert("end", os.path.basename(f))
        self.count_var.set("已选 %d 个" % len(self.files))
        self.status.set("就绪")

    # -- NeeView 路径 --
    def browse_neeview(self):
        p = filedialog.askopenfilename(title="选择 NeeView.exe",
                                       filetypes=[("程序", "*.exe"), ("所有文件", "*.*")])
        if p:
            self.neeview_var.set(p)
            self.save_config()

    # -- 处理 --
    def start(self):
        if not self.files:
            messagebox.showwarning(APP_TITLE, "请先添加要处理的文件。")
            return
        if self.open_neeview.get() and not os.path.exists(self.neeview_var.get().strip()):
            if not messagebox.askyesno(APP_TITLE,
                                       "未找到 NeeView：\n%s\n\n仍要继续吗？" %
                                       self.neeview_var.get().strip()):
                return
        self.save_config()
        self.btn_start.config(state="disabled", bg="#9DBDE8", fg="#F0F5FD")
        self.progress.pack(fill="x", pady=(8, 0), before=self.logfrm)
        self.progress.start(10)
        self.status.set("正在处理…")
        self.push_log("=" * 60)
        self.push_log("开始处理 %d 个文件" % len(self.files))
        t = threading.Thread(target=self.worker, daemon=True)
        t.start()

    def worker(self):
        results = []
        for src in self.files:
            try:
                dst = make_output_path(src)
                fix_kmoe_epub(src, dst, log=lambda m: self.push_log(m))
                results.append((src, dst, True))
                self.push_log("✔ 输出: %s" % dst)
            except Exception as ex:
                self.push_log("✘ 失败: %s" % os.path.basename(src))
                self.push_log("    原因: %s" % ex)
                results.append((src, None, False))

        opened = []
        if self.open_neeview.get():
            for _, dst, ok in results:
                if ok:
                    opened.append(dst)
        if opened:
            neeview = self.neeview_var.get().strip()
            if neeview and os.path.exists(neeview):
                try:
                    subprocess.Popen([neeview, opened[-1]])
                    self.push_log("已用 NeeView 打开：%s" % opened[-1])
                except Exception as ex:
                    self.push_log("启动 NeeView 失败：%s" % ex)
            else:
                self.push_log("未找到 NeeView，已跳过打开")

        self.push_log("全部完成。")
        try:
            self.root.after(0, self.finish)
        except Exception:
            pass

    def finish(self):
        self.progress.stop()
        self.progress.pack_forget()
        self.btn_start.config(state="normal", bg=ACCENT, fg="#FFFFFF")
        self.status.set("完成")

    # -- 日志 --
    def push_log(self, msg):
        self.log_q.put(str(msg))

    def drain_log(self):
        try:
            while True:
                msg = self.log_q.get_nowait()
                self.txt.config(state="normal")
                tag = None
                if msg.startswith("✔"):
                    tag = "ok"
                elif msg.startswith("✘"):
                    tag = "err"
                elif msg.startswith(("开始", "完成", "输出")):
                    tag = "info"
                if tag:
                    self.txt.insert("end", msg + "\n", tag)
                else:
                    self.txt.insert("end", msg + "\n")
                self.txt.see("end")
                self.txt.config(state="disabled")
        except queue.Empty:
            pass
        self.root.after(120, self.drain_log)

    # -- 配置 --
    def config_path(self):
        return os.path.join(os.path.dirname(os.path.abspath(sys.argv[0])), CONFIG_NAME)

    def load_config(self):
        try:
            with open(self.config_path(), encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            return {}

    def save_config(self):
        try:
            with open(self.config_path(), "w", encoding="utf-8") as f:
                json.dump({"neeview_path": self.neeview_var.get().strip()}, f,
                          ensure_ascii=False, indent=2)
        except Exception:
            pass

    def on_close(self):
        self.save_config()
        self.root.destroy()


def main_gui():
    root = tk.Tk()
    # ★ 修复2：withdraw 隐藏启动过程，布局算完再一次性显示
    # 这是 Tk 8.6 Windows 上消除最大化/启动黑闪的唯一可靠办法，
    # 比监听 <Configure> + update_idletasks() 可靠得多
    root.withdraw()
    try:
        root.configure(bg=BG, bd=0, highlightthickness=0)
    except Exception:
        pass
    # 标题和图标先设好，避免 deiconify 后再闪
    root.title(APP_TITLE)
    root.geometry("860x640")
    root.minsize(720, 560)

    App(root)

    # 在后台把所有控件布局算完，再显示窗口
    try:
        root.update_idletasks()
    except Exception:
        pass
    root.deiconify()
    # 确保背景统一，deiconify 后再强制一次
    try:
        root.configure(bg=BG)
    except Exception:
        pass
    root.mainloop()


def main_cli(argv):
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:
            pass
    ok = True
    for src in argv:
        try:
            dst = make_output_path(src)
            summary = fix_kmoe_epub(src, dst, log=print)
            print("[OK] %s" % summary["output"])
            print("     %s" % summary["verify"])
        except Exception as ex:
            print("[FAIL] %s -> %s" % (src, ex))
            ok = False
    return 0 if ok else 1


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--cli":
        sys.exit(main_cli(sys.argv[2:]))
    main_gui()
