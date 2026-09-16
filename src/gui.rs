//! KmoeFix 的图形界面：egui + rfd 原生窗口，与命令行共用同一套核心逻辑。
//!
//! 窗口客户区 720x560（最小 680x520），自上而下：
//!
//! ```text
//! 拖拽提示标签（Segoe UI 9pt）
//! 文件列表（Consolas 9pt，多选）+ 垂直滚动条                 ← 可伸缩
//! 按钮行：添加文件… / 移除选中 / 清空 ………………… 开始处理
//! 「选项」框：复选框 + NeeView 路径 + 输入框 + 浏览…
//! 进度条（indeterminate；静止时绿块停在最左）
//! 「日志:」+ 日志文本区（Consolas 9pt，三色，可滚动）        ← 可伸缩
//! ```
//!
//! 交互：添加/拖拽去重、移除选中、清空、开始处理（清空日志 + 启动进度条 +
//! 禁用按钮 + 后台线程逐个 `fix_one` + 队列轮询）、空列表与 NeeView 路径缺失时
//! 的弹窗提示、结束时回写配置。配置持久化到 exe 同目录 `KmoeFix.json`。
//!
//! 入口是 [`run_gui`]：CLI 与 GUI 共用同一个 exe（`src/main.rs` 分发），
//! 无参数双击、或 `--gui [文件…]` 时由 `cli.rs` 走到这里。

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};
use metrics::{CHK_BOX_SIDE, CHK_TEXT_GAP};

/// 应用名与配置文件名：配置写在 exe 同目录，便于程序整体搬移。
const APP_NAME: &str = "KmoeFix";
const CONFIG_NAME: &str = "KmoeFix.json";

const HINT_TEXT: &str = "拖拽 ZIP/EPUB/CBZ 到窗口，或点击添加";
const LOG_LABEL: &str = "日志:";
const OPTS_LABEL: &str = "选项";
const CHK_TEXT: &str = "完成后用 NeeView 打开（仅打开最后成功项）";
const PATH_LABEL: &str = "NeeView 路径:";
const BTN_ADD: &str = "添加文件…";
const BTN_REMOVE: &str = "移除选中";
const BTN_CLEAR: &str = "清空";
const BTN_START: &str = "开始处理";
const BTN_BROWSE: &str = "浏览…";

const FONT_UI: f32 = 12.0; // Segoe UI 9pt @150% DPI
const FONT_MONO: f32 = 12.0; // Consolas 9pt @150% DPI

// ---------------- 度量（单位=逻辑像素） ----------------

mod metrics {
    pub const MARGIN: f32 = 10.0;
    pub const HINT_TOP: f32 = 10.0;
    pub const LIST_TOP: f32 = 36.0;
    pub const LIST_TO_BTN: f32 = 6.6;
    pub const BTN_H: f32 = 25.9;
    pub const BTN_W: f32 = 85.3;
    pub const BTN_GAP: f32 = 6.7;
    pub const BTN_TO_OPTS: f32 = 20.0;
    pub const OPTS_H: f32 = 82.7;
    pub const OPTS_TO_PROG: f32 = 11.3;
    pub const PROG_H: f32 = 22.0;
    pub const PROG_TO_LABEL: f32 = 4.7;
    pub const LOG_LABEL_H: f32 = 16.0;
    pub const BOTTOM: f32 = 10.7;
    /// Listbox 单行高（Consolas 9pt）。
    pub const ROW_H: f32 = 14.0;
    pub const SB_W: f32 = 11.5;
    /// 固定占用窗口高度合计；其余高度按比例分给列表与日志两个可伸缩区。
    pub const FIXED: f32 = 235.9;
    /// 列表 : 日志 的伸缩比例。
    pub const LIST_SHARE: f32 = 0.4703;
    /// 列表框/日志框相对窗口右边距额外内收（滚动条占位）。
    pub const RIGHT_INSET: f32 = 3.3;
    /// 复选框：方框边长、方框到文字的间距。
    pub const CHK_BOX_SIDE: f32 = 13.0;
    pub const CHK_TEXT_GAP: f32 = 5.0;
}

// ---------------- 配色 ----------------

mod color {
    use super::egui::Color32;
    pub const BG: Color32 = Color32::from_rgb(0xF0, 0xF0, 0xF0);
    pub const TEXT: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0xA6, 0xA6, 0xA6);

    pub const BTN_FACE: Color32 = Color32::from_rgb(0xFD, 0xFD, 0xFD);
    pub const BTN_BORDER: Color32 = Color32::from_rgb(0xD5, 0xD5, 0xD5);
    pub const BTN_BORDER_DIM: Color32 = Color32::from_rgb(0xE0, 0xE0, 0xE0);
    pub const BTN_HOVER_FACE: Color32 = Color32::from_rgb(0xEA, 0xF3, 0xFB);
    pub const BTN_PRESS_FACE: Color32 = Color32::from_rgb(0xD9, 0xE9, 0xF7);
    pub const BTN_INNER_TOP: Color32 = Color32::from_rgb(0xE9, 0xE9, 0xE9);
    pub const BTN_INNER_BOTTOM: Color32 = Color32::from_rgb(0xC9, 0xC9, 0xC9);

    pub const FIELD_FACE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
    pub const SUNK_BORDER: Color32 = Color32::from_rgb(0xB0, 0xB0, 0xB0);
    pub const SUNK_INNER: Color32 = Color32::from_rgb(0xF0, 0xF0, 0xF0);
    pub const ENTRY_TOP: Color32 = Color32::from_rgb(0xE9, 0xE9, 0xE9);
    pub const ENTRY_RIGHT: Color32 = Color32::from_rgb(0xC4, 0xC4, 0xC4);
    pub const ENTRY_BOTTOM: Color32 = Color32::from_rgb(0x99, 0x99, 0x99);
    pub const FRAME_BORDER: Color32 = Color32::from_rgb(0xDF, 0xDF, 0xDF);

    pub const CHK_BOX_FACE: Color32 = Color32::from_rgb(0xF3, 0xF3, 0xF3);
    pub const CHK_BOX_BORDER: Color32 = Color32::from_rgb(0xB9, 0xB9, 0xB9);
    pub const CHK_ON: Color32 = Color32::from_rgb(0x00, 0x67, 0xC0);

    pub const PROG_TROUGH: Color32 = Color32::from_rgb(0xE6, 0xE6, 0xE6);
    pub const PROG_BORDER: Color32 = Color32::from_rgb(0xC3, 0xC3, 0xC3);
    pub const PROG_BORDER2: Color32 = Color32::from_rgb(0xD6, 0xD6, 0xD6);
    pub const PROG_FILL: Color32 = Color32::from_rgb(0x06, 0xB0, 0x25);
    pub const PROG_FILL_EDGE: Color32 = Color32::from_rgb(0x76, 0xCB, 0x85);

    pub const SEL_BG: Color32 = Color32::from_rgb(0x00, 0x78, 0xD7);
    pub const SEL_FG: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);

    pub const SB_TRACK: Color32 = Color32::from_rgb(0xF0, 0xF0, 0xF0);
    pub const SB_ARROW: Color32 = Color32::from_rgb(0xA4, 0xA4, 0xA4);
    pub const SB_THUMB: Color32 = Color32::from_rgb(0xCD, 0xCD, 0xCD);
    pub const SB_THUMB_BORDER: Color32 = Color32::from_rgb(0xB4, 0xB4, 0xB4);
}

