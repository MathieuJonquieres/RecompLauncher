//! Local friends book. Entirely offline; no account or remote service.

use eframe::egui::{self, RichText};
use recomp_core::i18n::I18n;
use recomp_core::online::Friend;
use tokio::sync::mpsc::UnboundedSender;

use crate::backend_bridge::Command;

#[derive(Default)]
pub struct FriendsView {
    pub friends: Vec<Friend>,
    loaded: bool,
    requested: bool,
    display_name: String,
    notes: String,
    service: String,
    handle: String,
}

impl FriendsView {
    pub fn apply(&mut self, friends: Vec<Friend>) {
        self.friends = friends;
        self.loaded = true;
    }

    pub fn show_central(
        &mut self,
        ui: &mut egui::Ui,
        commands: &UnboundedSender<Command>,
        i18n: &I18n,
    ) {
        ui.heading(i18n.text("friends-title"));
        ui.label(
            RichText::new(i18n.text("friends-description"))
                .small()
                .weak(),
        );
        ui.add_space(10.0);
        if !self.loaded {
            if !self.requested {
                self.requested = true;
                let _ = commands.send(Command::LoadFriends);
            }
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(i18n.text("friends-loading")).small().weak());
            });
            return;
        }

        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.display_name)
                    .hint_text(i18n.text("friends-display-name"))
                    .desired_width(150.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.notes)
                    .hint_text(i18n.text("friends-notes"))
                    .desired_width(180.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.service)
                    .hint_text(i18n.text("friends-service"))
                    .desired_width(150.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.handle)
                    .hint_text(i18n.text("friends-handle"))
                    .desired_width(150.0),
            );
            if ui.button(i18n.text("friends-add")).clicked() {
                let name = self.display_name.trim().to_string();
                if !name.is_empty() {
                    let handles = if self.service.trim().is_empty() || self.handle.trim().is_empty()
                    {
                        Vec::new()
                    } else {
                        vec![(
                            self.service.trim().to_string(),
                            self.handle.trim().to_string(),
                        )]
                    };
                    let notes = self.notes.trim();
                    let _ = commands.send(Command::AddFriend {
                        display_name: name,
                        notes: (!notes.is_empty()).then(|| notes.to_string()),
                        handles,
                    });
                    self.display_name.clear();
                    self.notes.clear();
                    self.service.clear();
                    self.handle.clear();
                }
            }
            if ui.button(i18n.text("friends-refresh")).clicked() {
                let _ = commands.send(Command::LoadFriends);
            }
        });
        ui.separator();

        if self.friends.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(i18n.text("friends-empty")).weak());
            });
            return;
        }
        for friend in &self.friends {
            ui.horizontal_wrapped(|ui| {
                ui.strong(&friend.display_name);
                for (service, handle) in &friend.handles {
                    ui.label(RichText::new(format!("{service}: {handle}")).small().weak());
                }
                if let Some(notes) = &friend.notes {
                    ui.label(RichText::new(notes).small());
                }
                if ui.button(i18n.text("friends-remove")).clicked() {
                    let _ = commands.send(Command::RemoveFriend { id: friend.id });
                }
            });
        }
    }
}
