use std::{sync::mpsc, thread};

use crate::{games, themes, widgets};

pub enum State {
    OutOfDate(self_update::Status),
    Menu,
    Login,
    Waiting(String),
    Game,
    Achievements(Vec<u32>),
    Pulls(String),
    Error(String),
}

pub enum Message {
    GoTo(State),
    Updated(Option<self_update::Status>),
    LoggedIn(User),
    Error(String),
    Toast(egui_notify::Toast),
    Achievements(Vec<u32>),
    Pulls(String),
}

pub struct App {
    theme: themes::Theme,
    message_tx: mpsc::Sender<Message>,
    message_rx: mpsc::Receiver<Message>,
    state: State,
    game: games::Game,
    username: String,
    password: String,
    user: Option<User>,
    toasts: egui_notify::Toasts,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    id: String,
    username: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();

        fonts.font_data.insert(
            "JetBrainsMonoNerdFont".to_string(),
            egui::FontData::from_static(include_bytes!("../JetBrainsMonoNerdFont-Regular.ttf")),
        );

        fonts.font_data.insert(
            "JetBrainsMonoNerdFontMono".to_string(),
            egui::FontData::from_static(include_bytes!("../JetBrainsMonoNerdFontMono-Regular.ttf")),
        );

        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "JetBrainsMonoNerdFont".to_string());

        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "JetBrainsMonoNerdFontMono".to_string());

        cc.egui_ctx.set_fonts(fonts);

        let user = if let Some(storage) = cc.storage {
            eframe::get_value(storage, "user").unwrap_or_default()
        } else {
            None
        };

        let theme: themes::Theme = cc
            .storage
            .and_then(|s| eframe::get_value(s, "theme"))
            .unwrap_or_default();

        cc.egui_ctx.set_style(theme.style());

        let (message_tx, message_rx) = mpsc::channel();

        update(&message_tx);

        Self {
            theme,
            message_tx,
            message_rx,
            state: State::Waiting("Updating".to_string()),
            game: games::Game::Hsr,
            username: String::new(),
            password: String::new(),
            user,
            toasts: egui_notify::Toasts::default().with_anchor(egui_notify::Anchor::BottomRight),
        }
    }

    fn message(&mut self, message: Message) {
        match message {
            Message::GoTo(state) => self.state = state,
            Message::Updated(status) => {
                if let Some(status) = status {
                    if status.updated() {
                        self.state = State::OutOfDate(status);
                    } else {
                        self.state = State::Menu;
                    }
                } else {
                    self.state = State::Error("Error updating".to_string());
                }
            }
            Message::LoggedIn(user) => {
                self.user = Some(user);
                self.state = State::Menu;
            }
            Message::Error(e) => self.state = State::Error(e),
            Message::Achievements(vec) => self.state = State::Achievements(vec),
            Message::Toast(toast) => {
                self.toasts.add(toast);
            }
            Message::Pulls(url) => self.state = State::Pulls(url),
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "user", &self.user);
        eframe::set_value(storage, "theme", &self.theme);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(message) = self.message_rx.try_recv() {
            self.message(message);
        }

        egui::TopBottomPanel::top("panel")
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.window_fill)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.add(widgets::Decorations);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if ui.button("Cycle Theme").clicked() {
                    match self.theme {
                        themes::Theme::Dark => self.theme = themes::Theme::Light,
                        themes::Theme::Light => self.theme = themes::Theme::Classic,
                        themes::Theme::Classic => self.theme = themes::Theme::Dark,
                    }

                    ctx.set_style(self.theme.style());
                }
            });

            match &self.state {
                State::Waiting(s) => {
                    ui.horizontal(|ui| {
                        ui.label(s);
                        ui.add(egui::Spinner::new().color(ui.visuals().text_color()))
                    });
                }
                State::OutOfDate(status) => {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Updated to Version {}. Restarting!",
                            status.version()
                        ))
                    });

                    let program_name = std::env::args().next().unwrap();
                    std::process::Command::new(program_name).spawn().unwrap();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                State::Login => {
                    if ui.button("Menu").clicked() {
                        self.state = State::Menu;
                    }

                    ui.label("Username:");
                    ui.text_edit_singleline(&mut self.username);

                    ui.label("Password:");
                    ui.add(egui::TextEdit::singleline(&mut self.password).password(true));

                    if ui.button("Login").clicked() {
                        login(&self.username, &self.password, &self.message_tx);
                        self.state = State::Waiting("Logging In".to_string());
                    }
                }
                State::Menu => {
                    if let Some(user) = &self.user {
                        ui.label(format!("Hi {}", user.username));

                        if ui.button("Logout").clicked() {
                            {
                                let id = user.id.clone();
                                thread::spawn(move || {
                                    let _ = ureq::post("https://stardb.gg/api/users/auth/logout")
                                        .set("Cookie", &id);
                                });
                            }

                            self.user = None;
                        }
                    } else if ui.button("Login").clicked() {
                        self.state = State::Login;
                    }

                    ui.add_space(10.0);

                    if ui.button("Honkai: Star Rail").clicked() {
                        self.game = games::Game::Hsr;
                        self.state = State::Game;
                    }

                    if ui.button("Genshin Impact").clicked() {
                        self.game = games::Game::Gi;
                        self.state = State::Game;
                    }

                    if ui.button("Zenless Zone Zero").clicked() {
                        self.game = games::Game::Zzz;
                        self.state = State::Game;
                    }
                }
                State::Achievements(achievements) => {
                    if ui.button("Menu").clicked() {
                        self.message_tx.send(Message::GoTo(State::Menu)).unwrap();
                    }

                    let key = match self.game {
                        games::Game::Hsr => {
                            ui.heading("HSR");
                            "hsr_achievements"
                        }
                        games::Game::Gi => {
                            ui.heading("GI");
                            "gi_achievements"
                        }
                        _ => unimplemented!(),
                    };

                    ui.label("Finished");

                    if ui
                        .button(format!(
                            "Copy {} achievements to clipboard",
                            achievements.len()
                        ))
                        .clicked()
                    {
                        if let Err(e) = arboard::Clipboard::new().and_then(|mut c| {
                            c.set_text(serde_json::json!({ key: achievements }).to_string())
                        }) {
                            self.message_tx.send(Message::Error(e.to_string())).unwrap();
                        } else {
                            self.toasts.success("Copied");
                        }
                    }

                    ui.hyperlink_to("Click here to import", "https://stardb.gg/import");

                    if let Some(user) = &self.user {
                        if ui
                            .button(format!("Sync to account: \"{}\"", user.username))
                            .clicked()
                        {
                            self.toasts.info("Syncing");

                            let prefix = match self.game {
                                games::Game::Hsr => "",
                                games::Game::Gi => "gi/",
                                _ => unimplemented!(),
                            };

                            let url = format!(
                                "https://stardb.gg/api/users/me/{prefix}achievements/completed"
                            );

                            {
                                let message_tx = self.message_tx.clone();
                                let id = user.id.clone();
                                let achievements = achievements.clone();

                                thread::spawn(move || {
                                    match ureq::put(&url).set("Cookie", &id).send_json(achievements)
                                    {
                                        Ok(r) => {
                                            if r.status() == 200 {
                                                message_tx
                                                    .send(Message::Toast(
                                                        egui_notify::Toast::success("Synced"),
                                                    ))
                                                    .unwrap();
                                            } else {
                                                message_tx
                                                    .send(Message::Toast(
                                                        egui_notify::Toast::error(
                                                            "Error. Try Relogging",
                                                        ),
                                                    ))
                                                    .unwrap();
                                            }
                                        }
                                        Err(e) => {
                                            message_tx.send(Message::Error(e.to_string())).unwrap();
                                        }
                                    }
                                });
                            }
                        }
                    }
                }
                State::Error(e) => {
                    if ui.button("Menu").clicked() {
                        self.message_tx.send(Message::GoTo(State::Menu)).unwrap();
                    }

                    ui.label(format!("Error: {e}"));
                }
                State::Game => {
                    if ui.button("Menu").clicked() {
                        self.state = State::Menu;
                    }

                    match self.game {
                        games::Game::Hsr => {
                            ui.heading("HSR");

                            if ui.button("Achievement Exporter").clicked() {
                                self.game.achievements(&self.message_tx);
                                self.state = State::Waiting("Preparing".to_string());
                            }

                            if ui.button("Warp Exporter").clicked() {
                                self.game.pulls(&self.message_tx);
                                self.state = State::Waiting("Running".to_string());
                            }
                        }
                        games::Game::Gi => {
                            ui.heading("GI");

                            if ui.button("Achievement Exporter").clicked() {
                                self.game.achievements(&self.message_tx);
                                self.state = State::Waiting("Preparing".to_string());
                            }

                            if ui.button("Wish Exporter").clicked() {
                                self.game.pulls(&self.message_tx);
                                self.state = State::Waiting("Running".to_string());
                            }
                        }
                        games::Game::Zzz => {
                            ui.heading("ZZZ");

                            if ui.button("Signal Exporter").clicked() {
                                self.game.pulls(&self.message_tx);
                                self.state = State::Waiting("Running".to_string());
                            }
                        }
                    }
                }
                State::Pulls(url) => {
                    if ui.button("Menu").clicked() {
                        self.message_tx.send(Message::GoTo(State::Menu)).unwrap();
                    }

                    ui.label("Finished");

                    if ui.button("Copy url to clipboard").clicked() {
                        if let Err(e) =
                            arboard::Clipboard::new().and_then(|mut c| c.set_text(url.clone()))
                        {
                            self.message_tx.send(Message::Error(e.to_string())).unwrap();
                        } else {
                            self.toasts.success("Copied");
                        }
                    }

                    let import_url = match self.game {
                        games::Game::Hsr => "https://stardb.gg/warp-import",
                        games::Game::Gi => "https://stardb.gg/genshin/wish-import",
                        games::Game::Zzz => "https://stardb.gg/zzz/signal-import",
                    };

                    ui.hyperlink_to("Click here to import", import_url);

                    if ui.button("Sync to stardb").clicked() {
                        self.toasts.info("Not yet implemented");
                    }
                }
            }
        });

        self.toasts.show(ctx);
    }
}

