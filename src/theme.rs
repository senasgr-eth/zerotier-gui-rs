// ZeroTier-GUI — a cross-platform front-end for ZeroTier.
//
// Rust + GPUI rewrite Copyright (C) 2026 Senasgr.
// Based on the original ZeroTier-GUI by Tomás Ralph (GPL-3.0-or-later).
// See NOTICE and LICENSE for attribution and licensing details.

//! Colours, type and small building blocks shared by every screen.

use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, ElementId, FontWeight, IntoElement, ParentElement, SharedString, Styled,
    Window, div, px, rgb,
};

pub const BG: u32 = 0xd9d9d9;
pub const SURFACE: u32 = 0xf2f2f2;
pub const SURFACE_ALT: u32 = 0xe9e9e9;
pub const INK: u32 = 0x202020;
pub const MUTED: u32 = 0x6a6a6a;
pub const BORDER: u32 = 0xc4c4c4;
pub const ACCENT: u32 = 0xffb253;
pub const ACCENT_HOVER: u32 = 0xffbf71;
pub const ACCENT_PRESSED: u32 = 0xf2a33d;
pub const ROW_ODD: u32 = 0xfcfcfc;
pub const ROW_EVEN: u32 = 0xeeeeee;
pub const ROW_HOVER: u32 = 0xf6f6f6;
pub const SELECTED_ROW: u32 = 0xffe4c0;
pub const SELECTED_ROW_HOVER: u32 = 0xffdcae;
pub const DANGER: u32 = 0xc83939;
pub const DANGER_SOFT: u32 = 0xf6d9d9;
pub const DANGER_INK: u32 = 0x8e1f1f;
pub const SUCCESS: u32 = 0x2f7d32;
pub const WARNING: u32 = 0xa35f00;
pub const LINK: u32 = 0x2c62a0;
pub const TOAST_BG: u32 = 0x2b2b2b;

/// True when the Linux-only features (systemd, pkexec, sysfs) are available.
pub const LINUX: bool = cfg!(target_os = "linux");

#[cfg(target_os = "linux")]
pub const UI_FONT: &str = "DejaVu Sans";
#[cfg(target_os = "macos")]
pub const UI_FONT: &str = "Helvetica Neue";
#[cfg(target_os = "windows")]
pub const UI_FONT: &str = "Segoe UI";
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub const UI_FONT: &str = "sans-serif";

#[cfg(target_os = "linux")]
pub const MONO_FONT: &str = "DejaVu Sans Mono";
#[cfg(target_os = "macos")]
pub const MONO_FONT: &str = "Menlo";
#[cfg(target_os = "windows")]
pub const MONO_FONT: &str = "Consolas";
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub const MONO_FONT: &str = "monospace";

pub fn panel() -> Div {
    div()
        .flex()
        .flex_col()
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgb(BORDER))
        .rounded_lg()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Danger,
    Ghost,
}

/// A consistently styled, optionally disabled action button.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let (background, hover, pressed, foreground, border) = match kind {
        ButtonKind::Primary => (ACCENT, ACCENT_HOVER, ACCENT_PRESSED, INK, 0xd99a3f),
        ButtonKind::Secondary => (0xf7f7f7, 0xffffff, 0xe6e6e6, INK, BORDER),
        ButtonKind::Danger => (0xfdf1f1, 0xf9dede, 0xf6d0d0, DANGER_INK, 0xd99c9c),
        ButtonKind::Ghost => (SURFACE, 0xe4e4e4, 0xdedede, MUTED, SURFACE),
    };

    div()
        .id(id)
        .h(px(32.))
        .px_3()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .border_1()
        .bg(rgb(background))
        .border_color(rgb(border))
        .text_color(rgb(foreground))
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .when(enabled, move |element| {
            element
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(hover)))
                .active(move |style| style.bg(rgb(pressed)))
                .on_click(on_click)
        })
        .when(!enabled, |element| element.opacity(0.42))
        .child(label.into())
}

/// A label/value pair used by the About, Join and Network Info screens.
pub fn field(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    monospace: bool,
) -> impl IntoElement {
    div()
        .flex()
        .items_start()
        .gap_3()
        .text_sm()
        .child(
            div()
                .w(px(170.))
                .flex_none()
                .text_color(rgb(MUTED))
                .child(label.into()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(rgb(INK))
                .when(monospace, |element| element.font_family(MONO_FONT))
                .child(value.into()),
        )
}

pub fn pill(text: impl Into<SharedString>, foreground: u32, background: u32) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1p5()
        .px_2()
        .py_1()
        .rounded_full()
        .bg(rgb(background))
        .text_xs()
        .text_color(rgb(foreground))
        .child(dot(foreground))
        .child(text.into())
}

pub fn dot(color: u32) -> impl IntoElement {
    div().size(px(7.)).rounded_full().bg(rgb(color))
}

/// The square checkbox used in the Network Info screen.
pub fn checkbox(checked: bool) -> impl IntoElement {
    div()
        .size(px(18.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .border_1()
        .border_color(rgb(if checked { 0xd99a3f } else { BORDER }))
        .bg(rgb(if checked { ACCENT } else { 0xffffff }))
        .when(checked, |element| {
            element.child(div().size(px(8.)).rounded_sm().bg(rgb(0x4a2c00)))
        })
}

pub fn empty_state(
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .p_10()
        .text_center()
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(INK))
                .child(title.into()),
        )
        .child(div().text_sm().text_color(rgb(MUTED)).child(body.into()))
}
