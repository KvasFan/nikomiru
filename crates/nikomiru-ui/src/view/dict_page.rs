// SPDX-License-Identifier: GPL-3.0-only

//! The dictionaries page (DIC-001, DIC-010, DIC-016, DIC-030…DIC-035): import
//! with progress and cancel, the update offer, and the dictionaries by type,
//! each with enable, rename, the popup block state, the user's CSS, order (by
//! drag and drop or ▲/▼) and delete. It covers the window and takes the
//! pointer, so nothing under it reacts; toasts stay above it.

use std::collections::HashMap;

use egui::{
    Align, Area, Button, Color32, ComboBox, DragAndDrop, FontId, Frame, Id, Key, LayerId, Layout,
    Margin, Order, ProgressBar, Rect, RichText, Role, ScrollArea, Sense, Stroke, StrokeKind,
    TextEdit, Ui, UiBuilder, WidgetInfo, vec2,
};
use nikomiru_dict::css::{Report, Sheet};
use nikomiru_library::{Collapse, DictionaryRecord};

use super::popups::Translate;
use super::{Dialog, View, theme};
use crate::dictionaries::{DictionaryPage, Group, Import, Member};
use crate::session::Session;
use crate::ui::Command;

const BACKDROP: Color32 = Color32::from_rgba_unmultiplied_const(0, 0, 0, 0xa6);
const CARD: Color32 = Color32::from_rgba_unmultiplied_const(0x20, 0x22, 0x27, 0xf0);
const CARD_BORDER: Color32 = Color32::from_rgba_unmultiplied_const(0xff, 0xff, 0xff, 0x40);
const ROW: Color32 = Color32::from_rgb(0x2a, 0x2d, 0x34);
const ROW_OFF: Color32 = Color32::from_rgb(0x1f, 0x21, 0x26);
const DETAILS: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xaa);
const OFF: Color32 = Color32::from_rgb(0x8a, 0x8f, 0x99);
/// The drop marker and the dragged dictionary's outline.
const DROP: Color32 = Color32::from_rgb(0x6a, 0xa0, 0xff);
/// What the CSS subset leaves out.
const LEFT_OUT: Color32 = Color32::from_rgb(0xf0, 0xb3, 0x5a);
/// The user's CSS is for a few rules: a long paste would be parsed by every
/// lookup.
const MAX_CSS_CHARS: usize = 32 * 1024;

/// What a drag carries (DIC-031): the dictionary, its name for the chip that
/// follows the pointer, and the group it moves in.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Dragged {
    id: String,
    name: String,
    group: Group,
}

/// A rename in progress.
#[derive(Debug)]
struct Rename {
    /// The dictionary.
    id: String,
    /// The text typed so far: it survives the page's refreshes.
    text: String,
    /// The field still has to take the keyboard.
    focus: bool,
    /// The field had the keyboard in the last frame.
    focused: bool,
}

/// The editor of a dictionary's CSS (DIC-035).
#[derive(Debug)]
struct CssEditor {
    /// The text typed so far: it survives the page's refreshes and a fold.
    text: String,
    /// Unfolded under the row.
    open: bool,
    /// What the subset keeps of `checked`, the text of the last check.
    report: Report,
    checked: Option<String>,
}

/// What the page keeps between frames; dropped when it closes.
#[derive(Debug, Default)]
pub(crate) struct State {
    renaming: Option<Rename>,
    /// The dictionary to delete once confirmed: its id and name.
    deleting: Option<(String, String)>,
    /// The CSS editors unfolded since the page opened, by dictionary.
    css: HashMap<String, CssEditor>,
}

/// What the page shows.
pub(crate) struct Scene<'a> {
    pub page: &'a DictionaryPage,
    pub tr: &'a Translate<'a>,
}

/// What the page asks of the view.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Asked {
    /// The file dialog for a dictionary.
    pub dialog: bool,
    /// Close the page.
    pub close: bool,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        page: &s.dict_page,
        tr: &tr,
    };
    let asked = draw(ctx, &scene, &mut view.dict, out);
    if asked.dialog {
        view.dialog = Some(Dialog::Dictionary);
    }
    if asked.close {
        view.close_dict_page();
    }
}

/// The size on disk: MB with a decimal from one mebibyte, kB below.
fn size(bytes: u64, tr: &Translate<'_>) -> String {
    const MIB: u64 = 1 << 20;
    if bytes >= MIB {
        let tenths = (bytes * 10 + MIB / 2) / MIB;
        tr(
            "dict-size-mb",
            &[
                ("whole", (tenths / 10).to_string()),
                ("tenth", (tenths % 10).to_string()),
            ],
        )
    } else {
        tr(
            "dict-size-kb",
            &[("size", ((bytes + 512) / 1024).max(1).to_string())],
        )
    }
}

/// The date of a Unix time in milliseconds, `YYYY-MM-DD` (UTC).
fn date(ms: i64) -> String {
    // days to civil date (Howard Hinnant's algorithm)
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The details line: what it contains, its size and when it was imported.
fn details(r: &DictionaryRecord, tr: &Translate<'_>) -> String {
    let mut parts: Vec<String> = [
        (r.counts.terms, "dict-type-terms"),
        (r.counts.kanji, "dict-type-kanji"),
        (
            r.counts.frequencies + r.counts.kanji_meta,
            "dict-type-frequency",
        ),
        (r.counts.pitches, "dict-type-pitch"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, label)| format!("{n} {}", tr(label, &[])))
    .collect();
    parts.push(size(r.size, tr));
    if r.imported_at > 0 {
        parts.push(date(r.imported_at));
    }
    parts.join(" · ")
}

/// The heading of a group.
fn group_name(group: Group, tr: &Translate<'_>) -> String {
    tr(&format!("dict-group-{}", group.name()), &[])
}

/// A dictionary listed under one of its other types: where it ranks in this
/// one; its row is under its main type.
fn reference(ui: &mut Ui, tr: &Translate<'_>, m: &Member<'_>) -> Rect {
    let home = group_name(Group::main(&m.record.kinds), tr);
    let text = tr(
        "dict-reference",
        &[("name", m.record.name().to_string()), ("group", home)],
    );
    Frame::new()
        .stroke(Stroke::new(1.0, ROW))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).italics().color(OFF)).truncate());
        })
        .response
        .rect
}