// ---------------- 配置 ----------------

#[derive(Default, serde::Serialize, serde::Deserialize, Clone)]
struct Config {
    #[serde(default)]
    neeview_path: String,
    #[serde(default)]
    open_with_neeview: bool,
    /// 侧放页回正的覆盖项（GUI 无控件，只能改配置文件）：
    /// `"off"` = 关闭（图片一个像素都不动）；`"90"` / `"180"` / `"270"` =
    /// 人工指定回正方向；缺省或 `"auto"` = 默认行为——只认包内参照图，不做尺寸猜测。
    #[serde(default)]
    rotate_cover: String,
}

impl Config {
    /// 配置取值 → core 的回正策略。缺省即默认行为（按参照图自动回正），
    /// 非法取值也按默认走，不打断处理流程。
    fn rotate_cover_mode(&self) -> crate::RotateCover {
        match self.rotate_cover.trim().to_ascii_lowercase().as_str() {
            "off" => crate::RotateCover::Off,
            "90" => crate::RotateCover::Fixed(90),
            "180" => crate::RotateCover::Fixed(180),
            "270" => crate::RotateCover::Fixed(270),
            _ => crate::RotateCover::Auto,
        }
    }

    fn load() -> Self {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(config_path(), text);
        }
    }
}

fn config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CONFIG_NAME)
}

// ---------------- 事件 / 日志 ----------------

#[derive(Clone, Copy, PartialEq)]
enum Tag {
    Ok,
    Err,
    Info,
}

impl Tag {
    /// 日志三色：ok #1E9E5A / err #D64545 / info #2B7DE9
    fn color(self) -> Color32 {
        match self {
            Tag::Ok => Color32::from_rgb(0x1E, 0x9E, 0x5A),
            Tag::Err => Color32::from_rgb(0xD6, 0x45, 0x45),
            Tag::Info => Color32::from_rgb(0x2B, 0x7D, 0xE9),
        }
    }
}

struct LogLine {
    tag: Tag,
    text: String,
}

/// worker → UI 事件（tag + 文本；done 附带统计）。
enum Event {
    Log { tag: Tag, text: String },
    Done { ok: usize, fail: usize, last_ok: Option<PathBuf> },
}

// ---------------- 布局 ----------------

struct Layout {
    left: f32,
    right: f32,
    hint_cy: f32,
    list: Rect,
    btn_y: f32,
    opts: Rect,
    prog: Rect,
    log_label: Pos2,
    log: Rect,
}

impl Layout {
    fn compute(rect: Rect) -> Self {
        use metrics::*;
        let left = rect.left() + MARGIN;
        let right = rect.right() - MARGIN;
        let flex = (rect.height() - FIXED).max(2.0 * ROW_H * 4.0);
        let list_h = flex * LIST_SHARE;

        let hint_cy = rect.top() + HINT_TOP + 7.0;
        let list_top = rect.top() + LIST_TOP;
        let btn_y = list_top + list_h + LIST_TO_BTN;
        let opts_top = btn_y + BTN_H + BTN_TO_OPTS;
        let prog_top = opts_top + OPTS_H + OPTS_TO_PROG;
        let log_label_y = prog_top + PROG_H + PROG_TO_LABEL;
        let log_top = log_label_y + LOG_LABEL_H;
        let log_bottom = rect.bottom() - BOTTOM;

        Self {
            left,
            right,
            hint_cy,
            list: Rect::from_min_max(
                Pos2::new(left, list_top),
                Pos2::new(right - RIGHT_INSET, list_top + list_h),
            ),
            btn_y,
            opts: Rect::from_min_max(
                Pos2::new(left - 1.3, opts_top),
                Pos2::new(right - 1.3, opts_top + OPTS_H),
            ),
            prog: Rect::from_min_max(
                Pos2::new(left - 1.3, prog_top),
                Pos2::new(right - 0.7, prog_top + PROG_H),
            ),
            log_label: Pos2::new(left + 2.0, log_label_y + 5.6),
            log: Rect::from_min_max(
                Pos2::new(left, log_top),
                Pos2::new(right - RIGHT_INSET, log_bottom),
            ),
        }
    }

    /// 左侧三个按钮（添加文件… / 移除选中 / 清空）。
    fn buttons(&self) -> [Rect; 3] {
        use metrics::*;
        let first = Rect::from_min_max(
            Pos2::new(self.left, self.btn_y),
            Pos2::new(self.left + BTN_W, self.btn_y + BTN_H),
        );
        let second = first.translate(Vec2::new(BTN_W + BTN_GAP, 0.0));
        let third = second.translate(Vec2::new(BTN_W + BTN_GAP, 0.0));
        [first, second, third]
    }
}

