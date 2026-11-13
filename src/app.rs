// ZeroTier-GUI — a cross-platform front-end for ZeroTier.
//
// Rust + GPUI rewrite Copyright (C) 2026 Senasgr.
// Based on the original ZeroTier-GUI by Tomás Ralph (GPL-3.0-or-later).
// See NOTICE and LICENSE for attribution and licensing details.

//! The application state and all of its screens.

use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, FocusHandle, Focusable, FontWeight, IntoElement,
    KeyDownEvent, ParentElement, Render, SharedString, Styled, Timer, Window, div, px, rgb, rgba,
};

use crate::history::{self, NetworkHistory};
use crate::theme::{
    self, ACCENT, BG, BORDER, DANGER, DANGER_INK, DANGER_SOFT, INK, LINK, MONO_FONT, MUTED,
    ROW_EVEN, ROW_HOVER, ROW_ODD, SELECTED_ROW, SELECTED_ROW_HOVER, SUCCESS, SURFACE_ALT, TOAST_BG,
    UI_FONT, WARNING, checkbox, empty_state, field, panel, pill,
};
use crate::zerotier::{self, CliError, CliErrorKind, ClientStatus, Network, Peer};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Screen {
    Networks,
    Join,
    Peers,
    PeerPaths { peer: String },
    NetworkInfo { network: String },
    About,
}

impl Screen {
    fn back_target(&self) -> Screen {
        match self {
            Screen::PeerPaths { .. } => Screen::Peers,
            _ => Screen::Networks,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StartupState {
    Checking,
    Ready,
    ServiceDown,
    NoAccess { has_token: bool },
    NotInstalled,
    Failed(String),
}

impl StartupState {
    fn from_error(error: &CliError) -> Self {
        match error.kind {
            CliErrorKind::NotInstalled => StartupState::NotInstalled,
            CliErrorKind::ServiceDown => StartupState::ServiceDown,
            CliErrorKind::AccessDenied => StartupState::NoAccess {
                has_token: zerotier::authtoken_exists(),
            },
            CliErrorKind::Other | CliErrorKind::Parse => StartupState::Failed(error.detail()),
        }
    }
}

#[derive(Debug, Clone)]
struct Dialog {
    title: String,
    message: String,
    error: bool,
    action: Option<ConfirmAction>,
}

#[derive(Debug, Clone)]
enum ConfirmAction {
    LeaveNetwork { id: String },
}

impl Dialog {
    fn info(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            error: false,
            action: None,
        }
    }

    fn error(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            error: true,
            action: None,
        }
    }

    fn confirm(
        title: impl Into<String>,
        message: impl Into<String>,
        action: ConfirmAction,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            error: false,
            action: Some(action),
        }
    }
}

#[derive(Debug, Clone)]
struct Toast {
    message: String,
    error: bool,
    sequence: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Busy {
    refreshing: bool,
    command: bool,
    startup: bool,
}

impl Busy {
    fn any(&self) -> bool {
        self.refreshing || self.command || self.startup
    }
}

pub struct ZeroTierGui {
    screen: Screen,
    focus_handle: FocusHandle,

    networks: Vec<Network>,
    peers: Vec<Peer>,
    interface_states: std::collections::BTreeMap<String, String>,
    status: ClientStatus,
    service_state: String,

    history: NetworkHistory,
    selection: Option<String>,
    peer_selection: Option<String>,
    join_input: String,
    join_selection: Option<String>,

    startup: StartupState,
    startup_error: Option<String>,

    dialog: Option<Dialog>,
    toast: Option<Toast>,
    toast_sequence: u64,
    busy: Busy,
}

impl ZeroTierGui {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            screen: Screen::Networks,
            focus_handle: cx.focus_handle(),
            networks: Vec::new(),
            peers: Vec::new(),
            interface_states: Default::default(),
            status: ClientStatus::default(),
            service_state: "unknown".to_string(),
            history: history::load(),
            selection: None,
            peer_selection: None,
            join_input: String::new(),
            join_selection: None,
            startup: StartupState::Checking,
            startup_error: None,
            dialog: None,
            toast: None,
            toast_sequence: 0,
            busy: Busy::default(),
        };