fn login(username: &str, password: &str, message_tx: &mpsc::Sender<Message>) {
    let username = username.to_string();
    let password = password.to_string();
    let message_tx = message_tx.clone();

    thread::spawn(move || {
        let json = ureq::json!({
            "username": username,
            "password": password
        });

        let id = ureq::post("https://stardb.gg/api/users/auth/login")
            .send_json(json)
            .ok()
            .and_then(|r| {
                r.header("Set-Cookie")
                    .and_then(|id| id.split(';').next())
                    .map(|s| s.to_string())
            });

        if let Some(id) = id {
            let username = username.to_string();

            let user = User { id, username };

            message_tx.send(Message::LoggedIn(user)).unwrap();
        } else {
            message_tx
                .send(Message::Error(
                    "There was an error during the login".to_string(),
                ))
                .unwrap();
        }
    });
}

fn update(message_tx: &mpsc::Sender<Message>) {
    let message_tx = message_tx.clone();

    thread::spawn(move || {
        let status = self_update::backends::github::Update::configure()
            .repo_owner("juliuskreutz")
            .repo_name("stardb-exporter")
            .bin_name("stardb-exporter")
            .current_version(self_update::cargo_crate_version!())
            .no_confirm(true)
            .build()
            .ok()
            .and_then(|e| e.update().ok());

        message_tx.send(Message::Updated(status)).unwrap();
    });
}