// ---------------- 原生外观控件 ----------------

fn button(ui: &mut egui::Ui, rect: Rect, text: &str, enabled: bool) -> bool {
    let resp = ui.interact(
        rect,
        ui.id().with(("KmoeFix_btn", text)),
        if enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let hovered = enabled && resp.hovered();
    let pressed = enabled && resp.is_pointer_button_down_on();
    let face = if pressed {
        color::BTN_PRESS_FACE
    } else if hovered {
        color::BTN_HOVER_FACE
    } else {
        color::BTN_FACE
    };
    let border = if enabled { color::BTN_BORDER } else { color::BTN_BORDER_DIM };

    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(2), face);
    p.rect_stroke(rect, CornerRadius::same(2), Stroke::new(1.0_f32, border), StrokeKind::Inside);
    // 按钮的立体感：内侧上高光 / 下阴影
    if enabled {
        let inner = rect.shrink(1.0);
        p.line_segment(
            [Pos2::new(inner.left() + 2.0, inner.top()), Pos2::new(inner.right() - 2.0, inner.top())],
            Stroke::new(1.0_f32, color::BTN_INNER_TOP),
        );
        p.line_segment(
            [Pos2::new(inner.left() + 2.0, inner.bottom()), Pos2::new(inner.right() - 2.0, inner.bottom())],
            Stroke::new(1.0_f32, color::BTN_INNER_BOTTOM),
        );
    }
    let fg = if enabled { color::TEXT } else { color::TEXT_DIM };
    p.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(FONT_UI), fg);
    resp.clicked()
}

/// 凹陷白底容器（Listbox / Text 共用）：深色外框 + 内侧高光。
fn sunken(p: &egui::Painter, rect: Rect) {
    p.rect_filled(rect, CornerRadius::ZERO, color::FIELD_FACE);
    p.rect_stroke(rect, CornerRadius::ZERO, Stroke::new(1.0_f32, color::SUNK_BORDER), StrokeKind::Inside);
    p.rect_stroke(
        rect.shrink(1.0),
        CornerRadius::ZERO,
        Stroke::new(1.0_f32, color::SUNK_INNER),
        StrokeKind::Inside,
    );
}

/// 输入框：白底 + 上/左浅边、下/右阴影（与列表框的凹陷边框不同）。
fn entry_field(p: &egui::Painter, rect: Rect) {
    p.rect_filled(rect, CornerRadius::ZERO, color::FIELD_FACE);
    let t = Stroke::new(1.0_f32, color::ENTRY_TOP);
    p.line_segment([rect.left_top(), rect.right_top()], t);
    p.line_segment([rect.left_top(), rect.left_bottom()], t);
    p.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, color::ENTRY_BOTTOM),
    );
    p.line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0_f32, color::ENTRY_RIGHT),
    );
}

/// LabelFrame「选项」：1px 边框 + 骑在边框上的标题（文字中线压在上边框上）。
fn label_frame(ui: &mut egui::Ui, rect: Rect, title: &str) {
    let p = ui.painter();
    p.rect_stroke(rect, CornerRadius::ZERO, Stroke::new(1.0_f32, color::FRAME_BORDER), StrokeKind::Inside);
    let anchor = Pos2::new(rect.left() + 8.0, rect.top() + 1.0);
    let galley = p.layout_no_wrap(title.to_owned(), FontId::proportional(FONT_UI), color::TEXT);
    // 标题处断开边框：先用底色铺一条，再画字
    p.rect_filled(
        Rect::from_center_size(anchor, galley.size()),
        CornerRadius::ZERO,
        color::BG,
    );
    p.galley(
        Pos2::new(anchor.x, anchor.y - galley.size().y * 0.5),
        galley,
        color::TEXT,
    );
}