        this.begin_startup(cx);
        this
    }

    // ---------------------------------------------------------------- tasks

    fn spawn_blocking<T, F, D>(cx: &mut Context<Self>, work: F, done: D)
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
        D: FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
    {
        cx.spawn(async move |this, cx| {
            let value = cx.background_executor().spawn(async move { work() }).await;
            this.update(cx, |this, cx| {
                done(this, value, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn begin_startup(&mut self, cx: &mut Context<Self>) {
        if self.busy.startup && self.startup != StartupState::Checking {
            return;
        }

        self.startup = StartupState::Checking;
        self.startup_error = None;
        self.busy.startup = true;
        cx.notify();

        Self::spawn_blocking(cx, zerotier::probe, |this, result, cx| {
            this.busy.startup = false;
            match result {
                Ok(networks) => {
                    this.startup = StartupState::Ready;
                    this.networks = networks;
                    this.sync_history_names(cx);
                    this.refresh(cx);
                }
                Err(error) => {
                    this.startup = StartupState::from_error(&error);
                    this.startup_error = match this.startup {
                        StartupState::NoAccess { .. } | StartupState::Failed(_) => {
                            Some(error.detail())
                        }
                        _ => None,
                    };
                }
            }
        });
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.busy.refreshing || self.busy.command || self.startup != StartupState::Ready {
            return;
        }

        self.busy.refreshing = true;
        cx.notify();

        Self::spawn_blocking(cx, zerotier::snapshot, |this, result, cx| {
            this.busy.refreshing = false;
            match result {
                Ok(snapshot) => {
                    this.apply_snapshot(snapshot, cx);
                }
                Err(error) => match error.kind {
                    CliErrorKind::NotInstalled
                    | CliErrorKind::ServiceDown
                    | CliErrorKind::AccessDenied => {
                        this.startup = StartupState::from_error(&error);
                        this.startup_error = Some(error.detail());
                    }
                    CliErrorKind::Other | CliErrorKind::Parse => {
                        this.toast(error.detail(), true, cx);
                    }
                },
            }
        });
    }

    fn apply_snapshot(&mut self, snapshot: zerotier::Snapshot, cx: &mut Context<Self>) {
        self.networks = snapshot.networks;
        self.peers = snapshot.peers;
        self.interface_states = snapshot.interface_states;
        self.status = snapshot.status;
        self.service_state = snapshot.service_state;

        if let Some(selection) = &self.selection
            && !self
                .networks
                .iter()
                .any(|network| network.key() == selection)
        {
            self.selection = None;
        }

        if let Screen::NetworkInfo { network } = &self.screen
            && !self
                .networks
                .iter()
                .any(|candidate| candidate.key() == network)
        {
            self.screen = Screen::Networks;
        }

        if let Screen::PeerPaths { peer } = &self.screen
            && !self
                .peers
                .iter()
                .any(|candidate| candidate.address == *peer)
        {
            self.screen = Screen::Peers;
        }

        if let Some(peer) = &self.peer_selection
            && !self
                .peers
                .iter()
                .any(|candidate| candidate.address == *peer)
        {
            self.peer_selection = None;
        }

        if let Some(selected) = &self.join_selection
            && !self.history.contains_key(selected)
        {
            self.join_selection = None;
        }

        self.sync_history_names(cx);
    }

    fn sync_history_names(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        for network in &self.networks {
            if let Some(entry) = self.history.get_mut(network.key())
                && entry.name != network.name
            {
                entry.name = network.name.clone();
                changed = true;
            }
        }

        if changed && let Err(error) = history::save(&self.history) {
            self.toast(
                format!("Could not save the network history: {error}"),
                true,
                cx,
            );
        }
    }

    // ----------------------------------------------------------- navigation

    fn go_back(&mut self, cx: &mut Context<Self>) {
        self.screen = self.screen.back_target();
        self.dialog = None;
        cx.notify();
    }

    fn open_join(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = Screen::Join;
        self.join_input.clear();
        self.join_selection = None;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn open_peers(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Peers;
        cx.notify();
        self.refresh(cx);
    }

    fn open_network_info(&mut self, cx: &mut Context<Self>) {
        let Some(network) = self.selection.clone() else {
            self.show_info(
                "No network selected",
                "Select a network from the list first, or double-click it.",
            );
            return;
        };

        self.screen = Screen::NetworkInfo { network };
        cx.notify();
    }

    fn open_peer_paths(&mut self, peer: Option<String>, cx: &mut Context<Self>) {
        let Some(peer) = peer.or_else(|| self.peer_selection.clone()) else {
            self.show_info("No peer selected", "Select a peer from the list first.");
            return;
        };

        self.screen = Screen::PeerPaths { peer };
        cx.notify();
    }

    fn open_about(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::About;
        cx.notify();
    }

    fn select_network(&mut self, id: String, open_info: bool, cx: &mut Context<Self>) {
        self.selection = Some(id.clone());
        if open_info {
            self.screen = Screen::NetworkInfo { network: id };
        }
        cx.notify();
    }

    fn select_peer(&mut self, address: String, open_paths: bool, cx: &mut Context<Self>) {
        self.peer_selection = Some(address.clone());
        if open_paths {
            self.screen = Screen::PeerPaths { peer: address };
        }
        cx.notify();
    }

    // -------------------------------------------------------------- dialogs

    fn show_info(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.dialog = Some(Dialog::info(title, message));
    }

    fn show_error(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.dialog = Some(Dialog::error(title, message));
    }

    fn dismiss_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialog = None;
        cx.notify();
    }

    fn confirm_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };

        if let Some(action) = dialog.action {
            self.run_confirmed_action(action, cx);
        } else {
            cx.notify();
        }
    }

    fn run_confirmed_action(&mut self, action: ConfirmAction, cx: &mut Context<Self>) {
        match action {
            ConfirmAction::LeaveNetwork { id } => {
                if self.busy.command {
                    return;
                }

                self.busy.command = true;
                cx.notify();
                Self::spawn_blocking(
                    cx,
                    move || zerotier::leave(&id),
                    |this, result, cx| {
                        this.busy.command = false;
                        match result {
                            Ok(()) => {
                                this.toast("Successfully left network", false, cx);
                                this.refresh(cx);
                            }
                            Err(error) => {
                                this.show_error("Could not leave network", error.detail())
                            }
                        }
                    },
                );
            }
        }
    }

    fn toast(&mut self, message: impl Into<String>, error: bool, cx: &mut Context<Self>) {
        self.toast_sequence += 1;
        let sequence = self.toast_sequence;
        self.toast = Some(Toast {
            message: message.into(),
            error,
            sequence,
        });

        cx.spawn(async move |this, cx| {
            Timer::after(Duration::from_secs(4)).await;
            this.update(cx, |this, cx| {
                if this.toast.as_ref().map(|toast| toast.sequence) == Some(sequence) {
                    this.toast = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();

        cx.notify();
    }

    // -------------------------------------------------------------- actions

    fn toggle_service(&mut self, cx: &mut Context<Self>) {
        if self.busy.command {
            return;
        }

        let running = self.service_is_active();
        self.busy.command = true;
        cx.notify();

        Self::spawn_blocking(
            cx,
            move || {
                zerotier::set_service_running(!running)?;
                if !running {
                    std::thread::sleep(Duration::from_millis(600));
                }
                Ok::<(), CliError>(())
            },
            move |this, result, cx| {
                this.busy.command = false;
                match result {
                    Ok(()) => {
                        let message = if running {
                            "ZeroTier service stopped"
                        } else {
                            "ZeroTier service started"
                        };
                        this.toast(message, false, cx);
                        this.refresh(cx);
                    }
                    Err(error) => {
                        this.show_error("Could not change the service state", error.detail())
                    }
                }
            },
        );
    }

    fn toggle_interface(&mut self, cx: &mut Context<Self>) {
        if self.busy.command {
            return;
        }

        let Some(network) = self.selected_network().cloned() else {
            self.show_info(
                "No network selected",
                "Select a network from the list first.",
            );
            return;
        };

        let interface = network.port_device_name.clone();
        if interface.is_empty() {
            self.show_error(
                "No interface",
                "This network does not have an interface yet. Wait for ZeroTier to configure it and refresh.",
            );
            return;
        }

        let down = self.interface_is_down(network.key());
        self.busy.command = true;
        cx.notify();

        Self::spawn_blocking(
            cx,
            move || zerotier::set_interface_running(&interface, down),
            move |this, result, cx| {
                this.busy.command = false;
                match result {
                    Ok(()) => {
                        this.toast(
                            if down {
                                "Interface connected"
                            } else {
                                "Interface disconnected"
                            },
                            false,
                            cx,
                        );
                        this.refresh(cx);
                    }
                    Err(error) => this.show_error("Could not change the interface", error.detail()),
                }
            },
        );
    }

    fn request_leave(&mut self, cx: &mut Context<Self>) {
        let Some(network) = self.selected_network().cloned() else {
            self.show_info(
                "No network selected",
                "Select a network from the list first.",
            );
            return;
        };

        self.dialog = Some(Dialog::confirm(
            "Leave Network",
            format!(
                "Are you sure you want to leave \"{}\" (ID: {})?",
                network.display_name(),
                network.key()
            ),
            ConfirmAction::LeaveNetwork {
                id: network.key().to_string(),
            },
        ));
        cx.notify();
    }

    fn join_network(&mut self, cx: &mut Context<Self>) {
        if self.busy.command {
            return;
        }

        let network_id = self.join_input.trim().to_ascii_lowercase();
        if network_id.len() != 16
            || !network_id
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            self.show_error(
                "Invalid network ID",
                "A ZeroTier network ID is 16 hexadecimal characters, for example 8056c2e21c000001.",
            );
            return;
        }

        if self
            .networks
            .iter()
            .any(|network| network.key() == network_id)
        {
            self.show_info("Already joined", "You're already a member of this network.");
            return;
        }

        self.busy.command = true;
        cx.notify();

        let id = network_id.clone();
        Self::spawn_blocking(
            cx,
            move || zerotier::join(&id),
            move |this, result, cx| {
                this.busy.command = false;
                match result {
                    Ok(()) => {
                        this.add_history_entry(&network_id, cx);
                        this.screen = Screen::Networks;
                        this.join_input.clear();
                        this.join_selection = None;
                        this.toast("Successfully joined network", false, cx);
                        this.refresh(cx);
                    }
                    Err(error) => {
                        let message = if error.kind == CliErrorKind::ServiceDown {
                            "The 'zerotier-one' service isn't running.".to_string()
                        } else {
                            format!("Invalid network ID.\n\n{}", error.message)
                        };
                        this.show_error("Could not join network", message);
                    }
                }
            },
        );
    }

    fn add_history_entry(&mut self, network_id: &str, cx: &mut Context<Self>) {
        let name = self
            .networks
            .iter()
            .find(|network| network.key() == network_id)
            .map(Network::display_name)
            .unwrap_or_default();

        self.history.insert(
            network_id.to_string(),
            history::HistoryEntry {
                name,
                join_date: history::timestamp(),
            },
        );

        if let Err(error) = history::save(&self.history) {
            self.toast(
                format!("Could not save the network history: {error}"),
                true,
                cx,
            );
        }
    }

    fn delete_history_entry(&mut self, cx: &mut Context<Self>) {
        let Some(network_id) = self.join_selection.clone() else {
            self.show_info(
                "No history entry selected",
                "Select an entry in the network history first.",
            );
            return;
        };

        self.history.remove(&network_id);
        self.join_selection = None;
        if let Err(error) = history::save(&self.history) {
            self.toast(
                format!("Could not save the network history: {error}"),
                true,
                cx,
            );
        } else {
            self.toast("History entry deleted", false, cx);
        }
        cx.notify();
    }

    fn set_network_option(
        &mut self,
        network_id: String,
        option: &'static str,
        value: bool,
        cx: &mut Context<Self>,
    ) {
        if self.busy.command {
            return;
        }

        self.busy.command = true;
        cx.notify();

        Self::spawn_blocking(
            cx,
            move || zerotier::set_network_option(&network_id, option, value),
            move |this, result, cx| {
                this.busy.command = false;
                match result {
                    Ok(()) => this.refresh(cx),
                    Err(error) => this.show_error("Could not update the network", error.detail()),
                }
            },
        );
    }

    fn start_service_from_startup(&mut self, cx: &mut Context<Self>) {
        if self.busy.startup {
            return;
        }

        self.busy.startup = true;
        self.startup_error = None;
        cx.notify();

        Self::spawn_blocking(
            cx,
            || {
                zerotier::set_service_running(true)?;
                std::thread::sleep(Duration::from_millis(600));
                Ok::<(), CliError>(())
            },
            |this, result, cx| {
                this.busy.startup = false;
                match result {
                    Ok(()) => this.begin_startup(cx),
                    Err(error) => {
                        this.startup = StartupState::ServiceDown;
                        this.startup_error = Some(error.detail());
                    }
                }
            },
        );
    }

    fn grant_access(&mut self, cx: &mut Context<Self>) {
        if self.busy.startup {
            return;
        }

        let user = zerotier::username();
        self.busy.startup = true;
        self.startup_error = None;
        cx.notify();

        Self::spawn_blocking(
            cx,
            move || zerotier::grant_access(&user),
            |this, result, cx| {
                this.busy.startup = false;
                match result {
                    Ok(()) => {
                        this.show_info(
                            "Access granted",
                            "You were successfully added to the zerotier-one group. \
                             Log out and back in (then reopen this application) for the \
                             change to take effect.",
                        );
                        cx.notify();
                    }
                    Err(error) => {
                        this.startup_error = Some(error.detail());
                        this.show_error("Could not grant access", error.detail());
                    }
                }
            },
        );
    }

    // ---------------------------------------------------------------- input

    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let control = event.keystroke.modifiers.control;

        if self.dialog.is_some() {
            match key {
                "escape" => self.dismiss_dialog(cx),
                "enter" => self.confirm_dialog(cx),
                _ => {}
            }
            return;
        }

        if control && key == "r" {
            self.refresh(cx);
            return;
        }

        match &self.screen {
            Screen::Join => match key {
                "escape" => self.go_back(cx),
                "enter" => self.join_network(cx),
                "backspace" => {
                    self.join_input.pop();
                    cx.notify();
                }
                "v" if control => {
                    if let Some(clipboard) = cx.read_from_clipboard()
                        && let Some(text) = clipboard.text()
                    {
                        for character in text.chars() {
                            self.push_join_character(character);
                        }
                        cx.notify();
                    }
                }
                "u" if control => {
                    self.join_input.clear();
                    cx.notify();
                }
                _ => {
                    if let Some(text) = &event.keystroke.key_char {
                        let mut changed = false;
                        for character in text.chars() {
                            changed |= self.push_join_character(character);
                        }
                        if changed {
                            cx.notify();
                        }
                    }
                }
            },
            Screen::PeerPaths { .. } => {
                if key == "escape" {
                    self.go_back(cx);
                } else if control && key == "r" {
                    self.refresh(cx);
                }
            }
            Screen::Peers => {
                if key == "escape" {
                    self.go_back(cx);
                }
            }
            Screen::NetworkInfo { .. } | Screen::About => {
                if key == "escape" {
                    self.go_back(cx);
                }
            }
            Screen::Networks => {}
        }

        let _ = window;
    }

    fn push_join_character(&mut self, character: char) -> bool {
        if character.is_ascii_hexdigit() && self.join_input.len() < 16 {
            self.join_input.push(character.to_ascii_lowercase());
            true
        } else {
            false
        }
    }

    // --------------------------------------------------------------- helpers

    fn selected_network(&self) -> Option<&Network> {
        let selection = self.selection.as_deref()?;
        self.networks
            .iter()
            .find(|network| network.key() == selection)
    }

    fn interface_state(&self, network: &Network) -> String {
        self.interface_states
            .get(network.key())
            .cloned()
            .unwrap_or_else(|| zerotier::interface_state(&network.port_device_name))
    }

    fn interface_is_down(&self, network_id: &str) -> bool {
        self.interface_states
            .get(network_id)
            .map(|state| state.eq_ignore_ascii_case("down"))
            .unwrap_or(false)
    }

    fn service_is_active(&self) -> bool {
        self.service_state == "active"
    }

    fn service_pill(&self) -> AnyElement {
        let (foreground, background) = match self.service_state.as_str() {
            "active" => (SUCCESS, 0xe2f0e3),
            "inactive" | "failed" => (DANGER, 0xf7e2e2),
            _ => (MUTED, 0xebebeb),
        };

        let text = if self.startup == StartupState::Ready {
            format!("Service: {}", self.service_state)
        } else {
            format!("Service: {}", self.service_label())
        };

        pill(text, foreground, background).into_any_element()
    }

    fn service_label(&self) -> &'static str {
        match &self.startup {
            StartupState::Checking => "checking",
            StartupState::NoAccess { .. } => "access denied",
            StartupState::NotInstalled => "not installed",
            StartupState::Failed(_) => "error",
            _ => "stopped",
        }
    }

    // ---------------------------------------------------------------- render

    fn render_topbar(&self, _cx: &mut Context<Self>) -> AnyElement {
        let mut badges = div()
            .flex()
            .items_center()
            .gap_2()
            .child(self.service_pill());

        if self.busy.any() {
            badges = badges.child(div().text_xs().text_color(rgb(MUTED)).child("Working…"));
        }

        div()
            .flex()
            .items_center()
            .justify_between()
            .px_5()
            .pt_4()
            .pb_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(INK))
                            .child("ZeroTier-GUI"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child("A cross-platform front-end for ZeroTier"),
                    ),
            )
            .child(badges)
            .into_any_element()
    }

    fn render_statusbar(&self, _cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(30.))
            .px_5()
            .border_t_1()
            .border_color(rgb(BORDER))
            .text_xs()
            .text_color(rgb(MUTED))
            .child(format!(
                "ZeroTier-GUI {}  ·  Rust + GPUI",
                env!("CARGO_PKG_VERSION")
            ))
            .child(if self.status.address.is_empty() {
                "ZeroTier: -".to_string()
            } else {
                format!(
                    "ZeroTier {}  ·  {}",
                    self.status.display_version(),
                    self.status.display_address()
                )
            })
            .into_any_element()
    }

    fn render_screen(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.startup != StartupState::Ready {
            return self.render_startup(cx);
        }

        match &self.screen {
            Screen::Networks => self.render_networks(cx),
            Screen::Join => self.render_join(cx),
            Screen::Peers => self.render_peers(cx),
            Screen::PeerPaths { peer } => self.render_peer_paths(peer.clone(), cx),
            Screen::NetworkInfo { network } => self.render_network_info(network.clone(), cx),
            Screen::About => self.render_about(cx),
        }
    }

    fn screen_header(
        &self,
        title: impl Into<SharedString>,
        subtitle: impl Into<SharedString>,
        actions: Vec<AnyElement>,
    ) -> AnyElement {
        div()
            .flex()
            .items_end()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(INK))
                            .child(title.into()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(subtitle.into()),
                    ),
            )
            .child(div().flex().items_center().gap_2().children(actions))
            .into_any_element()
    }

    fn back_button(&self, cx: &mut Context<Self>) -> AnyElement {
        theme::button("back", "← Back", theme::ButtonKind::Secondary, true, {
            cx.listener(|this, _: &ClickEvent, _window, cx| this.go_back(cx))
        })
        .into_any_element()
    }

    fn refresh_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let enabled = !self.busy.refreshing && !self.busy.command;
        theme::button(
            "refresh",
            if self.busy.refreshing {
                "Refreshing…"
            } else {
                "Refresh"
            },
            theme::ButtonKind::Secondary,
            enabled,
            cx.listener(|this, _: &ClickEvent, _window, cx| this.refresh(cx)),
        )
        .into_any_element()
    }

    fn render_networks(&self, cx: &mut Context<Self>) -> AnyElement {
        let has_selection = self.selection.is_some();
        let interface_down = self
            .selected_network()
            .map(|network| self.interface_is_down(network.key()))
            .unwrap_or(false);

        let connect_label = if interface_down {
            "Connect Interface"
        } else {
            "Disconnect Interface"
        };

        let header = self.screen_header(
            "Joined Networks",
            match self.networks.len() {
                0 => "No networks joined yet".to_string(),
                1 => "1 network".to_string(),
                count => format!("{count} networks"),
            },
            vec![
                self.refresh_button(cx),
                theme::button(
                    "join-network",
                    "Join Network",
                    theme::ButtonKind::Primary,
                    !self.busy.command,
                    cx.listener(|this, _: &ClickEvent, window, cx| this.open_join(window, cx)),
                )
                .into_any_element(),
                theme::button(
                    "show-peers",
                    "Show Peers",
                    theme::ButtonKind::Secondary,
                    true,
                    cx.listener(|this, _: &ClickEvent, _window, cx| this.open_peers(cx)),
                )
                .into_any_element(),
                theme::button(
                    "about",
                    "About",
                    theme::ButtonKind::Ghost,
                    true,
                    cx.listener(|this, _: &ClickEvent, _window, cx| this.open_about(cx)),
                )
                .into_any_element(),
            ],
        );

        let mut rows: Vec<AnyElement> = Vec::with_capacity(self.networks.len());
        for (index, network) in self.networks.iter().enumerate() {
            rows.push(self.render_network_row(network, index, cx));
        }

        let body = if self.networks.is_empty() {
            div()
                .id("network-empty")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(empty_state(
                    "No networks joined",
                    "Join a network with its 16-character network ID to get started.",
                ))
        } else {
            div()
                .id("network-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(rows)
        };

        let table = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(ROW_ODD))
            .overflow_hidden()
            .child(table_header(&[
                ("Network ID", Some(320.)),
                ("Name", None),
                ("Status", Some(110.)),
            ]))
            .child(body);

        let mut footer = div().flex().items_center().gap_2().child(theme::button(
            "leave-network",
            "Leave Network",
            theme::ButtonKind::Danger,
            has_selection && !self.busy.command,
            cx.listener(|this, _: &ClickEvent, _window, cx| this.request_leave(cx)),
        ));

        if theme::LINUX {
            footer = footer.child(theme::button(
                "toggle-interface",
                connect_label,
                theme::ButtonKind::Secondary,
                has_selection && !self.busy.command,
                cx.listener(|this, _: &ClickEvent, _window, cx| this.toggle_interface(cx)),
            ));
        }

        footer = footer
            .child(theme::button(
                "network-info",
                "Network Info",
                theme::ButtonKind::Secondary,
                has_selection,
                cx.listener(|this, _: &ClickEvent, _window, cx| this.open_network_info(cx)),
            ))
            .child(div().flex_1())
            .child(theme::button(
                "open-central",
                "ZeroTier Central",
                theme::ButtonKind::Secondary,
                true,
                |_: &ClickEvent, _: &mut Window, _: &mut App| zerotier::open_central(),
            ));

        if theme::LINUX {
            footer = footer.child(theme::button(
                "toggle-service",
                "Toggle ZT Service",
                theme::ButtonKind::Secondary,
                !self.busy.command,
                cx.listener(|this, _: &ClickEvent, _window, cx| this.toggle_service(cx)),
            ));
        }

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(header)
            .child(table)
            .child(footer)
            .into_any_element()
    }

    fn render_network_row(
        &self,
        network: &Network,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = network.key().to_string();
        let selected = self.selection.as_deref() == Some(id.as_str());
        let down = self.interface_is_down(&id);
        let status = if network.status.is_empty() {
            "-".to_string()
        } else {
            network.status.clone()
        };

        let background = if selected {
            SELECTED_ROW
        } else if down {
            0xf4d7d7
        } else if index.is_multiple_of(2) {
            ROW_EVEN
        } else {
            ROW_ODD
        };

        let hover = if selected {
            SELECTED_ROW_HOVER
        } else if down {
            0xefcaca
        } else {
            ROW_HOVER
        };

        let status_color = if down {
            DANGER_INK
        } else if status == "OK" {
            SUCCESS
        } else {
            MUTED
        };

        let click_id = id.clone();
        div()
            .id(SharedString::from(format!("network-row-{id}")))
            .flex()
            .items_center()
            .h(px(34.))
            .px_3()
            .gap_2()
            .bg(rgb(background))
            .border_b_1()
            .border_color(rgb(0xe3e3e3))
            .when(selected, |element| {
                element
                    .border_l_3()
                    .border_color(rgb(ACCENT))
                    .bg(rgb(SELECTED_ROW))
            })
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(hover)))
            .on_click(cx.listener(move |this, event: &ClickEvent, _window, cx| {
                let double_click = matches!(
                    event,
                    ClickEvent::Mouse(mouse) if mouse.down.click_count >= 2
                );
                this.select_network(click_id.clone(), double_click, cx);
            }))
            .child(table_cell(id, Some(320.), true, INK))
            .child(table_cell(network.display_name(), None, false, INK))
            .child(
                div()
                    .w(px(110.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .text_xs()
                    .text_color(rgb(status_color))
                    .child(theme::dot(status_color))
                    .child(status)
                    .when(down, |element| {
                        element.child(
                            div()
                                .ml_1()
                                .px_1()
                                .rounded_sm()
                                .bg(rgb(DANGER_SOFT))
                                .text_color(rgb(DANGER_INK))
                                .child("iface down"),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_join(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut entries: Vec<(&String, &history::HistoryEntry)> = self.history.iter().collect();
        entries.sort_by(|left, right| right.1.join_date.cmp(&left.1.join_date));

        let mut history_rows: Vec<AnyElement> = Vec::with_capacity(entries.len());
        for (network_id, entry) in entries {
            let id = network_id.clone();
            let selected = self.join_selection.as_deref() == Some(network_id.as_str());
            let click_id = id.clone();

            history_rows.push(
                div()
                    .id(SharedString::from(format!("history-row-{network_id}")))
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0xe5e5e5))
                    .bg(rgb(if selected { SELECTED_ROW } else { 0xffffff }))
                    .when(selected, |element| {
                        element.border_l_3().border_color(rgb(ACCENT))
                    })
                    .cursor_pointer()
                    .hover(move |style| {
                        style.bg(rgb(if selected {
                            SELECTED_ROW_HOVER
                        } else {
                            ROW_HOVER
                        }))
                    })
                    .on_click(cx.listener(move |this, event: &ClickEvent, _window, cx| {
                        this.join_selection = Some(click_id.clone());
                        this.join_input = click_id.clone();
                        let double_click = matches!(
                            event,
                            ClickEvent::Mouse(mouse) if mouse.down.click_count >= 2
                        );
                        if double_click {
                            this.join_network(cx);
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(INK))
                            .child(entry.display_name()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_family(MONO_FONT)
                            .text_color(rgb(MUTED))
                            .child(network_id.clone()),
                    )
                    .into_any_element(),
            );
        }

        let history_panel = panel()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(32.))
                    .px_3()
                    .bg(rgb(SURFACE_ALT))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child("NETWORK HISTORY")
                    .child(format!("{}", self.history.len())),
            )
            .child(if history_rows.is_empty() {
                div()
                    .id("history-empty")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(empty_state(
                        "Nothing here yet",
                        "Networks you join are remembered here for quick access.",
                    ))
            } else {
                div()
                    .id("history-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(history_rows)
            });

        let (info_id, info_name, info_date, info_joined) = match &self.join_selection {
            Some(network_id) => {
                let entry = self.history.get(network_id);
                (
                    network_id.clone(),
                    entry
                        .map(history::HistoryEntry::display_name)
                        .unwrap_or_else(|| "-".to_string()),
                    entry
                        .map(|entry| entry.join_date.clone())
                        .unwrap_or_else(|| "-".to_string()),
                    if self
                        .networks
                        .iter()
                        .any(|network| network.key() == network_id)
                    {
                        "Yes".to_string()
                    } else {
                        "No".to_string()
                    },
                )
            }
            None => (
                "-".to_string(),
                "-".to_string(),
                "-".to_string(),
                "-".to_string(),
            ),
        };

        let info_panel = panel()
            .w(px(360.))
            .flex_none()
            .p_3()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child("INFO"),
            )
            .child(field("Network ID:", info_id, true))
            .child(field("Name:", info_name, false))
            .child(field("Last joined date:", info_date, false))
            .child(field("Currently joined:", info_joined, false))
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Select an entry to reuse its network ID."),
            );

        let input_display: SharedString = if self.join_input.is_empty() {
            "8056c2e21c000001".into()
        } else {
            self.join_input.clone().into()
        };
        let input_is_empty = self.join_input.is_empty();

        let input = div()
            .id("join-input")
            .flex()
            .items_center()
            .gap_0p5()
            .w(px(230.))
            .h(px(32.))
            .px_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0xd99a3f))
            .bg(rgb(0xffffff))
            .font_family(MONO_FONT)
            .text_sm()
            .cursor_text()
            .on_click(cx.listener(|this, _: &ClickEvent, window, _cx| {
                window.focus(&this.focus_handle);
            }))
            .child(
                div()
                    .text_color(rgb(if input_is_empty { 0xa0a0a0 } else { INK }))
                    .child(input_display),
            )
            .child(div().w(px(1.)).h(px(17.)).bg(rgb(INK)));

        let footer = div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().text_sm().text_color(rgb(INK)).child("Network ID:"))
            .child(input)
            .child(theme::button(
                "join-submit",
                "Join",
                theme::ButtonKind::Primary,
                !self.busy.command,
                cx.listener(|this, _: &ClickEvent, _window, cx| this.join_network(cx)),
            ))
            .child(theme::button(
                "delete-history",
                "Delete history entry",
                theme::ButtonKind::Danger,
                self.join_selection.is_some(),
                cx.listener(|this, _: &ClickEvent, _window, cx| this.delete_history_entry(cx)),
            ))
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Enter joins · Esc goes back · Ctrl+V pastes"),
            );

        let header = self.screen_header(
            "Join Network",
            "Enter a network ID, or pick one from your history.",
            vec![self.back_button(cx)],
        );

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(header)
            .child(
                div()
                    .flex()
                    .gap_3()
                    .flex_1()
                    .min_h_0()
                    .child(history_panel)
                    .child(info_panel),
            )
            .child(footer)
            .into_any_element()
    }

    fn render_peers(&self, cx: &mut Context<Self>) -> AnyElement {
        let has_selection = self.peer_selection.is_some();
        let mut rows: Vec<AnyElement> = Vec::with_capacity(self.peers.len());

        for (index, peer) in self.peers.iter().enumerate() {
            let address = peer.address.clone();
            let selected = self.peer_selection.as_deref() == Some(address.as_str());
            let background = if selected {
                SELECTED_ROW
            } else if index % 2 == 0 {
                ROW_EVEN
            } else {
                ROW_ODD
            };
            let click_address = address.clone();
            let online_color = if peer.latency > 0 { SUCCESS } else { MUTED };

            rows.push(
                div()
                    .id(SharedString::from(format!("peer-row-{address}")))
                    .flex()
                    .items_center()
                    .h(px(34.))
                    .px_3()
                    .gap_2()
                    .bg(rgb(background))
                    .border_b_1()
                    .border_color(rgb(0xe3e3e3))
                    .when(selected, |element| {
                        element
                            .border_l_3()
                            .border_color(rgb(ACCENT))
                            .bg(rgb(SELECTED_ROW))
                    })
                    .cursor_pointer()
                    .hover(move |style| {
                        style.bg(rgb(if selected {
                            SELECTED_ROW_HOVER
                        } else {
                            ROW_HOVER
                        }))
                    })
                    .on_click(cx.listener(move |this, event: &ClickEvent, _window, cx| {
                        let double_click = matches!(
                            event,
                            ClickEvent::Mouse(mouse) if mouse.down.click_count >= 2
                        );
                        this.select_peer(click_address.clone(), double_click, cx);
                    }))
                    .child(table_cell(address, Some(160.), true, INK))
                    .child(table_cell(peer.display_version(), Some(120.), true, INK))
                    .child(table_cell(peer.role.clone(), None, false, INK))
                    .child(table_cell(
                        peer.display_latency(),
                        Some(110.),
                        true,
                        online_color,
                    ))
                    .child(table_cell(
                        format!("{}", peer.paths.len()),
                        Some(80.),
                        true,
                        MUTED,
                    ))
                    .into_any_element(),
            );
        }

        let body = if self.peers.is_empty() {
            div()
                .id("peer-empty")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(empty_state(
                    "No peers",
                    "ZeroTier has not discovered any peers yet.",
                ))
        } else {
            div()
                .id("peer-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(rows)
        };

        let table = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(ROW_ODD))
            .overflow_hidden()
            .child(table_header(&[
                ("ZT Address", Some(160.)),
                ("Version", Some(120.)),
                ("Role", None),
                ("Latency", Some(110.)),
                ("Paths", Some(80.)),
            ]))
            .child(body);

        let header = self.screen_header(
            "Peers",
            match self.peers.len() {
                0 => "No peers discovered".to_string(),
                1 => "1 peer".to_string(),
                count => format!("{count} peers"),
            },
            vec![
                self.refresh_button(cx),
                theme::button(
                    "see-paths",
                    "See Paths",
                    theme::ButtonKind::Primary,
                    has_selection,
                    cx.listener(|this, _: &ClickEvent, _window, cx| this.open_peer_paths(None, cx)),
                )
                .into_any_element(),
                self.back_button(cx),
            ],
        );

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(header)
            .child(table)
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Double-click a peer to inspect its connection paths."),
            )
            .into_any_element()
    }

    fn render_peer_paths(&self, peer_address: String, cx: &mut Context<Self>) -> AnyElement {
        let peer = self
            .peers
            .iter()
            .find(|peer| peer.address == peer_address)
            .cloned();

        let paths = peer
            .as_ref()
            .map(|peer| peer.paths.clone())
            .unwrap_or_default();

        let mut rows: Vec<AnyElement> = Vec::with_capacity(paths.len());
        for (index, path) in paths.iter().enumerate() {
            let background = if index % 2 == 0 { ROW_EVEN } else { ROW_ODD };
            let active_color = if path.active { SUCCESS } else { MUTED };
            let preferred_color = if path.preferred { SUCCESS } else { MUTED };

            rows.push(
                div()
                    .flex()
                    .items_center()
                    .h(px(34.))
                    .px_3()
                    .gap_2()
                    .bg(rgb(background))
                    .border_b_1()
                    .border_color(rgb(0xe3e3e3))
                    .child(table_cell(
                        bool_label(path.active),
                        Some(60.),
                        true,
                        active_color,
                    ))
                    .child(table_cell(path.address.clone(), None, true, INK))
                    .child(table_cell(
                        bool_label(path.expired),
                        Some(70.),
                        true,
                        if path.expired { DANGER } else { MUTED },
                    ))
                    .child(table_cell(
                        zerotier::format_age(path.last_receive),
                        Some(110.),
                        true,
                        MUTED,
                    ))
                    .child(table_cell(
                        zerotier::format_age(path.last_send),
                        Some(110.),
                        true,
                        MUTED,
                    ))
                    .child(table_cell(
                        bool_label(path.preferred),
                        Some(80.),
                        true,
                        preferred_color,
                    ))
                    .child(table_cell(
                        format!("{}", path.trusted_path_id),
                        Some(110.),
                        true,
                        MUTED,
                    ))
                    .into_any_element(),
            );
        }

        let table = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(ROW_ODD))
            .overflow_hidden()
            .child(table_header(&[
                ("Active", Some(60.)),
                ("Address", None),
                ("Expired", Some(70.)),
                ("Last Receive", Some(110.)),
                ("Last Send", Some(110.)),
                ("Preferred", Some(80.)),
                ("Trusted Path ID", Some(110.)),
            ]))
            .child(if rows.is_empty() {
                div()
                    .id("path-empty")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(empty_state(
                        "No paths",
                        "This peer has not reported any connection paths.",
                    ))
            } else {
                div()
                    .id("path-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows)
            });

        let header = self.screen_header(
            format!("Paths for {peer_address}"),
            match peer {
                Some(peer) => {
                    let mut parts = Vec::new();
                    let version = peer.display_version();
                    if version != "-" {
                        parts.push(version);
                    }
                    if !peer.role.is_empty() {
                        parts.push(peer.role.clone());
                    }
                    parts.push(format!("{} path(s)", peer.paths.len()));
                    parts.join("  ·  ")
                }
                None => "Peer is no longer available".to_string(),
            },
            vec![self.refresh_button(cx), self.back_button(cx)],
        );

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(header)
            .child(table)
            .into_any_element()
    }

    fn render_network_info(&self, network_id: String, cx: &mut Context<Self>) -> AnyElement {
        let Some(network) = self
            .networks
            .iter()
            .find(|network| network.key() == network_id)
            .cloned()
        else {
            return div()
                .flex()
                .flex_col()
                .gap_3()
                .child(self.screen_header(
                    "Network Info",
                    "This network is no longer available.",
                    vec![self.back_button(cx)],
                ))
                .child(empty_state(
                    "Network not found",
                    "It may have been removed. Go back and refresh the list.",
                ))
                .into_any_element();
        };

        let interface_state = self.interface_state(&network);
        let assigned = if network.assigned_addresses.is_empty() {
            "-".to_string()
        } else {
            network.assigned_addresses.join("\n")
        };

        let details = panel()
            .p_4()
            .gap_2()
            .child(
                div()
                    .flex()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .flex_1()
                            .child(field("Name:", network.display_name(), false))
                            .child(field("Network ID:", network.key().to_string(), true))
                            .child(field("Status:", network.status.clone(), false))
                            .child(field("State:", interface_state.clone(), false))
                            .child(field("Type:", network.network_type.clone(), false)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .flex_1()
                            .child(field("Device:", network.port_device_name.clone(), true))
                            .child(field("Bridge:", bool_yes_no(network.bridge), false))
                            .child(field("MAC Address:", network.mac.clone(), true))
                            .child(field("MTU:", network.mtu.to_string(), false))
                            .child(field("DHCP:", bool_yes_no(network.dhcp), false)),
                    ),
            )
            .child(div().h(px(1.)).bg(rgb(BORDER)).my_2())
            .child(field(
                "Assigned Addresses:",
                if network.assigned_addresses.is_empty() {
                    "-".to_string()
                } else {
                    network.assigned_addresses[0].clone()
                },
                true,
            ))
            .children(
                network
                    .assigned_addresses
                    .iter()
                    .skip(1)
                    .map(|address| field("", address.clone(), true)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(if assigned == "-" {
                        "No addresses assigned".to_string()
                    } else {
                        String::new()
                    }),
            );

        let flags = panel()
            .p_3()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child("NETWORK OPTIONS"),
            )
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .flex_1()
                            .child(self.render_flag(
                                &network,
                                "Allow Default Route",
                                "allowDefault",
                                network.allow_default,
                                cx,
                            ))
                            .child(self.render_flag(
                                &network,
                                "Allow Global IP",
                                "allowGlobal",
                                network.allow_global,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .flex_1()
                            .child(self.render_flag(
                                &network,
                                "Allow Managed IP",
                                "allowManaged",
                                network.allow_managed,
                                cx,
                            ))
                            .child(self.render_flag(
                                &network,
                                "Allow DNS Configuration",
                                "allowDNS",
                                network.allow_dns,
                                cx,
                            )),
                    ),
            );

        let mut footer = div().flex().items_center().gap_2();

        if theme::LINUX {
            footer = footer.child(theme::button(
                "info-toggle-interface",
                if interface_state.eq_ignore_ascii_case("down") {
                    "Connect Interface"
                } else {
                    "Disconnect Interface"
                },
                theme::ButtonKind::Secondary,
                !self.busy.command,
                cx.listener(|this, _: &ClickEvent, _window, cx| this.toggle_interface(cx)),
            ));
        }

        footer = footer.child(div().flex_1()).child(self.back_button(cx));

        let header = self.screen_header(
            "Network Info",
            format!("{}  ·  {}", network.display_name(), network.key()),
            vec![self.refresh_button(cx)],
        );

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(header)
            .child(
                div()
                    .id("network-info-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .gap_3()
                    .overflow_y_scroll()
                    .child(details)
                    .child(flags),
            )
            .child(footer)
            .into_any_element()
    }

    fn render_flag(
        &self,
        network: &Network,
        label: &'static str,
        option: &'static str,
        checked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = !self.busy.command;
        let network_id = network.key().to_string();
        let element_id = format!("flag-{option}-{network_id}");

        div()
            .id(SharedString::from(element_id))
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_2()
            .rounded_md()
            .when(enabled, |element| {
                element
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0xf7f7f7)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.set_network_option(network_id.clone(), option, !checked, cx)
                    }))
            })
            .when(!enabled, |element| element.opacity(0.55))
            .child(checkbox(checked))
            .child(div().text_sm().text_color(rgb(INK)).child(label))
            .into_any_element()
    }

    fn render_about(&self, cx: &mut Context<Self>) -> AnyElement {
        let status_color = if self.status.online { SUCCESS } else { WARNING };

        let card =
            panel()
                .w(px(560.))
                .p_6()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_2xl()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(INK))
                                .child("ZeroTier GUI"),
                        )
                        .child(div().text_sm().text_color(rgb(MUTED)).child(
                            "An unofficial Rust + GPUI rewrite of the original ZeroTier-GUI.",
                        )),
                )
                .child(div().h(px(1.)).bg(rgb(BORDER)))
                .child(field(
                    "My ZeroTier Address:",
                    self.status.display_address(),
                    true,
                ))
                .child(field(
                    "ZeroTier Version:",
                    self.status.display_version(),
                    true,
                ))
                .child(field(
                    "ZeroTier GUI Version:",
                    env!("CARGO_PKG_VERSION").to_string(),
                    true,
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_sm()
                        .child(
                            div()
                                .w(px(170.))
                                .flex_none()
                                .text_color(rgb(MUTED))
                                .child("Status:"),
                        )
                        .child(theme::dot(status_color))
                        .child(
                            div()
                                .font_family(MONO_FONT)
                                .text_color(rgb(status_color))
                                .child(self.status.display_state()),
                        ),
                )
                .child(div().h(px(1.)).bg(rgb(BORDER)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_sm()
                        .text_color(rgb(MUTED))
                        .child("Original ZeroTier-GUI by Tomás Ralph")
                        .child("Rust + GPUI rewrite by Senasgr")
                        .child(
                            div()
                                .id("repository-link")
                                .text_color(rgb(LINK))
                                .cursor_pointer()
                                .hover(|style| style.underline())
                                .on_click(|_: &ClickEvent, _: &mut Window, _: &mut App| {
                                    zerotier::open_repository()
                                })
                                .child("github.com/senasgr-eth/zerotier-gui-rs"),
                        )
                        .child(
                            div()
                                .id("original-link")
                                .text_color(rgb(LINK))
                                .cursor_pointer()
                                .hover(|style| style.underline())
                                .on_click(|_: &ClickEvent, _: &mut Window, _: &mut App| {
                                    zerotier::open_original_project()
                                })
                                .child("Original project: github.com/tralph3/ZeroTier-GUI"),
                        ),
                );

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(self.screen_header(
                "About",
                "Application and ZeroTier client information.",
                vec![self.back_button(cx)],
            ))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_start()
                    .justify_center()
                    .child(card),
            )
            .into_any_element()
    }

    fn render_startup(&self, cx: &mut Context<Self>) -> AnyElement {
        let (title, message) = match &self.startup {
            StartupState::Checking => (
                "Checking ZeroTier…",
                "Looking for zerotier-cli and the zerotier-one service.".to_string(),
            ),
            StartupState::ServiceDown => (
                "ZeroTier service is stopped",
                if theme::LINUX {
                    "The 'zerotier-one' service isn't running.\n\n\
                     Do you wish to grant root access to enable it?"
                        .to_string()
                } else {
                    "The 'zerotier-one' service isn't running.\n\n\
                     Start it with your operating system's service manager, then press Retry."
                        .to_string()
                },
            ),
            StartupState::NoAccess { has_token: false } => (
                if theme::LINUX {
                    "No authtoken found"
                } else {
                    "ZeroTier access denied"
                },
                if theme::LINUX {
                    "No authtoken.secret file has been found in \"/var/lib/zerotier-one\".\n\n\
                     This usually means you never started the zerotier-one service. \
                     Do you wish to start it now?"
                        .to_string()
                } else {
                    "This user doesn't have access to ZeroTier.\n\n\
                     Check the ZeroTier service permissions for your operating system, \
                     then press Retry."
                        .to_string()
                },
            ),
            StartupState::NoAccess { has_token: true } => (
                "Root access needed",
                format!(
                    "In order to grant {} permission to use ZeroTier we need temporary \
                     root access to add them to the zerotier-one group and change the \
                     auth-token permissions in /var/lib/zerotier-one.\n\n\
                     Otherwise, you would need to run this program as root. Grant access?",
                    zerotier::username()
                ),
            ),
            StartupState::NotInstalled => (
                "ZeroTier isn't installed",
                if theme::LINUX {
                    "Install ZeroTier One and make sure 'zerotier-cli' is on your PATH.\n\n\
                     On Arch Linux: sudo pacman -S zerotier-one\n\
                     On Debian/Ubuntu: see https://www.zerotier.com/download/"
                        .to_string()
                } else {
                    "Install ZeroTier One from https://www.zerotier.com/download/ and \
                     make sure 'zerotier-cli' is on your PATH."
                        .to_string()
                },
            ),
            StartupState::Failed(message) => ("Couldn't reach ZeroTier", message.clone()),
            StartupState::Ready => ("Ready", String::new()),
        };

        let mut buttons: Vec<AnyElement> = Vec::new();

        match &self.startup {
            StartupState::ServiceDown if theme::LINUX => {
                buttons.push(
                    theme::button(
                        "startup-start-service",
                        "Start Service",
                        theme::ButtonKind::Primary,
                        !self.busy.startup,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.start_service_from_startup(cx)
                        }),
                    )
                    .into_any_element(),
                );
            }
            StartupState::NoAccess { has_token: false } if theme::LINUX => {
                buttons.push(
                    theme::button(
                        "startup-start-service",
                        "Start Service",
                        theme::ButtonKind::Primary,
                        !self.busy.startup,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.start_service_from_startup(cx)
                        }),
                    )
                    .into_any_element(),
                );
            }
            StartupState::NoAccess { has_token: true } if theme::LINUX => {
                if zerotier::is_root() {
                    buttons.push(
                        theme::button(
                            "startup-retry",
                            "Retry",
                            theme::ButtonKind::Primary,
                            !self.busy.startup,
                            cx.listener(|this, _: &ClickEvent, _window, cx| this.begin_startup(cx)),
                        )
                        .into_any_element(),
                    );
                } else {
                    buttons.push(
                        theme::button(
                            "startup-grant",
                            "Grant Access",
                            theme::ButtonKind::Primary,
                            !self.busy.startup,
                            cx.listener(|this, _: &ClickEvent, _window, cx| this.grant_access(cx)),
                        )
                        .into_any_element(),
                    );
                }
            }
            StartupState::NotInstalled | StartupState::Failed(_) => {
                buttons.push(
                    theme::button(
                        "startup-retry",
                        "Retry",
                        theme::ButtonKind::Primary,
                        !self.busy.startup,
                        cx.listener(|this, _: &ClickEvent, _window, cx| this.begin_startup(cx)),
                    )
                    .into_any_element(),
                );
            }
            StartupState::Checking
            | StartupState::Ready
            | StartupState::ServiceDown
            | StartupState::NoAccess { .. } => {}
        }

        if matches!(
            self.startup,
            StartupState::ServiceDown | StartupState::NoAccess { has_token: false }
        ) || matches!(self.startup, StartupState::NoAccess { has_token: true })
            && !zerotier::is_root()
        {
            buttons.push(
                theme::button(
                    "startup-retry-secondary",
                    "Retry",
                    theme::ButtonKind::Secondary,
                    !self.busy.startup,
                    cx.listener(|this, _: &ClickEvent, _window, cx| this.begin_startup(cx)),
                )
                .into_any_element(),
            );
        }

        buttons.push(
            theme::button(
                "startup-quit",
                "Quit",
                theme::ButtonKind::Ghost,
                true,
                |_: &ClickEvent, _: &mut Window, cx: &mut App| cx.quit(),
            )
            .into_any_element(),
        );

        let mut card = panel()
            .w(px(520.))
            .p_6()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(INK))
                            .child("ZeroTier-GUI"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child("A cross-platform front-end for ZeroTier"),
                    ),
            )
            .child(div().h(px(1.)).bg(rgb(BORDER)))
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(INK))
                    .child(title),
            )
            .child(div().text_sm().text_color(rgb(MUTED)).child(message));

        if self.busy.startup && self.startup != StartupState::Checking {
            card = card.child(div().text_xs().text_color(rgb(MUTED)).child("Working…"));
        }

        if let Some(error) = &self.startup_error {
            card = card.child(
                div()
                    .rounded_md()
                    .bg(rgb(DANGER_SOFT))
                    .border_1()
                    .border_color(rgb(0xd99c9c))
                    .p_3()
                    .text_xs()
                    .text_color(rgb(DANGER_INK))
                    .child(error.clone()),
            );
        }

        card = card.child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                .children(buttons),
        );

        div()
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .child(card)
            .into_any_element()
    }

    fn render_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = &self.dialog else {
            return div().into_any_element();
        };

        let is_confirm = dialog.action.is_some();
        let header_color = if dialog.error { DANGER } else { 0x3b3b3b };

        let confirm_button = if is_confirm {
            theme::button(
                "dialog-confirm",
                "Leave",
                theme::ButtonKind::Danger,
                !self.busy.command,
                cx.listener(|this, _: &ClickEvent, _window, cx| this.confirm_dialog(cx)),
            )
            .into_any_element()
        } else {
            theme::button(
                "dialog-ok",
                "OK",
                theme::ButtonKind::Primary,
                true,
                cx.listener(|this, _: &ClickEvent, _window, cx| this.dismiss_dialog(cx)),
            )
            .into_any_element()
        };

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000066))
            .child(
                div()
                    .w(px(460.))
                    .occlude()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(0xf8f8f8))
                    .shadow_lg()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .h(px(42.))
                            .px_5()
                            .bg(rgb(header_color))
                            .text_color(rgb(0xffffff))
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(dialog.title.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .p_5()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(INK))
                                    .child(dialog.message.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .gap_2()
                                    .when(is_confirm, |element| {
                                        element.child(theme::button(
                                            "dialog-cancel",
                                            "Cancel",
                                            theme::ButtonKind::Secondary,
                                            true,
                                            cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                this.dismiss_dialog(cx)
                                            }),
                                        ))
                                    })
                                    .child(confirm_button),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_toast(&self) -> Option<AnyElement> {
        let toast = self.toast.as_ref()?;

        Some(
            div()
                .absolute()
                .bottom_4()
                .right_4()
                .max_w(px(520.))
                .px_4()
                .py_3()
                .rounded_md()
                .shadow_lg()
                .bg(rgb(if toast.error { DANGER } else { TOAST_BG }))
                .text_color(rgb(0xffffff))
                .text_sm()
                .child(toast.message.clone())
                .into_any_element(),
        )
    }
}