/// A row as a drop target of its group's drag (DIC-031): the gap a drop
/// lands in is above or below the row, by the half the pointer is over, and
/// a line marks it. `(i, from)`: the row's place and the dragged one's.
fn drop_zone(
    ui: &mut Ui,
    rect: Rect,
    (group, id): (Group, &str),
    (i, from): (usize, usize),
    out: &mut Vec<Command>,
) {
    let half_gap = ui.spacing().item_spacing.y / 2.0;
    // the gaps between the rows belong to the rows around them
    let zone = ui.interact(
        rect.expand2(vec2(0.0, half_gap)),
        Id::unique(("dict-drop", group.name(), id)),
        Sense::hover(),
    );
    let Some(pointer) = ui.ctx().pointer_latest_pos() else {
        return;
    };
    if zone.dnd_hover_payload::<Dragged>().is_none() {
        return;
    }
    let below = pointer.y >= rect.center().y;
    let to = i + usize::from(below);
    // next to itself: a drop would change nothing, so nothing is marked
    if to == from || to == from + 1 {
        return;
    }
    let y = if below {
        rect.bottom() + half_gap
    } else {
        rect.top() - half_gap
    };
    ui.painter()
        .hline(rect.x_range(), y, Stroke::new(2.0, DROP));
    if let Some(d) = zone.dnd_release_payload::<Dragged>() {
        out.push(Command::DictMoveTo(d.id.clone(), to));
    }
}

/// While a dictionary is dragged: its name follows the pointer, and the list
/// scrolls when the pointer nears its top or bottom edge.
fn follow_drag(ui: &mut Ui, d: &Dragged) {
    let Some(pointer) = ui.ctx().pointer_latest_pos() else {
        return;
    };
    let painter = ui
        .ctx()
        .layer_painter(LayerId::new(Order::Tooltip, Id::unique("dict-drag-chip")));
    let galley = painter.layout_no_wrap(d.name.clone(), FontId::proportional(15.0), Color32::WHITE);
    let chip = Rect::from_min_size(
        pointer + vec2(14.0, -galley.size().y / 2.0 - 4.0),
        galley.size() + vec2(16.0, 8.0),
    );
    painter.rect(chip, 6, ROW, Stroke::new(1.0, DROP), StrokeKind::Outside);
    painter.galley(chip.min + vec2(8.0, 4.0), galley, Color32::WHITE);

    let view = ui.clip_rect();
    const EDGE: f32 = 32.0;
    let speed = 600.0 * ui.input(|i| i.stable_dt).min(0.1);
    let step = if (view.top() - EDGE..view.top() + EDGE).contains(&pointer.y) {
        speed
    } else if (view.bottom() - EDGE..view.bottom() + EDGE).contains(&pointer.y) {
        -speed
    } else {
        0.0
    };
    if step != 0.0 && view.x_range().contains(pointer.x) {
        ui.scroll_with_delta_animation(vec2(0.0, step), egui::style::ScrollAnimation::none());
        ui.ctx().request_repaint();
    }
}

/// A flat button with a glyph, its tooltip and accessible name.
fn glyph(ui: &mut Ui, enabled: bool, glyph: &str, name: &str) -> bool {
    let response = ui
        .add_enabled(
            enabled,
            Button::new(RichText::new(glyph).size(15.0))
                .frame(false)
                .min_size(vec2(34.0, 32.0)),
        )
        .on_hover_text(name);
    let label = name.to_string();
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, &label));
    response.clicked()
}

pub(crate) fn draw(
    ctx: &egui::Context,
    scene: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> Asked {
    let tr = |id: &str| (scene.tr)(id, &[]);
    let page = scene.page;
    let screen = ctx.content_rect();
    let mut asked = Asked::default();
    let width = 780.0_f32.min(screen.width() - 32.0).max(0.0);
    let height = 640.0_f32.min(screen.height() - 32.0).max(0.0);

    // one layer for the backdrop and the card: it comes on top when it
    // appears, and its backdrop takes every click and the wheel
    full_screen(ctx, "dict-page", BACKDROP, |ui| {
        let card = Rect::from_center_size(screen.center(), vec2(width, height));
        let mut ui = ui.new_child(
            UiBuilder::new()
                .max_rect(card)
                .layout(Layout::top_down(Align::Min)),
        );
        Frame::new()
            .fill(CARD)
            .stroke(Stroke::new(1.0, CARD_BORDER))
            .corner_radius(10)
            .inner_margin(Margin::same(16))
            .show(&mut ui, |ui| {
                ui.set_width((width - 34.0).max(0.0));
                ui.set_height((height - 34.0).max(0.0));
                ui.spacing_mut().item_spacing.y = 12.0;
                ui.visuals_mut().override_text_color = Some(Color32::WHITE);

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tr("dict-page-title"))
                            .size(22.0)
                            .family(theme::bold()),
                    );
                    let idle = matches!(page.import, Import::Idle);
                    if ui
                        .add_enabled(idle, Button::new(tr("dict-import")))
                        .clicked()
                    {
                        asked.dialog = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if glyph(ui, true, "✕", &tr("dict-close")) {
                            asked.close = true;
                        }
                    });
                });

                import_block(ui, page, &tr, out);

                if page.rows.is_empty() {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        ui.add(
                            egui::Label::new(RichText::new(tr("dict-empty")).color(DETAILS)).wrap(),
                        );
                    });
                    return;
                }
                ScrollArea::vertical()
                    .id_salt("dict-page-rows")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        let dragged = DragAndDrop::payload::<Dragged>(ui.ctx());
                        if let Some(d) = &dragged {
                            follow_drag(ui, d);
                        }
                        for group in Group::ALL {
                            let rows = page.group(group);
                            let Some(last) = rows.len().checked_sub(1) else {
                                continue;
                            };
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(group_name(group, scene.tr))
                                    .size(14.0)
                                    .family(theme::bold())
                                    .color(DETAILS),
                            );
                            // the rows of the dragged dictionary's group are its
                            // drop targets
                            let from = dragged
                                .as_deref()
                                .filter(|d| d.group == group)
                                .and_then(|d| rows.iter().position(|m| m.id == d.id));
                            for (i, m) in rows.iter().enumerate() {
                                let rect = if m.reference {
                                    reference(ui, scene.tr, m)
                                } else {
                                    let place = Place {
                                        group,
                                        up: i > 0,
                                        down: i < last,
                                        dragged: from == Some(i),
                                    };
                                    row(ui, scene.tr, state, out, m, place)
                                };
                                if let Some(from) = from {
                                    drop_zone(ui, rect, (group, m.id), (i, from), out);
                                }
                            }
                        }
                    });
            });
    });

    if let Some((id, name)) = state.deleting.clone() {
        confirm_delete(ctx, &tr, state, out, &id, &name);
    }
    asked
}