/// 复选框：未选=浅灰面 + 灰边；选中=蓝底白勾。
fn checkbox(ui: &mut egui::Ui, rect: Rect, checked: bool, label: &str) -> bool {
    let p = ui.painter();
    let box_side = CHK_BOX_SIDE;
    let box_rect = Rect::from_min_size(
        Pos2::new(rect.left(), rect.center().y - box_side * 0.5),
        Vec2::splat(box_side),
    );
    if checked {
        p.rect_filled(box_rect, CornerRadius::same(2), color::CHK_ON);
        p.rect_stroke(
            box_rect,
            CornerRadius::same(2),
            Stroke::new(1.0_f32, Color32::from_rgb(0x0C, 0x6E, 0xC2)),
            StrokeKind::Inside,
        );
        let c = box_rect.center();
        let s = Stroke::new(1.6_f32, Color32::WHITE);
        p.line_segment([Pos2::new(c.x - 3.4, c.y + 0.2), Pos2::new(c.x - 1.0, c.y + 2.6)], s);
        p.line_segment([Pos2::new(c.x - 1.0, c.y + 2.6), Pos2::new(c.x + 3.4, c.y - 2.6)], s);
    } else {
        p.rect_filled(box_rect, CornerRadius::same(2), color::CHK_BOX_FACE);
        p.rect_stroke(
            box_rect,
            CornerRadius::same(2),
            Stroke::new(1.0_f32, color::CHK_BOX_BORDER),
            StrokeKind::Inside,
        );
    }
    p.text(
        Pos2::new(box_rect.right() + CHK_TEXT_GAP, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(FONT_UI),
        color::TEXT,
    );

    let resp = ui.interact(
        rect,
        ui.id().with(("KmoeFix_chk", label)),
        egui::Sense::click(),
    );
    resp.clicked()
}

/// 进度条：indeterminate 样式，静止时绿块停在最左。
fn progressbar(p: &egui::Painter, rect: Rect, phase: Option<f32>) {
    p.rect_filled(rect, CornerRadius::ZERO, color::PROG_TROUGH);
    p.rect_stroke(rect, CornerRadius::ZERO, Stroke::new(1.0_f32, color::PROG_BORDER2), StrokeKind::Inside);
    p.rect_stroke(
        rect.shrink(1.0),
        CornerRadius::ZERO,
        Stroke::new(1.0_f32, color::PROG_BORDER),
        StrokeKind::Inside,
    );

    let inner = rect.shrink(3.0);
    let block_w = 15.0;
    let travel = (inner.width() - block_w).max(0.0);
    let x = inner.left() + travel * phase.unwrap_or(0.0);
    let block = Rect::from_min_size(Pos2::new(x, inner.top()), Vec2::new(block_w, inner.height()));
    p.rect_filled(block, CornerRadius::same(3), color::PROG_FILL);
    p.rect_stroke(
        block,
        CornerRadius::same(3),
        Stroke::new(1.0_f32, color::PROG_FILL_EDGE),
        StrokeKind::Inside,
    );
}

/// 垂直滚动条：轨道 + 上下箭头 + 滑块。返回（点击箭头/轨道后的新偏移）。
fn scrollbar(
    ui: &mut egui::Ui,
    rect: Rect,
    content_h: f32,
    view_h: f32,
    offset: f32,
) -> Option<f32> {
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::ZERO, color::SB_TRACK);

    let arrow_h = 9.0;
    let up = Rect::from_min_size(rect.min, Vec2::new(rect.width(), arrow_h));
    let down = Rect::from_min_size(Pos2::new(rect.left(), rect.bottom() - arrow_h), Vec2::new(rect.width(), arrow_h));
    let mut result = None;

    for (r, up_dir) in [(up, true), (down, false)] {
        let resp = ui.interact(r, ui.id().with(("KmoeFix_sb_arrow", up_dir, rect.top() as i32)), egui::Sense::click());
        if resp.hovered() {
            p.rect_filled(r, CornerRadius::ZERO, Color32::from_rgb(0xE5, 0xF1, 0xFB));
        }
        let c = r.center();
        let dir = if up_dir { -1.0 } else { 1.0 };
        p.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(c.x, c.y + 2.4 * dir),
                Pos2::new(c.x - 3.4, c.y - 1.6 * dir),
                Pos2::new(c.x + 3.4, c.y - 1.6 * dir),
            ],
            color::SB_ARROW,
            Stroke::NONE,
        ));
        if resp.clicked() {
            result = Some(offset + 14.0 * dir);
        }
    }

    let track = Rect::from_min_max(
        Pos2::new(rect.left(), rect.top() + arrow_h),
        Pos2::new(rect.right(), rect.bottom() - arrow_h),
    );
    if content_h > view_h && track.height() > 4.0 {
        let thumb_h = (track.height() * view_h / content_h).max(18.0).min(track.height());
        let max_off = content_h - view_h;
        let t = if max_off > 0.0 { (offset / max_off).clamp(0.0, 1.0) } else { 0.0 };
        let thumb = Rect::from_min_size(
            Pos2::new(track.left() + 1.0, track.top() + (track.height() - thumb_h) * t),
            Vec2::new(track.width() - 2.0, thumb_h),
        );
        p.rect_filled(thumb, CornerRadius::same(2), color::SB_THUMB);
        p.rect_stroke(thumb, CornerRadius::same(2), Stroke::new(1.0_f32, color::SB_THUMB_BORDER), StrokeKind::Inside);

        let resp = ui.interact(thumb, ui.id().with(("KmoeFix_sb_thumb", rect.top() as i32)), egui::Sense::click_and_drag());
        if resp.dragged() && max_off > 0.0 {
            let usable = (track.height() - thumb_h).max(1.0);
            result = Some(offset + resp.drag_delta().y / usable * max_off);
        } else if resp.clicked() {
            // 点击滑块外的轨道：上下翻页
            if let Some(mouse) = ui.ctx().pointer_interact_pos() {
                let dir = if mouse.y < thumb.top() { -1.0 } else { 1.0 };
                result = Some(offset + 0.9 * view_h * dir);
            }
        }
    }
    result
}

/// 读取滚轮并返回位移（仅当指针落在区域内部）。
fn wheel_delta(ui: &egui::Ui, rect: Rect) -> f32 {
    let hovering = ui
        .ctx()
        .pointer_hover_pos()
        .is_some_and(|p| rect.contains(p));
    if !hovering {
        return 0.0;
    }
    ui.input(|i| i.raw_scroll_delta.y)
}

// ---------------- App ----------------

#[derive(Default)]
struct Gui {
    files: Vec<PathBuf>,
    cfg: Config,
    running: bool,
    logs: Vec<LogLine>,
    rx: Option<Receiver<Event>>,
    /// 文件列表选中行（升序）与 Shift 连选的锚点。
    sel: Vec<usize>,
    anchor: Option<usize>,
    list_offset: f32,
    log_offset: f32,
    /// 日志是否吸底（收到新行后自动滚到底部）。
    log_stick: bool,
    /// 进度条动画相位。
    phase: f32,
    /// 截图自检：仅当设置了 KMOEFIX_GUI_SHOT（目标 PNG 路径）时启用。
    shot: Option<(Arc<egui::Context>, String)>,
    /// KMOEFIX_GUI_AUTOTEST（分号分隔的文件/目录列表）：启动即装填并自动开始处理，
    /// 收到 Done 后延迟截图，使一张截图同时覆盖 UI 与真实处理日志。
    autotest: bool,
    auto_started: bool,
    /// 收到 Done 后开始等待截图的时间戳，用于延迟触发。
    shot_wait_since: Option<std::time::Instant>,
}

impl Gui {
    fn setup_shot(&mut self, ctx: &egui::Context) {
        let Ok(path) = std::env::var("KMOEFIX_GUI_SHOT") else { return };
        self.shot = Some((Arc::new(ctx.clone()), path));
    }

    fn setup_autotest(&mut self) {
        let Ok(list) = std::env::var("KMOEFIX_GUI_AUTOTEST") else {
            return;
        };
        self.autotest = true;
        let paths: Vec<PathBuf> = list
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect();
        let n = collect_into(&mut self.files, paths);
        self.push_log(Tag::Info, format!("已添加 {n} 个文件（自检装填）"));
    }