impl Focusable for ZeroTierGui {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ZeroTierGui {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let topbar = self.render_topbar(cx);
        let screen = self.render_screen(cx);
        let statusbar = self.render_statusbar(cx);
        let dialog = self.dialog.as_ref().map(|_| self.render_dialog(cx));
        let toast = self.render_toast();

        div()
            .id("app-root")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(BG))
            .font_family(UI_FONT)
            .text_color(rgb(INK))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_key(event, window, cx)
            }))
            .child(topbar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .px_5()
                    .pb_3()
                    .child(screen),
            )
            .child(statusbar)
            .children(dialog)
            .children(toast)
    }
}

// ------------------------------------------------------------------ helpers

fn table_header(columns: &[(&'static str, Option<f32>)]) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .h(px(30.))
        .px_3()
        .gap_2()
        .bg(rgb(SURFACE_ALT))
        .border_b_1()
        .border_color(rgb(BORDER))
        .children(columns.iter().map(|(label, width)| {
            let mut cell = div()
                .min_w_0()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(MUTED))
                .child(*label);

            if let Some(width) = width {
                cell = cell.w(px(*width)).flex_none();
            } else {
                cell = cell.flex_1();
            }

            cell
        }))
}

fn table_cell(
    content: impl Into<SharedString>,
    width: Option<f32>,
    monospace: bool,
    color: u32,
) -> Div {
    let mut cell = div()
        .min_w_0()
        .truncate()
        .text_sm()
        .text_color(rgb(color))
        .when(monospace, |element| element.font_family(MONO_FONT))
        .child(content.into());

    if let Some(width) = width {
        cell = cell.w(px(width)).flex_none();
    } else {
        cell = cell.flex_1();
    }

    cell
}

fn bool_label(value: bool) -> String {
    if value {
        "yes".to_string()
    } else {
        "no".to_string()
    }
}

fn bool_yes_no(value: bool) -> String {
    if value {
        "Yes".to_string()
    } else {
        "No".to_string()
    }
}