/// A layer over the whole window, shaded with `shade`, that takes every
/// click: what is under it does not react.
fn full_screen(ctx: &egui::Context, id: &str, shade: Color32, content: impl FnOnce(&mut Ui)) {
    let screen = ctx.content_rect();
    Area::new(Id::unique(id))
        .order(Order::Foreground)
        .fade_in(false)
        .constrain(false)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(rect, 0.0, shade);
            content(ui);
        });
}

/// The running import, or the offer to update an installed dictionary.
fn import_block(
    ui: &mut Ui,
    page: &DictionaryPage,
    tr: &dyn Fn(&str) -> String,
    out: &mut Vec<Command>,
) {
    match &page.import {
        Import::Idle => {}
        Import::Running { file, progress, .. } => {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.add(egui::Label::new(format!("{}: {file}", tr("dict-importing"))).truncate());
                ui.horizontal(|ui| {
                    if progress.files_total == 0 {
                        ui.spinner();
                    } else {
                        let done = progress.files_done as f32 / progress.files_total.max(1) as f32;
                        ui.add(ProgressBar::new(done).desired_width(ui.available_width() - 120.0));
                    }
                    if ui.button(tr("dict-cancel")).clicked() {
                        out.push(Command::DictCancel);
                    }
                });
                ui.label(
                    RichText::new(format!("{} {}", progress.entries, tr("dict-entries")))
                        .color(DETAILS),
                );
            });
        }
        Import::Confirm {
            title,
            installed,
            zip,
            ..
        } => {
            let file = zip
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.add(egui::Label::new(tr("dict-update-offer")).wrap());
                ui.label(
                    RichText::new(format!(
                        "{title} — {} {installed} → {file}",
                        tr("dict-revision")
                    ))
                    .color(DETAILS),
                );
                ui.horizontal(|ui| {
                    if ui.button(tr("dict-update")).clicked() {
                        out.push(Command::DictConfirm(true));
                    }
                    if ui.button(tr("dict-keep")).clicked() {
                        out.push(Command::DictConfirm(false));
                    }
                });
            });
        }
    }
}

/// Where a dictionary's row is in its group.
#[derive(Debug, Clone, Copy)]
struct Place {
    group: Group,
    /// ▲ can move it.
    up: bool,
    /// ▼ can move it.
    down: bool,
    /// It is being dragged.
    dragged: bool,
}

/// The handle a drag starts from (DIC-031): six dots, the whole height of
/// the row's first line.
fn drag_handle(ui: &mut Ui, tr: &Translate<'_>, m: &Member<'_>, place: Place) {
    let (rect, _) = ui.allocate_exact_size(vec2(12.0, 30.0), Sense::hover());
    let handle = ui.interact(rect, Id::unique(("dict-drag", m.id)), Sense::drag());
    handle.dnd_set_drag_payload(Dragged {
        id: m.id.to_string(),
        name: m.record.name().to_string(),
        group: place.group,
    });
    let color = if handle.hovered() || place.dragged {
        Color32::WHITE
    } else {
        OFF
    };
    for dx in [-2.5, 2.5] {
        for dy in [-5.0, 0.0, 5.0] {
            ui.painter()
                .circle_filled(rect.center() + vec2(dx, dy), 1.5, color);
        }
    }
    let name = tr("dict-drag", &[]);
    handle.clone().on_hover_text(&name);
    handle.widget_info(|| WidgetInfo::labeled(Role::Image, true, &name));
}