    fn take_screenshot(&mut self, ctx: &egui::Context) -> Option<Arc<egui::ColorImage>> {
        let (_, path) = self.shot.as_ref()?;
        let path = path.clone();
        ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        })
        .map(|img| {
            eprintln!("[shot] event captured, saving to {path}");
            img
        })
    }

    fn push_log(&mut self, tag: Tag, text: impl Into<String>) {
        self.logs.push(LogLine { tag, text: text.into() });
        self.log_stick = true;
    }

    /// 收完队列事件；返回是否收到 Done（Done 时停进度条、回写配置）。
    fn pump(&mut self, ctx: &egui::Context) {
        let mut finished = false;
        if let Some(rx) = self.rx.take() {
            while let Ok(e) = rx.try_recv() {
                match e {
                    Event::Log { tag, text } => self.push_log(tag, text),
                    Event::Done { ok, fail, last_ok } => {
                        finished = true;
                        self.running = false;
                        self.rx = None;
                        self.shot_wait_since = Some(std::time::Instant::now());
                        self.push_log(
                            if fail == 0 { Tag::Ok } else { Tag::Err },
                            format!("全部完成: 成功 {ok} / 失败 {fail}"),
                        );
                        self.cfg.save();
                        if self.cfg.open_with_neeview {
                            self.launch_neeview(last_ok);
                        }
                        break;
                    }
                }
            }
            if !finished {
                self.rx = Some(rx); // 处理中，等待下一帧继续收
            }
        }
        if self.running {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }

    /// NeeView 分支：勾选了「完成后打开」且路径不存在时弹确认框；
    /// 框里无论选「是」还是「否」都只结束、不尝试打开。
    fn launch_neeview(&mut self, last_ok: Option<PathBuf>) {
        let Some(ok_path) = last_ok else { return };
        let nee = self.cfg.neeview_path.trim().to_string();
        if !Path::new(&nee).exists() {
            let _ = rfd::MessageDialog::new()
                .set_title("提示")
                .set_description(format!("NeeView 路径不存在:\n{nee}\n是否仍继续（仅处理完成）？"))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            return;
        }
        match std::process::Command::new(&nee).arg(&ok_path).spawn() {
            Ok(_) => self.push_log(Tag::Ok, format!("已用 NeeView 打开: {}", file_name_of(&ok_path))),
            Err(e) => self.push_log(Tag::Err, format!("NeeView 启动失败: {e}")),
        }
    }

    fn handle_drop(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx
            .input(|i| i.raw.dropped_files.clone())
            .into_iter()
            .filter_map(|f| f.path)
            .collect();
        if !dropped.is_empty() {
            let n = collect_into(&mut self.files, dropped);
            if n > 0 {
                self.push_log(Tag::Info, format!("已添加 {n} 个文件（拖拽）"));
            }
        }
    }

    /// 开始处理：空列表弹警告；否则清空日志、启动进度条、禁用按钮、开后台线程。
    fn start(&mut self) {
        if self.files.is_empty() {
            let _ = rfd::MessageDialog::new()
                .set_title("提示")
                .set_description("请先添加文件（支持拖拽）")
                .set_buttons(rfd::MessageButtons::Ok)
                .show();
            return;
        }
        self.logs.clear();
        self.log_offset = 0.0;
        self.log_stick = true;
        self.phase = 0.0;
        self.running = true;
        self.start_worker();
    }

    /// 后台线程：逐个 fix_one，事件全走 channel，UI 线程不碰任何处理逻辑。
    fn start_worker(&mut self) {
        if self.rx.is_some() || self.files.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);

        let files: Vec<PathBuf> = self.files.clone();
        // 回正策略取自配置：缺省即「按包内参照图自动回正」，配置里写了角度或 off 才覆盖
        let opts = crate::FixOptions { rotate_cover: self.cfg.rotate_cover_mode() };
        std::thread::spawn(move || {
            let mut ok_cnt = 0usize;
            let mut fail_cnt = 0usize;
            let mut last_ok: Option<PathBuf> = None;
            for src in &files {
                let name = file_name_of(src);
                let _ = tx.send(Event::Log {
                    tag: Tag::Info,
                    text: format!("▶ 处理: {name}"),
                });
                let dst = crate::get_unique_dst(&src.to_string_lossy());
                let src_s = src.to_string_lossy().into_owned();
                let dst_s = dst.to_string_lossy().into_owned();
                let r = crate::fix_one_with(&src_s, Some(&dst_s), Some(&|s: &str| {
                    let _ = tx.send(Event::Log {
                        tag: Tag::Info,
                        text: format!("  {s}"),
                    });
                }), &opts);
                match r {
                    Ok(_) => {
                        ok_cnt += 1;
                        last_ok = Some(dst.clone());
                        let _ = tx.send(Event::Log {
                            tag: Tag::Ok,
                            text: format!("✔ 完成: {}", file_name_of(&dst)),
                        });
                    }
                    Err(e) => {
                        fail_cnt += 1;
                        // KmoeError 只带消息本身，上一行已经打印过，不再重复。
                        let _ = tx.send(Event::Log {
                            tag: Tag::Err,
                            text: format!("✘ 失败 {name}: {}", e.msg),
                        });
                    }
                }
            }
            let _ = tx.send(Event::Done { ok: ok_cnt, fail: fail_cnt, last_ok });
        });
    }
}

fn is_supported(p: &Path) -> bool {
    p.is_file()
        && p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "zip" | "epub" | "cbz"))
}

/// 文件直接加入列表；目录则收集其中直接包含的 `*.zip` / `*.epub` / `*.cbz`。返回新增数（去重）。
fn collect_into(files: &mut Vec<PathBuf>, paths: Vec<PathBuf>) -> usize {
    let mut added = 0usize;
    for p in paths {
        let cands: Vec<PathBuf> = if p.is_dir() {
            std::fs::read_dir(&p)
                .map(|rd| rd.flatten().map(|e| e.path()).collect())
                .unwrap_or_default()
        } else {
            vec![p]
        };
        for fp in cands {
            if is_supported(&fp) && !files.contains(&fp) {
                files.push(fp);
                added += 1;
            }
        }
    }
    added
}

