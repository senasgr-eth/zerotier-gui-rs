// ZeroTier-GUI — a cross-platform front-end for ZeroTier.
//
// Rust + GPUI rewrite Copyright (C) 2026 Senasgr.
// Based on the original ZeroTier-GUI by Tomás Ralph (GPL-3.0-or-later).
// See NOTICE and LICENSE for attribution and licensing details.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! ZeroTier-GUI: a Linux front-end for ZeroTier, written in Rust with GPUI.

mod app;
mod history;
mod theme;
mod zerotier;

use app::ZeroTierGui;
use gpui::prelude::*;
use gpui::{App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(960.), px(680.)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(800.), px(560.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("ZeroTier-GUI".into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                app_id: Some("zerotier-gui".to_string()),
                ..Default::default()
            },
            |_, cx| cx.new(ZeroTierGui::new),
        )
        .expect("could not open the ZeroTier-GUI window");

        cx.activate(true);
    });
}