fn row(
    ui: &mut Ui,
    tr: &Translate<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
    m: &Member<'_>,
    place: Place,
) -> Rect {
    let t = |key: &str| tr(key, &[]);
    let (id, r) = (m.id, m.record);
    let (up, down) = (place.up, place.down);
    let mut frame = Frame::new()
        .fill(if r.enabled { ROW } else { ROW_OFF })
        .corner_radius(6)
        .inner_margin(Margin::symmetric(10, 8));
    if place.dragged {
        frame = frame.stroke(Stroke::new(1.0, DROP));
    }
    frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if place.dragged {
                ui.multiply_opacity(0.5);
            }
            ui.horizontal(|ui| {
                drag_handle(ui, tr, m, place);
                let mut enabled = r.enabled;
                let check = ui
                    .checkbox(&mut enabled, "")
                    .on_hover_text(t("dict-enabled"));
                let name = t("dict-enabled");
                check.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, r.enabled, &name));
                if check.changed() {
                    out.push(Command::DictSetEnabled(id.to_string(), enabled));
                }

                // name and details, or the rename field; then, from the right,
                // four glyphs, the CSS button and the block state menu
                let buttons = 4.0 * 34.0 + CSS_BUTTON + MENU + 6.0 * ui.spacing().item_spacing.x;
                let room = (ui.available_width() - buttons).max(80.0);
                let mut renamed = false;
                ui.vertical(|ui| {
                    ui.set_width(room);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    match &mut state.renaming {
                        Some(rename) if rename.id == id => {
                            renamed = rename_field(ui, r, rename, out);
                        }
                        _ => {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(r.name())
                                            .size(16.0)
                                            .family(theme::bold())
                                            .color(if r.enabled { Color32::WHITE } else { OFF }),
                                    )
                                    .truncate(),
                                );
                                let mut about = format!("{} {}", t("dict-revision"), r.revision);
                                if let Some(code) = r.language.as_deref().filter(|c| !c.is_empty())
                                {
                                    about.push_str(" · ");
                                    about.push_str(&tr("dict-language", &[("code", code.into())]));
                                }
                                ui.label(RichText::new(about).color(DETAILS));
                            });
                        }
                    }
                    ui.add(
                        egui::Label::new(RichText::new(details(r, tr)).size(13.0).color(DETAILS))
                            .truncate(),
                    );
                });
                if renamed {
                    state.renaming = None;
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if glyph(ui, true, "🗑", &t("dict-delete")) {
                        state.deleting = Some((id.to_string(), r.name().to_string()));
                    }
                    if glyph(ui, true, "✎", &t("dict-rename")) {
                        state.renaming = match &state.renaming {
                            Some(rename) if rename.id == id => None,
                            _ => Some(Rename {
                                id: id.to_string(),
                                text: r.display_name.clone().unwrap_or_default(),
                                focus: true,
                                focused: false,
                            }),
                        };
                    }
                    // the user's CSS, for the glossaries of term dictionaries (DIC-035)
                    if r.kinds.terms {
                        let open = state.css.get(id).is_some_and(|e| e.open);
                        if css_toggle(ui, open, &t("dict-css")) {
                            let editor =
                                state
                                    .css
                                    .entry(id.to_string())
                                    .or_insert_with(|| CssEditor {
                                        text: r.user_css.clone(),
                                        open: false,
                                        report: Report::default(),
                                        checked: None,
                                    });
                            editor.open = !editor.open;
                        }
                    } else {
                        ui.add_space(CSS_BUTTON + ui.spacing().item_spacing.x);
                    }
                    if glyph(ui, down, "▼", &t("dict-move-down")) {
                        out.push(Command::DictMove(id.to_string(), 1));
                    }
                    if glyph(ui, up, "▲", &t("dict-move-up")) {
                        out.push(Command::DictMove(id.to_string(), -1));
                    }
                    // the popup block state, for term dictionaries (DIC-034)
                    if r.kinds.terms {
                        let options = [
                            (
                                Collapse::Automatic,
                                "automatic",
                                t("dict-collapse-automatic"),
                            ),
                            (Collapse::Expanded, "expanded", t("dict-collapse-expanded")),
                            (
                                Collapse::Collapsed,
                                "collapsed",
                                t("dict-collapse-collapsed"),
                            ),
                        ];
                        let current = options
                            .iter()
                            .find(|(c, _, _)| *c == r.collapse)
                            .map(|(_, _, label)| label.clone())
                            .unwrap_or_default();
                        let combo = ComboBox::from_id_salt(("dict-collapse", id))
                            .width(MENU)
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for (c, key, label) in &options {
                                    if ui.selectable_label(*c == r.collapse, label).clicked() {
                                        out.push(Command::DictSetCollapse(
                                            id.to_string(),
                                            (*key).to_string(),
                                        ));
                                    }
                                }
                            });
                        let name = t("dict-collapse");
                        combo.response.clone().on_hover_text(&name);
                        combo
                            .response
                            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &name));
                    } else {
                        ui.add_space(MENU + ui.spacing().item_spacing.x);
                    }
                });
            });
            if r.kinds.terms
                && let Some(editor) = state.css.get_mut(id).filter(|e| e.open)
            {
                css_panel(ui, tr, m, editor, out);
            }
        })
        .response
        .rect
}

/// The width of the button that unfolds the CSS editor.
const CSS_BUTTON: f32 = 44.0;
/// The width of the block state menu.
const MENU: f32 = 130.0;

/// The button that unfolds and folds a dictionary's CSS editor.
fn css_toggle(ui: &mut Ui, open: bool, name: &str) -> bool {
    let response = ui
        .add(
            Button::selectable(open, RichText::new("CSS").size(13.0))
                .min_size(vec2(CSS_BUTTON, 28.0)),
        )
        .on_hover_text(name);
    let label = name.to_string();
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, open, &label));
    response.clicked()
}

/// Up to six of `items`, each once, for a line of the CSS feedback.
fn listed(items: &[String]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for item in items {
        if !seen.contains(&item.as_str()) {
            seen.push(item);
        }
    }
    let more = seen.len() > 6;
    seen.truncate(6);
    let mut text = seen.join(", ");
    if more {
        text.push_str(", …");
    }
    text
}