fn file_name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

// ---------------- 绘制 ----------------

impl Gui {
    fn draw_file_list(&mut self, ui: &mut egui::Ui, l: &Layout) {
        let outer = l.list;
        let sb_rect = Rect::from_min_max(
            Pos2::new(outer.right() - metrics::SB_W, outer.top()),
            outer.max,
        );
        let view = Rect::from_min_max(outer.min, Pos2::new(sb_rect.left(), outer.bottom()));

        sunken(ui.painter(), outer);

        let content_h = self.files.len() as f32 * metrics::ROW_H + 2.0;
        let view_h = (view.height() - 4.0).max(0.0);
        let max_off = (content_h - view_h).max(0.0);

        let wheel = wheel_delta(ui, outer);
        if wheel != 0.0 {
            self.list_offset -= wheel;
        }
        self.list_offset = self.list_offset.clamp(0.0, max_off);

        if let Some(off) = scrollbar(ui, sb_rect, content_h, view_h, self.list_offset) {
            self.list_offset = off.clamp(0.0, max_off);
        }

        let inner = view.shrink2(Vec2::new(3.0, 2.0));
        let p = ui.painter().with_clip_rect(inner);
        let font = FontId::monospace(FONT_MONO);

        for i in 0..self.files.len() {
            let top = inner.top() + i as f32 * metrics::ROW_H - self.list_offset;
            let row = Rect::from_min_size(Pos2::new(inner.left(), top), Vec2::new(inner.width(), metrics::ROW_H));
            if row.bottom() < inner.top() || row.top() > inner.bottom() {
                continue;
            }
            let selected = self.sel.contains(&i);
            if selected {
                p.rect_filled(row, CornerRadius::ZERO, color::SEL_BG);
            }
            p.text(
                Pos2::new(row.left(), row.center().y),
                Align2::LEFT_CENTER,
                self.files[i].to_string_lossy(),
                font.clone(),
                if selected { color::SEL_FG } else { color::TEXT },
            );
        }

        // 点击行：多选（普通=单选、Ctrl=切换、Shift=连选）
        let resp = ui.interact(inner, ui.id().with("KmoeFix_file_list"), egui::Sense::click());
        if resp.clicked() {
            let multi = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            let shift = ui.input(|i| i.modifiers.shift);
            let hit = ui
                .ctx()
                .pointer_interact_pos()
                .filter(|pos| inner.contains(*pos))
                .map(|pos| ((pos.y - inner.top() + self.list_offset) / metrics::ROW_H) as isize)
                .filter(|idx| *idx >= 0 && (*idx as usize) < self.files.len())
                .map(|idx| idx as usize);
            match hit {
                Some(i) if shift => {
                    let anchor = self.anchor.unwrap_or(i);
                    let (a, b) = if anchor <= i { (anchor, i) } else { (i, anchor) };
                    self.sel = (a..=b).collect();
                }
                Some(i) if multi => {
                    if let Some(pos) = self.sel.iter().position(|&s| s == i) {
                        self.sel.remove(pos);
                    } else {
                        self.sel.push(i);
                        self.sel.sort_unstable();
                    }
                    self.anchor = Some(i);
                }
                Some(i) => {
                    self.sel = vec![i];
                    self.anchor = Some(i);
                }
                None => {
                    self.sel.clear();
                    self.anchor = None;
                }
            }
        }
    }

    fn draw_options(&mut self, ui: &mut egui::Ui, l: &Layout) {
        label_frame(ui, l.opts, OPTS_LABEL);
        let content_left = l.opts.left() + 14.3;

        // 「选项」框第一行只有一个复选框：自动回正已是默认行为（只认包内参照图），
        // 界面上不放开关，它的点击区横跨整行。
        let row_top = l.opts.top() + 21.6;
        let row_bottom = l.opts.top() + 43.6;
        let chk_row = Rect::from_min_max(
            Pos2::new(content_left, row_top),
            Pos2::new(l.opts.right() - 12.0, row_bottom),
        );
        if checkbox(ui, chk_row, self.cfg.open_with_neeview, CHK_TEXT) {
            self.cfg.open_with_neeview = !self.cfg.open_with_neeview;
            self.cfg.save();
        }

        let row_y = l.opts.top() + 46.3;
        let row_h = 24.0;
        let p = ui.painter();
        p.text(
            Pos2::new(content_left + 1.0, row_y + row_h * 0.5),
            Align2::LEFT_CENTER,
            PATH_LABEL,
            FontId::proportional(FONT_UI),
            color::TEXT,
        );
        let browse_w = metrics::BTN_W;
        let entry_rect = Rect::from_min_max(
            Pos2::new(content_left + 91.0, row_y),
            Pos2::new(l.opts.right() - 12.7 - browse_w - 7.0, row_y + row_h),
        );
        entry_field(ui.painter(), entry_rect);
        let inner = entry_rect.shrink2(Vec2::new(2.0, 2.0));
        let text_color = color::TEXT;
        let resp = ui.put(
            inner,
            egui::TextEdit::singleline(&mut self.cfg.neeview_path)
                .frame(false)
                .font(FontId::proportional(FONT_UI))
                .text_color(text_color)
                .margin(egui::Margin::symmetric(0, 0)),
        );
        if resp.changed() {
            self.cfg.save();
        }

        let browse_rect = Rect::from_min_max(
            Pos2::new(l.opts.right() - 12.7 - browse_w, row_y),
            Pos2::new(l.opts.right() - 12.7, row_y + row_h),
        );
        if button(ui, browse_rect, BTN_BROWSE, true) {
            if let Some(fp) = rfd::FileDialog::new()
                .set_title("选择 NeeView.exe")
                .add_filter("程序", &["exe"])
                .add_filter("所有文件", &["*"])
                .pick_file()
            {
                self.cfg.neeview_path = fp.to_string_lossy().into_owned();
                self.cfg.save();
            }
        }
    }