/// The editor of a dictionary's CSS (DIC-035), applied after its own
/// `styles.css` through the same safe subset: a code field (it takes the
/// keyboard and the input method like every text field), Save, Revert and
/// Clear, and what the subset keeps of what is typed.
fn css_panel(
    ui: &mut Ui,
    tr: &Translate<'_>,
    m: &Member<'_>,
    editor: &mut CssEditor,
    out: &mut Vec<Command>,
) {
    let t = |key: &str| tr(key, &[]);
    let (id, r) = (m.id, m.record);
    ui.add_space(6.0);
    ui.add(egui::Label::new(RichText::new(t("dict-css-intro")).size(13.0).color(DETAILS)).wrap());
    let name = tr("dict-css-field", &[("title", r.name().to_string())]);
    ScrollArea::vertical()
        .id_salt(("dict-css", id))
        .max_height(180.0)
        .show(ui, |ui| {
            let field = ui.add(
                TextEdit::multiline(&mut editor.text)
                    .code_editor()
                    .desired_rows(5)
                    .desired_width(f32::INFINITY)
                    .char_limit(MAX_CSS_CHARS)
                    .hint_text(r#"span[data-sc-class="tag"] { color: #8ab4ff }"#),
            );
            field.widget_info(|| WidgetInfo::labeled(Role::MultilineTextInput, true, &name));
        });

    if editor.checked.as_ref() != Some(&editor.text) {
        editor.report = Sheet::check(&editor.text);
        editor.checked = Some(editor.text.clone());
    }
    let report = &editor.report;
    let small =
        |text: String, color| egui::Label::new(RichText::new(text).size(13.0).color(color)).wrap();
    ui.add(small(
        tr("dict-css-rules", &[("count", report.rules.to_string())]),
        DETAILS,
    ));
    if !report.dropped.is_empty() {
        let list = listed(&report.dropped);
        ui.add(small(tr("dict-css-dropped", &[("list", list)]), LEFT_OUT));
    }
    if !report.ignored.is_empty() {
        let list = listed(&report.ignored);
        ui.add(small(tr("dict-css-ignored", &[("list", list)]), LEFT_OUT));
    }

    let unsaved = editor.text != r.user_css;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(unsaved, Button::new(t("dict-css-save")))
            .clicked()
        {
            out.push(Command::DictSetUserCss(id.to_string(), editor.text.clone()));
        }
        if ui
            .add_enabled(unsaved, Button::new(t("dict-css-revert")))
            .clicked()
        {
            editor.text.clone_from(&r.user_css);
        }
        if ui
            .add_enabled(!editor.text.is_empty(), Button::new(t("dict-css-clear")))
            .clicked()
        {
            editor.text.clear();
        }
        if unsaved {
            ui.label(RichText::new(t("dict-css-unsaved")).color(LEFT_OUT));
        }
    });
}

/// The rename field: Enter renames (an empty name restores the title), Esc
/// cancels without closing the page. Returns whether the rename is over.
fn rename_field(
    ui: &mut Ui,
    r: &DictionaryRecord,
    rename: &mut Rename,
    out: &mut Vec<Command>,
) -> bool {
    let escape = ui.input(|i| i.key_pressed(Key::Escape));
    let field = ui.add(
        TextEdit::singleline(&mut rename.text)
            .hint_text(&r.title)
            .desired_width(f32::INFINITY),
    );
    let name = r.title.clone();
    field.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &name));
    if std::mem::take(&mut rename.focus) {
        field.request_focus();
    }
    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    if enter {
        out.push(Command::DictRename(
            rename.id.clone(),
            rename.text.trim().to_string(),
        ));
    }
    // egui takes the keyboard from the field on Esc; the page stays
    let cancelled = rename.focused && escape && !field.has_focus();
    rename.focused = field.has_focus();
    enter || cancelled
}