    fn draw_log(&mut self, ui: &mut egui::Ui, l: &Layout) {
        let p = ui.painter();
        p.text(
            l.log_label,
            Align2::LEFT_CENTER,
            LOG_LABEL,
            FontId::proportional(FONT_UI),
            color::TEXT,
        );

        let outer = l.log;
        let sb_rect = Rect::from_min_max(Pos2::new(outer.right() - metrics::SB_W, outer.top()), outer.max);
        let view = Rect::from_min_max(outer.min, Pos2::new(sb_rect.left(), outer.bottom()));
        sunken(ui.painter(), outer);

        let inner = view.shrink2(Vec2::new(3.0, 2.0));
        let font = FontId::monospace(FONT_MONO);

        // 预排：得到每行 galley 与总高
        let mut lines: Vec<(Arc<egui::Galley>, Color32)> = Vec::with_capacity(self.logs.len());
        let mut total_h = 0.0f32;
        for line in &self.logs {
            let galley = ui
                .painter()
                .layout(line.text.clone(), font.clone(), line.tag.color(), inner.width());
            total_h += galley.size().y;
            lines.push((galley, line.tag.color()));
        }

        let view_h = inner.height();
        let max_off = (total_h - view_h).max(0.0);
        let wheel = wheel_delta(ui, outer);
        if wheel != 0.0 {
            self.log_offset -= wheel;
            self.log_stick = false;
        }
        if self.log_stick {
            self.log_offset = max_off;
        }
        self.log_offset = self.log_offset.clamp(0.0, max_off);
        if let Some(off) = scrollbar(ui, sb_rect, total_h, view_h, self.log_offset) {
            self.log_offset = off.clamp(0.0, max_off);
            self.log_stick = (max_off - self.log_offset).abs() < 0.5;
        }

        let bp = ui.painter().with_clip_rect(inner);
        let mut y = inner.top() - self.log_offset;
        for (galley, color) in lines {
            let h = galley.size().y;
            if y + h >= inner.top() && y <= inner.bottom() {
                bp.galley(Pos2::new(inner.left(), y), galley, color);
            }
            y += h;
        }
    }

    fn draw_ui(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let l = Layout::compute(rect);

        ui.painter().text(
            Pos2::new(l.left + 2.0, l.hint_cy),
            Align2::LEFT_CENTER,
            HINT_TEXT,
            FontId::proportional(FONT_UI),
            color::TEXT,
        );

        self.draw_file_list(ui, &l);

        // 按钮行：左三 + 右对齐「开始处理」
        let [b1, b2, b3] = l.buttons();
        if button(ui, b1, BTN_ADD, true) {
            if let Some(paths) = rfd::FileDialog::new()
                .set_title("选择 Kmoe 漫画包")
                .add_filter("漫画包", &["zip", "epub", "cbz"])
                .add_filter("所有文件", &["*"])
                .pick_files()
            {
                let n = collect_into(&mut self.files, paths);
                if n > 0 {
                    self.push_log(Tag::Info, format!("已添加 {n} 个文件"));
                }
            }
        }
        if button(ui, b2, BTN_REMOVE, true) && !self.sel.is_empty() {
            let n = self.sel.len();
            for &i in self.sel.iter().rev() {
                self.files.remove(i);
            }
            self.sel.clear();
            self.anchor = None;
            self.push_log(Tag::Info, format!("已移除 {n} 项"));
        }
        if button(ui, b3, BTN_CLEAR, true) {
            self.files.clear();
            self.sel.clear();
            self.anchor = None;
            self.push_log(Tag::Info, "已清空");
        }
        let start_rect = Rect::from_min_size(
            Pos2::new(l.right - 2.0 - metrics::BTN_W, l.btn_y),
            Vec2::new(metrics::BTN_W, metrics::BTN_H),
        );
        if button(ui, start_rect, BTN_START, !self.running) {
            self.start();
        }

        self.draw_options(ui, &l);

        let phase = if self.running { Some((self.phase / 1.6) % 1.0) } else { None };
        progressbar(ui.painter(), l.prog, phase);

        self.draw_log(ui, &l);
    }
}

impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_drop(ctx);
        self.pump(ctx);

        if self.autotest && !self.auto_started {
            self.auto_started = true;
            if !self.files.is_empty() {
                self.start();
            }
        }

        if self.running {
            self.phase += ctx.input(|i| i.stable_dt).min(0.1) * 1.6;
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        // 自检：Done 后延迟 1.2s 截图，确保最终日志已渲染
        if let Some(t0) = self.shot_wait_since {
            if t0.elapsed() >= Duration::from_millis(1200) {
                self.shot_wait_since = None;
                eprintln!("[shot] firing Screenshot command");
                if let Some((ctx2, _)) = &self.shot {
                    ctx2.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                }
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // 自检：收到 screenshot 事件后写 PNG 并退出（环境变量驱动，正常使用不受影响）
        if let Some(img) = self.take_screenshot(ctx) {
            if let Some((_, path)) = &self.shot {
                if let Err(e) = save_png(path, &img) {
                    eprintln!("GUI shot save failed: {e}");
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            self.shot = None;
        }

        // 自检：纯截图模式（无 AUTOTEST）启动后延迟 0.6s 截图，等首帧稳定
        if !self.autotest && self.shot.is_some() && self.shot_wait_since.is_none() {
            self.shot_wait_since = Some(std::time::Instant::now() - Duration::from_millis(600));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(color::BG))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                self.draw_ui(ui, rect);
            });
    }
}

/// 启动 GUI。`preload` 来自命令行 `--gui <文件…>`，装填进列表后不自动开始。
pub fn run_gui(preload: Vec<PathBuf>) -> eframe::Result<()> {
    let cfg = Config::load();
    let title = format!("{APP_NAME} v{} - Kmoe漫画包顺序修正", crate::VERSION);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 560.0])
            .with_min_inner_size([680.0, 520.0])
            .with_title(title.clone()),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            install_theme(&cc.egui_ctx);
            let mut gui = Gui { cfg, log_stick: true, ..Default::default() };
            if !preload.is_empty() {
                let n = collect_into(&mut gui.files, preload.clone());
                if n > 0 {
                    gui.push_log(Tag::Info, format!("已添加 {n} 个文件（命令行）"));
                }
            }
            gui.setup_shot(&cc.egui_ctx);
            gui.setup_autotest();
            Ok(Box::new(gui))
        }),
    )
}