/// Asks before deleting: the name as text, Delete and Cancel.
fn confirm_delete(
    ctx: &egui::Context,
    tr: &dyn Fn(&str) -> String,
    state: &mut State,
    out: &mut Vec<Command>,
    id: &str,
    name: &str,
) {
    let screen = ctx.content_rect();
    full_screen(ctx, "dict-delete", Color32::from_black_alpha(0x60), |ui| {
        let size = vec2(420.0_f32.min(screen.width() - 64.0).max(0.0), 0.0);
        let rect = Rect::from_center_size(screen.center(), size).expand2(vec2(0.0, 60.0));
        let mut ui = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::Center)),
        );
        Frame::new()
            .fill(Color32::from_rgb(0x2a, 0x2d, 0x34))
            .stroke(Stroke::new(1.0, CARD_BORDER))
            .corner_radius(8)
            .inner_margin(Margin::same(16))
            .show(&mut ui, |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                ui.visuals_mut().override_text_color = Some(Color32::WHITE);
                ui.add(
                    egui::Label::new(
                        RichText::new(tr("dict-delete-confirm")).family(theme::bold()),
                    )
                    .wrap(),
                );
                ui.add(egui::Label::new(name).wrap());
                ui.horizontal(|ui| {
                    if ui.button(tr("dict-delete")).clicked() {
                        out.push(Command::DictDelete(id.to_string()));
                        state.deleting = None;
                    }
                    if ui.button(tr("dict-cancel")).clicked() {
                        state.deleting = None;
                    }
                });
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, pos2};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use nikomiru_dict::yomitan::Kinds;

    struct Test {
        page: DictionaryPage,
        state: State,
        out: Vec<Command>,
        asked: Asked,
    }

    fn record(title: &str, enabled: bool) -> DictionaryRecord {
        DictionaryRecord {
            title: title.into(),
            revision: "2".into(),
            kinds: Kinds {
                terms: true,
                ..Kinds::default()
            },
            order: 0,
            enabled,
            language: Some("ja".into()),
            display_name: None,
            collapse: Collapse::Automatic,
            user_css: String::new(),
            counts: nikomiru_dict::store::Counts {
                terms: 1234,
                ..Default::default()
            },
            imported_at: 1_792_454_400_000,
            size: 2 << 20,
        }
    }

    fn harness(page: DictionaryPage) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(1280.0, 720.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let video = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    if video.clicked() {
                        t.out.push(Command::VideoClicked);
                    }
                    let tr = |id: &str, args: &[(&str, String)]| {
                        let mut text = id.to_string();
                        for (k, v) in args {
                            text.push_str(&format!(" {k}={v}"));
                        }
                        text
                    };
                    let scene = Scene {
                        page: &t.page,
                        tr: &tr,
                    };
                    let asked = draw(ui.ctx(), &scene, &mut t.state, &mut t.out);
                    t.asked.dialog |= asked.dialog;
                    t.asked.close |= asked.close;
                },
                Test {
                    page,
                    state: State::default(),
                    out: Vec::new(),
                    asked: Asked::default(),
                },
            )
    }

    fn page(rows: &[(&str, DictionaryRecord)]) -> DictionaryPage {
        let mut page = DictionaryPage::default();
        page.rows = rows
            .iter()
            .map(|(id, r)| (id.to_string(), r.clone()))
            .collect();
        page
    }

    fn click(h: &mut Harness<'_, Test>, label: &str, nth: usize) -> Vec<Command> {
        let nodes: Vec<_> = h.query_all_by_label(label).collect();
        let at = nodes[nth].rect().center();
        h.hover_at(at);
        h.drag_at(at);
        h.drop_at(at);
        h.run();
        std::mem::take(&mut h.state_mut().out)
    }

    /// The height of the drop marker in the last frame, if one is drawn.
    fn marker(h: &Harness<'_, Test>) -> Option<f32> {
        h.output().shapes.iter().find_map(|c| match &c.shape {
            egui::Shape::LineSegment { points, stroke } if stroke.color == DROP => {
                Some(points[0].y)
            }
            _ => None,
        })
    }

    /// Drags the handle of row `nth` to `to`: the marker over the target
    /// before the release, and the commands.
    fn drag(h: &mut Harness<'_, Test>, nth: usize, to: Pos2) -> (Option<f32>, Vec<Command>) {
        let handles: Vec<_> = h.query_all_by_label("dict-drag").collect();
        let from = handles[nth].rect().center();
        h.hover_at(from);
        h.drag_at(from);
        // past the distance of a click: a drag
        h.hover_at(from + vec2(0.0, 12.0));
        h.hover_at(to);
        h.run();
        let marker = marker(h);
        h.drop_at(to);
        h.run();
        (marker, std::mem::take(&mut h.state_mut().out))
    }

    #[test]
    fn sizes_and_dates_are_written_out() {
        let tr = |id: &str, args: &[(&str, String)]| {
            let get = |k: &str| {
                args.iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            };
            match id {
                "dict-size-mb" => format!("{}.{} MB", get("whole"), get("tenth")),
                "dict-size-kb" => format!("{} kB", get("size")),
                other => other.to_string(),
            }
        };
        assert_eq!(size(1, &tr), "1 kB");
        assert_eq!(size(300 * 1024 + 700, &tr), "301 kB");
        assert_eq!(size(3 << 19, &tr), "1.5 MB");
        assert_eq!(size(261 << 20, &tr), "261.0 MB");
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_792_454_400_000), "2026-10-20");
        assert_eq!(date(951_782_400_000), "2000-02-29");
        assert_eq!(
            details(&record("JM", true), &tr),
            "1234 dict-type-terms · 2.0 MB · 2026-10-20"
        );
    }

    // T-DIC-030
    #[test]
    fn dictionaries_are_listed_by_type_with_their_details() {
        use nikomiru_dict::store::Counts;
        let typed = |title: &str, kinds: Kinds, counts: Counts| DictionaryRecord {
            kinds,
            counts,
            ..record(title, true)
        };
        let mut jm = record("JM", true);
        jm.kinds.frequency = true;
        jm.counts.frequencies = 50;
        let mut kanjidic = typed(
            "KANJIDIC",
            Kinds {
                kanji: true,
                ..Kinds::default()
            },
            Counts {
                kanji: 13108,
                ..Counts::default()
            },
        );
        kanjidic.language = None;
        let nhk = typed(
            "NHK",
            Kinds {
                pitch: true,
                ..Kinds::default()
            },
            Counts {
                pitches: 99,
                ..Counts::default()
            },
        );
        let jpdb = typed(
            "JPDB",
            Kinds {
                frequency: true,
                ..Kinds::default()
            },
            Counts {
                frequencies: 900,
                ..Counts::default()
            },
        );
        let mut h = harness(page(&[("p", nhk), ("f", jpdb), ("a", jm), ("k", kanjidic)]));
        h.run();
        // the headings in the page's order, each over its dictionaries; the
        // term dictionary's frequencies rank after JPDB's, and say where its
        // row is
        let column = [
            "dict-group-terms",
            "JM",
            "dict-group-kanji",
            "KANJIDIC",
            "dict-group-frequency",
            "JPDB",
            "dict-reference name=JM group=dict-group-terms",
            "dict-group-pitch",
            "NHK",
        ];
        let top = |label: &str| h.get_by_label(label).rect().top();
        for pair in column.windows(2) {
            assert!(top(pair[0]) < top(pair[1]), "{} over {}", pair[0], pair[1]);
        }
        // title, revision and language; entries, size and import date
        let about = "dict-revision 2 · dict-language code=ja";
        assert_eq!(h.query_all_by_label(about).count(), 3);
        assert!(h.query_by_label("dict-revision 2").is_some(), "no language");
        let size = "dict-size-mb whole=2 tenth=0";
        let jm = format!("1234 dict-type-terms · 50 dict-type-frequency · {size} · 2026-10-20");
        assert!(h.query_by_label(&jm).is_some(), "{jm}");
        let kanji = format!("13108 dict-type-kanji · {size} · 2026-10-20");
        assert!(h.query_by_label(&kanji).is_some(), "{kanji}");
        // a reference has no controls: one switch per dictionary
        assert_eq!(h.query_all_by_label("dict-enabled").count(), 4);
    }

    // T-DIC-031
    #[test]
    fn a_row_dragged_by_its_handle_drops_into_a_gap_of_its_group() {
        let freq = DictionaryRecord {
            kinds: Kinds {
                frequency: true,
                ..Kinds::default()
            },
            ..record("F", true)
        };
        // B gives frequencies too: a reference in F's group
        let mut b = record("B", true);
        b.kinds.frequency = true;
        let mut h = harness(page(&[
            ("a", record("A", true)),
            ("b", b),
            ("c", record("C", true)),
            ("f", freq),
        ]));
        h.run();
        assert_eq!(
            h.query_all_by_label("dict-drag").count(),
            4,
            "none on a reference"
        );
        // the upper half of a row is its name, the lower half its details
        let upper = |h: &Harness<'_, Test>, name: &str| h.get_by_label(name).rect();
        let details = "1234 dict-type-terms · dict-size-mb whole=2 tenth=0 · 2026-10-20";
        let lower = |h: &Harness<'_, Test>, nth: usize| {
            let all: Vec<_> = h.query_all_by_label(details).collect();
            all[nth].rect()
        };

        // C over the upper half of A: it goes above A, and the marker says so
        let a = upper(&h, "A");
        let (line, out) = drag(&mut h, 2, a.center());
        assert!(line.is_some_and(|y| y < a.top()), "{line:?} {a:?}");
        assert!(
            matches!(&out[..], [Command::DictMoveTo(id, 0)] if id == "c"),
            "{out:?}"
        );
        // A over the lower half of B: between B and C
        let b = lower(&h, 1);
        let (line, out) = drag(&mut h, 0, b.center());
        assert!(line.is_some_and(|y| y > b.bottom()), "{line:?} {b:?}");
        assert!(
            matches!(&out[..], [Command::DictMoveTo(id, 2)] if id == "a"),
            "{out:?}"
        );
        // B next to where it is: no marker, nothing sent
        let a_below = lower(&h, 0).center();
        let c_above = upper(&h, "C").center();
        for to in [a_below, c_above] {
            let (line, out) = drag(&mut h, 1, to);
            assert!(line.is_none() && out.is_empty(), "{line:?} {out:?}");
        }
        // another group is not a target
        let f = upper(&h, "F").center();
        let (line, out) = drag(&mut h, 0, f);
        assert!(line.is_none() && out.is_empty(), "{line:?} {out:?}");
        // a reference is: F above B's frequencies
        let reference = h
            .get_by_label("dict-reference name=B group=dict-group-terms")
            .rect();
        let (line, out) = drag(&mut h, 3, reference.center_top() + vec2(0.0, 2.0));
        assert!(line.is_some_and(|y| y < reference.top()), "{line:?}");
        assert!(
            matches!(&out[..], [Command::DictMoveTo(id, 0)] if id == "f"),
            "{out:?}"
        );
        // a click on a handle is not a drag
        let out = click(&mut h, "dict-drag", 0);
        assert!(out.is_empty(), "{out:?}");
    }

    // T-DIC-035
    #[test]
    fn the_css_editor_says_what_applies_and_saves_reverts_and_clears() {
        let mut jm = record("JM", true);
        jm.user_css = "div { margin: 1em }".into();
        let freq = DictionaryRecord {
            kinds: Kinds {
                frequency: true,
                ..Kinds::default()
            },
            ..record("F", true)
        };
        let mut h = harness(page(&[("a", jm), ("f", freq)]));
        h.run();
        // a term dictionary's: its glossaries are what it styles
        assert_eq!(h.query_all_by_label("dict-css").count(), 1);
        let field = "dict-css-field title=JM";
        assert!(h.query_by_label(field).is_none(), "folded");
        click(&mut h, "dict-css", 0);
        let saved = h.get_by_label(field).value();
        assert_eq!(saved.as_deref(), Some("div { margin: 1em }"));
        assert!(h.query_by_label("dict-css-rules count=1").is_some());
        assert!(h.query_by_label("dict-css-unsaved").is_none());

        // Clear, then type: the feedback follows the text
        click(&mut h, "dict-css-clear", 0);
        assert!(h.query_by_label("dict-css-rules count=0").is_some());
        h.get_by_label(field).focus();
        h.run();
        let css =
            "span { color: red; position: absolute } a:hover { color: blue } ul { cursor: x }";
        h.get_by_label(field).type_text(css);
        h.run();
        assert_eq!(h.get_by_label(field).value().as_deref(), Some(css));
        assert!(h.query_by_label("dict-css-rules count=1").is_some());
        assert!(
            h.query_by_label("dict-css-dropped list=position, cursor")
                .is_some()
        );
        assert!(h.query_by_label("dict-css-ignored list=a:hover").is_some());
        assert!(h.query_by_label("dict-css-unsaved").is_some());

        // Save sends the text; folding keeps the draft
        let out = click(&mut h, "dict-css-save", 0);
        assert!(
            matches!(&out[..], [Command::DictSetUserCss(id, text)] if id == "a" && text == css),
            "{out:?}"
        );
        click(&mut h, "dict-css", 0);
        assert!(h.query_by_label(field).is_none());
        click(&mut h, "dict-css", 0);
        assert_eq!(h.get_by_label(field).value().as_deref(), Some(css));
        // Revert goes back to the saved CSS
        click(&mut h, "dict-css-revert", 0);
        let reverted = h.get_by_label(field).value();
        assert_eq!(reverted.as_deref(), Some("div { margin: 1em }"));
        assert!(h.query_by_label("dict-css-unsaved").is_none());
        // once the page reads the saved text back, nothing is left to save
        click(&mut h, "dict-css-clear", 0);
        h.state_mut().page.rows[0].1.user_css.clear();
        h.run();
        assert!(h.query_by_label("dict-css-unsaved").is_none());
        let out = click(&mut h, "dict-css-save", 0);
        assert!(out.is_empty(), "{out:?}");
    }

    // T-DIC-031, T-DIC-032
    #[test]
    fn rows_reorder_and_enable_and_the_page_takes_the_clicks() {
        let mut h = harness(page(&[
            ("a", record("JM", true)),
            ("b", record("Freq", false)),
        ]));
        h.run();
        assert!(h.query_by_label("JM").is_some() && h.query_by_label("Freq").is_some());
        // ▲ on the first row is disabled, on the second it moves it up
        let out = click(&mut h, "dict-move-up", 0);
        assert!(out.is_empty(), "{out:?}");
        let out = click(&mut h, "dict-move-up", 1);
        assert!(
            matches!(&out[..], [Command::DictMove(id, -1)] if id == "b"),
            "{out:?}"
        );
        let out = click(&mut h, "dict-move-down", 0);
        assert!(
            matches!(&out[..], [Command::DictMove(id, 1)] if id == "a"),
            "{out:?}"
        );
        let out = click(&mut h, "dict-enabled", 1);
        assert!(
            matches!(&out[..], [Command::DictSetEnabled(id, true)] if id == "b"),
            "{out:?}"
        );
        // the backdrop takes clicks: the video does not play or pause
        h.hover_at(pos2(5.0, 5.0));
        h.drag_at(pos2(5.0, 5.0));
        h.drop_at(pos2(5.0, 5.0));
        h.run();
        assert!(h.state().out.is_empty(), "{:?}", h.state().out);
        click(&mut h, "dict-close", 0);
        assert!(h.state().asked.close);
        click(&mut h, "dict-import", 0);
        assert!(h.state().asked.dialog);
    }

    // T-DIC-034
    #[test]
    fn the_popup_block_state_is_chosen_from_its_menu() {
        let mut h = harness(page(&[("a", record("JM", true))]));
        h.run();
        // the menu opens over the page, and its choice is sent
        click(&mut h, "dict-collapse", 0);
        let out = click(&mut h, "dict-collapse-expanded", 0);
        assert!(
            matches!(&out[..], [Command::DictSetCollapse(id, state)] if id == "a" && state == "expanded"),
            "{out:?}"
        );
    }

    // T-DIC-033
    #[test]
    fn rename_commits_with_enter_and_escape_keeps_the_page() {
        let mut h = harness(page(&[("a", record("JM", true))]));
        h.run();
        click(&mut h, "dict-rename", 0);
        assert!(h.query_by_label("JM").is_some(), "the title is the hint");
        h.event(egui::Event::Text("My JM".into()));
        h.run();
        h.key_press(Key::Enter);
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(
            matches!(&out[..], [Command::DictRename(id, name)] if id == "a" && name == "My JM"),
            "{out:?}"
        );
        assert!(h.state().state.renaming.is_none());
        // Esc cancels the rename only
        click(&mut h, "dict-rename", 0);
        h.event(egui::Event::Text("x".into()));
        h.run();
        h.key_press(Key::Escape);
        h.run();
        assert!(h.state().state.renaming.is_none());
        assert!(h.state().out.is_empty() && !h.state().asked.close);
    }

    // T-DIC-033
    #[test]
    fn delete_asks_first() {
        let mut h = harness(page(&[("a", record("JM", true))]));
        h.run();
        let out = click(&mut h, "dict-delete", 0);
        assert!(out.is_empty());
        assert!(h.query_by_label("dict-delete-confirm").is_some());
        let out = click(&mut h, "dict-cancel", 0);
        assert!(out.is_empty() && h.state().state.deleting.is_none());
        click(&mut h, "dict-delete", 0);
        // the dialog's button is the one in the middle of the window
        let centre = pos2(640.0, 360.0);
        let (n, _) = h
            .query_all_by_label("dict-delete")
            .map(|node| node.rect().center().distance(centre))
            .enumerate()
            .fold(
                (0, f32::INFINITY),
                |best, (i, d)| if d < best.1 { (i, d) } else { best },
            );
        let out = click(&mut h, "dict-delete", n);
        assert!(
            matches!(&out[..], [Command::DictDelete(id)] if id == "a"),
            "{out:?}"
        );
    }

    // T-DIC-010, T-DIC-016
    #[test]
    fn a_running_import_can_be_cancelled_and_an_update_confirmed() {
        let mut running = page(&[]);
        running.import = Import::Running {
            file: "jmdict.zip".into(),
            progress: nikomiru_dict::import::Progress {
                files_done: 2,
                files_total: 4,
                entries: 5000,
            },
            cancel: nikomiru_core::Cancel::new(),
            generation: 1,
        };
        let mut h = harness(running);
        h.run();
        assert!(h.query_by_label("dict-importing: jmdict.zip").is_some());
        assert!(h.query_by_label("dict-empty").is_some());
        let out = click(&mut h, "dict-cancel", 0);
        assert!(matches!(&out[..], [Command::DictCancel]), "{out:?}");
        // no other import meanwhile
        click(&mut h, "dict-import", 0);
        assert!(!h.state().asked.dialog);

        let mut offer = page(&[("a", record("JM", true))]);
        offer.import = Import::Confirm {
            zip: "/x/jmdict-3.zip".into(),
            id: "a".into(),
            title: "JM".into(),
            installed: "2".into(),
        };
        let mut h = harness(offer);
        h.run();
        let out = click(&mut h, "dict-update", 0);
        assert!(matches!(&out[..], [Command::DictConfirm(true)]), "{out:?}");
        let out = click(&mut h, "dict-keep", 0);
        assert!(matches!(&out[..], [Command::DictConfirm(false)]), "{out:?}");
    }
}