/// 浅色主题 + Segoe UI / Consolas / 微软雅黑。
/// egui 默认字体不覆盖中文，缺了会在界面上渲染成方块。
fn install_theme(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::ZERO;
    style.spacing.button_padding = Vec2::ZERO;
    for (text_style, size) in [
        (egui::TextStyle::Body, FONT_UI),
        (egui::TextStyle::Button, FONT_UI),
        (egui::TextStyle::Small, FONT_UI - 1.0),
        (egui::TextStyle::Monospace, FONT_MONO),
    ] {
        style.text_styles.insert(text_style, FontId::proportional(size));
    }
    style.text_styles.insert(egui::TextStyle::Monospace, FontId::monospace(FONT_MONO));
    // 文本编辑无边框：边框由 sunken() 手绘
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    style.visuals.widgets.active.bg_stroke = Stroke::NONE;
    style.visuals.extreme_bg_color = color::FIELD_FACE;
    style.visuals.selection.bg_fill = color::SEL_BG;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, color::SEL_FG);
    ctx.set_style(style);

    let win_fonts = Path::new(&std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()))
        .join("Fonts");
    // (egui 字体键, 候选文件名) —— 西文/等宽走 Segoe UI / Consolas，中文回退微软雅黑
    let wanted: [(&str, &[&str]); 3] = [
        ("segoeui", &["segoeui.ttf", "segoeuib.ttf"]),
        ("consola", &["consola.ttf"]),
        ("msyh", &["msyh.ttc", "msyh.ttf"]),
    ];
    let mut fonts = egui::FontDefinitions::default();
    let mut loaded: Vec<&str> = Vec::new();
    for (key, candidates) in wanted {
        let Some(bytes) = candidates
            .iter()
            .map(|f| win_fonts.join(f))
            .find(|p| p.is_file())
            .and_then(|p| std::fs::read(p).ok())
        else {
            continue;
        };
        fonts.font_data.insert(key.to_owned(), egui::FontData::from_owned(bytes).into());
        loaded.push(key);
    }
    for key in loaded {
        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, key.to_owned());
        fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, key.to_owned());
    }
    ctx.set_fonts(fonts);
}

/// 把 egui 截图（RGBA）写成 PNG。无 png 依赖：写最小可行 PNG（无压缩）。
fn save_png(path: &str, img: &egui::ColorImage) -> Result<(), String> {
    let (w, h) = (img.size[0] as usize, img.size[1] as usize);
    eprintln!("[shot] image {}x{}, pixels={}", w, h, img.pixels.len());
    let expected = w.checked_mul(h).unwrap_or(0);
    if w == 0 || h == 0 || img.pixels.len() != expected {
        return Err(format!("bad image size {}x{} pixels={}", w, h, img.pixels.len()));
    }
    let raw: Vec<u8> = {
        let mut v = Vec::with_capacity(h * (1 + w * 4));
        for row in img.pixels.chunks_exact(w) {
            v.push(0);
            for p in row {
                v.extend_from_slice(&[p.r(), p.g(), p.b(), p.a()]);
            }
        }
        v
    };
    let mut raw_out = Vec::new();
    {
        use std::io::Write as _;
        let mut enc = flate2::write::ZlibEncoder::new(&mut raw_out, flate2::Compression::none());
        enc.write_all(&raw).map_err(|e| e.to_string())?;
        enc.finish().map_err(|e| e.to_string())?;
    }
    let comp = raw_out;

    let chunk = |tag: &[u8; 4], data: &[u8]| -> Vec<u8> {
        let mut c = Vec::new();
        c.extend_from_slice(&(data.len() as u32).to_be_bytes());
        c.extend_from_slice(tag);
        c.extend_from_slice(data);
        let mut h = crc32fast::Hasher::new();
        h.update(tag);
        h.update(data);
        c.extend_from_slice(&h.finalize().to_be_bytes());
        c
    };

    let mut out = Vec::new();
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let ihdr: Vec<u8> = {
        let mut v = Vec::new();
        v.extend_from_slice(&(w as u32).to_be_bytes());
        v.extend_from_slice(&(h as u32).to_be_bytes());
        v.push(8); // bit depth
        v.push(6); // color type RGBA
        v.push(0);
        v.push(0);
        v.push(0);
        v
    };
    out.extend_from_slice(&chunk(b"IHDR", &ihdr));
    out.extend_from_slice(&chunk(b"IDAT", &comp));
    out.extend_from_slice(&chunk(b"IEND", &[]));
    std::fs::write(path, &out).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_cover_config_maps_to_mode() {
        // 缺省（含空串）= 默认行为：按参照图自动回正
        let cfg = Config::default();
        assert_eq!(cfg.rotate_cover_mode(), crate::RotateCover::Auto, "缺省应自动回正");

        // 配置里显式写的角度 / off 仍然生效（GUI 无控件，只能手改配置文件）
        let cfg = Config { rotate_cover: "270".to_string(), ..Config::default() };
        assert_eq!(cfg.rotate_cover_mode(), crate::RotateCover::Fixed(270));
        let cfg = Config { rotate_cover: " off ".to_string(), ..Config::default() };
        assert_eq!(cfg.rotate_cover_mode(), crate::RotateCover::Off);

        // 非法取值按默认走，不打断处理流程
        let cfg = Config { rotate_cover: "xyz".to_string(), ..Config::default() };
        assert_eq!(cfg.rotate_cover_mode(), crate::RotateCover::Auto);
    }
}
