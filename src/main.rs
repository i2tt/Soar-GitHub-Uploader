#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
struct Config {
    token: String,
}

#[derive(Deserialize, Clone, Default)]
struct GitHubUser {
    login: String,
    name: Option<String>,
    public_repos: u64,
    html_url: String,
}

#[derive(Deserialize, Clone)]
struct RepoItem {
    name: String,
    private: bool,
    html_url: String,
    description: Option<String>,
    stargazers_count: u64,
}

#[derive(Deserialize)]
struct RepoResponse {
    html_url: String,
    clone_url: String,
}

#[derive(Deserialize)]
struct LicenseInfo {
    body: String,
}

const CONFIG_FILE: &str = "config.json";

fn load_saved_token() -> Option<String> {
    if Path::new(CONFIG_FILE).exists() {
        if let Ok(content) = fs::read_to_string(CONFIG_FILE) {
            if let Ok(cfg) = serde_json::from_str::<Config>(&content) {
                let trimmed = cfg.token.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }
    None
}

fn save_token_to_file(token: &str) {
    let cfg = Config {
        token: token.to_string(),
    };
    if let Ok(serialized) = serde_json::to_string_pretty(&cfg) {
        let _ = fs::write(CONFIG_FILE, serialized);
    }
}

fn api_fetch_user(token: &str) -> Result<GitHubUser, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("Soar-GitHub-Uploader")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get("https://api.github.com/user")
        .bearer_auth(token)
        .send()
        .map_err(|e| e.to_string())?;

    if resp.status().is_success() {
        resp.json::<GitHubUser>().map_err(|e| e.to_string())
    } else if resp.status().as_u16() == 401 {
        Err("Authentication failed. Invalid Personal Access Token.".to_string())
    } else {
        Err(format!("GitHub API returned HTTP {}", resp.status()))
    }
}

fn api_fetch_repos(token: &str) -> Result<Vec<RepoItem>, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("Soar-GitHub-Uploader")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get("https://api.github.com/user/repos?type=all&sort=updated&per_page=100")
        .bearer_auth(token)
        .send()
        .map_err(|e| e.to_string())?;

    if resp.status().is_success() {
        resp.json::<Vec<RepoItem>>().map_err(|e| e.to_string())
    } else {
        Err(format!("Failed to list repositories: HTTP {}", resp.status()))
    }
}

fn api_create_repo(
    token: &str,
    name: &str,
    description: &str,
    is_private: bool,
) -> Result<RepoResponse, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("Soar-GitHub-Uploader")
        .build()
        .map_err(|e| e.to_string())?;

    let mut body = serde_json::json!({
        "name": name,
        "private": is_private,
        "auto_init": false
    });

    if !description.trim().is_empty() {
        body["description"] = serde_json::Value::String(description.to_string());
    }

    let resp = client
        .post("https://api.github.com/user/repos")
        .bearer_auth(token)
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;

    if resp.status().is_success() {
        resp.json::<RepoResponse>().map_err(|e| e.to_string())
    } else {
        let raw = resp.text().unwrap_or_default();
        Err(format!("Creation failed: {}", raw))
    }
}

fn api_delete_repo(token: &str, owner: &str, repo: &str) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("Soar-GitHub-Uploader")
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("https://api.github.com/repos/{}/{}", owner, repo);
    let resp = client
        .delete(&url)
        .bearer_auth(token)
        .send()
        .map_err(|e| e.to_string())?;

    if resp.status().is_success() || resp.status().as_u16() == 204 {
        Ok(())
    } else if resp.status().as_u16() == 403 || resp.status().as_u16() == 404 {
        Err("Deletion failed. Ensure your token has the 'delete_repo' scope enabled on GitHub.".to_string())
    } else {
        Err(format!("Deletion failed with HTTP {}", resp.status()))
    }
}

fn api_fetch_license(key: &str) -> Option<String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("Soar-GitHub-Uploader")
        .build()
        .ok()?;

    let url = format!("https://api.github.com/licenses/{}", key);
    let resp = client.get(&url).send().ok()?;
    if resp.status().is_success() {
        if let Ok(info) = resp.json::<LicenseInfo>() {
            return Some(info.body);
        }
    }
    None
}

fn git_push_folder(folder: &Path, token: &str, clone_url: &str, user_login: &str) -> Result<(), String> {
    let auth_url = if clone_url.starts_with("https://") {
        clone_url.replace("https://", &format!("https://{}@", token))
    } else {
        clone_url.to_string()
    };

    let git_dir = folder.join(".git");
    if !git_dir.exists() {
        let status = Command::new("git")
            .arg("init")
            .current_dir(folder)
            .status()
            .map_err(|e| format!("git init error: {}", e))?;
        if !status.success() {
            return Err("git init failed".to_string());
        }
    }

    let email = format!("{}@users.noreply.github.com", user_login);
    let _ = Command::new("git")
        .args(["config", "user.name", user_login])
        .current_dir(folder)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", &email])
        .current_dir(folder)
        .status();

    let _ = Command::new("git")
        .args(["branch", "-M", "main"])
        .current_dir(folder)
        .status();

    let status = Command::new("git")
        .args(["add", "-A"])
        .current_dir(folder)
        .status()
        .map_err(|e| format!("git add error: {}", e))?;
    if !status.success() {
        return Err("git add failed".to_string());
    }

    let _ = Command::new("git")
        .args([
            "-c", &format!("user.name={}", user_login),
            "-c", &format!("user.email={}", email),
            "commit", "-m", "Initial commit",
        ])
        .current_dir(folder)
        .status();

    let _ = Command::new("git")
        .args(["remote", "remove", "origin"])
        .current_dir(folder)
        .status();

    let status = Command::new("git")
        .args(["remote", "add", "origin", &auth_url])
        .current_dir(folder)
        .status()
        .map_err(|e| format!("git remote add error: {}", e))?;
    if !status.success() {
        return Err("git remote add failed".to_string());
    }

    let status = Command::new("git")
        .args(["push", "-f", "-u", "origin", "main"])
        .current_dir(folder)
        .status()
        .map_err(|e| format!("git push error: {}", e))?;

    let _ = Command::new("git")
        .args(["remote", "set-url", "origin", clone_url])
        .current_dir(folder)
        .status();

    if status.success() {
        Ok(())
    } else {
        Err("git push failed. Check your token permissions (repo scope required).".to_string())
    }
}

#[derive(PartialEq)]
enum ActiveTab {
    Publish,
    Manage,
    Profile,
    Credits,
}

struct SoarApp {
    token_input: String,
    authenticated: bool,
    user: GitHubUser,
    active_tab: ActiveTab,

    repo_name: String,
    repo_desc: String,
    repo_private: bool,
    license_choice: usize,
    folder_path: String,

    my_repos: Vec<RepoItem>,
    confirm_delete: Option<String>,

    status_message: String,
    is_error: bool,
}

impl Default for SoarApp {
    fn default() -> Self {
        let mut app = Self {
            token_input: String::new(),
            authenticated: false,
            user: GitHubUser::default(),
            active_tab: ActiveTab::Publish,

            repo_name: String::new(),
            repo_desc: String::new(),
            repo_private: false,
            license_choice: 0,
            folder_path: ".".to_string(),

            my_repos: Vec::new(),
            confirm_delete: None,

            status_message: String::new(),
            is_error: false,
        };

        if let Some(token) = load_saved_token() {
            app.token_input = token.clone();
            if let Ok(user) = api_fetch_user(&token) {
                app.user = user;
                app.authenticated = true;
            }
        }

        app
    }
}

impl eframe::App for SoarApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.window_rounding = egui::Rounding::same(8.0);
        style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(6.0);
        style.visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);
        style.visuals.widgets.hovered.rounding = egui::Rounding::same(6.0);
        style.visuals.widgets.active.rounding = egui::Rounding::same(6.0);
        ctx.set_style(style);

        egui::TopBottomPanel::top("top_header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("⚡ SOAR GITHUB UPLOADER").size(20.0).strong().color(egui::Color32::from_rgb(56, 189, 248)));
                ui.label(egui::RichText::new("v3.5").size(12.0).color(egui::Color32::GRAY));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.authenticated {
                        if ui.button("🔑 Switch Token").clicked() {
                            self.authenticated = false;
                            self.token_input.clear();
                            let _ = fs::remove_file(CONFIG_FILE);
                        }
                        ui.label(
                            egui::RichText::new(format!("● Logged in as: {}", self.user.login))
                                .size(13.0)
                                .color(egui::Color32::from_rgb(74, 222, 128))
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("○ Not Authenticated")
                                .size(13.0)
                                .color(egui::Color32::from_rgb(248, 113, 113))
                        );
                    }
                });
            });
            ui.add_space(4.0);
        });

        if !self.authenticated {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.label(egui::RichText::new("🔑 GitHub Authentication").size(24.0).strong().color(egui::Color32::WHITE));
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Enter your Personal Access Token with 'repo' & 'delete_repo' scopes to get started.")
                            .size(14.0)
                            .color(egui::Color32::LIGHT_GRAY)
                    );
                    ui.add_space(24.0);

                    ui.horizontal(|ui| {
                        ui.add_space(ui.available_width() / 4.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.token_input)
                                .password(true)
                                .hint_text("ghp_xxxxxxxxxxxxxxxxxxxx")
                                .desired_width(320.0)
                        );
                    });

                    ui.add_space(16.0);

                    if ui.add_sized([160.0, 36.0], egui::Button::new(egui::RichText::new("⚡ Authenticate").size(15.0).strong())).clicked() {
                        let token = self.token_input.trim().to_string();
                        if token.is_empty() {
                            self.status_message = "Please enter a valid GitHub token.".to_string();
                            self.is_error = true;
                        } else {
                            match api_fetch_user(&token) {
                                Ok(user) => {
                                    self.user = user;
                                    self.authenticated = true;
                                    save_token_to_file(&token);
                                    self.status_message = format!("Welcome back, {}!", self.user.login);
                                    self.is_error = false;
                                }
                                Err(err) => {
                                    self.status_message = err;
                                    self.is_error = true;
                                }
                            }
                        }
                    }

                    if !self.status_message.is_empty() {
                        ui.add_space(16.0);
                        let col = if self.is_error {
                            egui::Color32::from_rgb(248, 113, 113)
                        } else {
                            egui::Color32::from_rgb(74, 222, 128)
                        };
                        ui.label(egui::RichText::new(&self.status_message).size(14.0).color(col));
                    }
                });
            });
            return;
        }

        egui::TopBottomPanel::top("nav_tabs").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.selectable_label(self.active_tab == ActiveTab::Publish, "⚡ Publish Repository").clicked() {
                    self.active_tab = ActiveTab::Publish;
                }
                if ui.selectable_label(self.active_tab == ActiveTab::Manage, "🗑 Manage & Delete Repos").clicked() {
                    self.active_tab = ActiveTab::Manage;
                    if let Ok(repos) = api_fetch_repos(&self.token_input) {
                        self.my_repos = repos;
                    }
                }
                if ui.selectable_label(self.active_tab == ActiveTab::Profile, "👤 Profile").clicked() {
                    self.active_tab = ActiveTab::Profile;
                }
                if ui.selectable_label(self.active_tab == ActiveTab::Credits, "📜 Credits").clicked() {
                    self.active_tab = ActiveTab::Credits;
                }
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                ActiveTab::Publish => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("⚡ Create & Push New Repository").size(18.0).strong().color(egui::Color32::WHITE));
                        ui.add_space(12.0);

                        egui::Grid::new("publish_grid").num_columns(2).spacing([16.0, 12.0]).show(ui, |ui| {
                            ui.label(egui::RichText::new("Repository Name:").strong());
                            ui.add(egui::TextEdit::singleline(&mut self.repo_name).hint_text("e.g. My-Project").desired_width(320.0));
                            ui.end_row();

                            ui.label(egui::RichText::new("Description:").strong());
                            ui.add(egui::TextEdit::singleline(&mut self.repo_desc).hint_text("Optional project description").desired_width(320.0));
                            ui.end_row();

                            ui.label(egui::RichText::new("Visibility:").strong());
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut self.repo_private, false, "🌐 Public");
                                ui.radio_value(&mut self.repo_private, true, "🔒 Private");
                            });
                            ui.end_row();

                            ui.label(egui::RichText::new("License:").strong());
                            egui::ComboBox::from_id_salt("license_combo")
                                .selected_text(match self.license_choice {
                                    1 => "MIT",
                                    2 => "Apache-2.0",
                                    3 => "GPL-3.0",
                                    4 => "Unlicense",
                                    _ => "None",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.license_choice, 0, "None");
                                    ui.selectable_value(&mut self.license_choice, 1, "MIT");
                                    ui.selectable_value(&mut self.license_choice, 2, "Apache-2.0");
                                    ui.selectable_value(&mut self.license_choice, 3, "GPL-3.0");
                                    ui.selectable_value(&mut self.license_choice, 4, "Unlicense");
                                });
                            ui.end_row();

                            ui.label(egui::RichText::new("Folder Path:").strong());
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut self.folder_path).hint_text(". for current directory").desired_width(260.0));
                                if ui.button("📂 Current").clicked() {
                                    self.folder_path = ".".to_string();
                                }
                            });
                            ui.end_row();
                        });

                        ui.add_space(20.0);

                        let btn = egui::Button::new(egui::RichText::new("🚀 Create & Push to GitHub").size(15.0).strong());
                        if ui.add_sized([260.0, 40.0], btn).clicked() {
                            let name = self.repo_name.trim().to_string();
                            if name.is_empty() {
                                self.status_message = "Please enter a repository name.".to_string();
                                self.is_error = true;
                            } else {
                                let folder = PathBuf::from(&self.folder_path);
                                if !folder.exists() || !folder.is_dir() {
                                    self.status_message = format!("Folder does not exist: {}", folder.display());
                                    self.is_error = true;
                                } else {
                                    if self.license_choice > 0 {
                                        let lic_key = match self.license_choice {
                                            1 => "mit",
                                            2 => "apache-2.0",
                                            3 => "gpl-3.0",
                                            _ => "unlicense",
                                        };
                                        let lic_file = folder.join("LICENSE");
                                        if !lic_file.exists() {
                                            if let Some(body) = api_fetch_license(lic_key) {
                                                let _ = fs::write(&lic_file, body);
                                            }
                                        }
                                    }

                                    match api_create_repo(&self.token_input, &name, &self.repo_desc, self.repo_private) {
                                        Ok(repo) => {
                                            match git_push_folder(&folder, &self.token_input, &repo.clone_url, &self.user.login) {
                                                Ok(_) => {
                                                    self.status_message = format!("Successfully published to: {}", repo.html_url);
                                                    self.is_error = false;
                                                }
                                                Err(e) => {
                                                    self.status_message = format!("Repo created ({}), but push failed: {}", repo.html_url, e);
                                                    self.is_error = true;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            self.status_message = e;
                                            self.is_error = true;
                                        }
                                    }
                                }
                            }
                        }

                        if !self.status_message.is_empty() {
                            ui.add_space(16.0);
                            let col = if self.is_error {
                                egui::Color32::from_rgb(248, 113, 113)
                            } else {
                                egui::Color32::from_rgb(74, 222, 128)
                            };
                            ui.label(egui::RichText::new(&self.status_message).size(14.0).color(col));
                        }
                    });
                }
                ActiveTab::Manage => {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("🗑 Your Repositories").size(18.0).strong().color(egui::Color32::WHITE));
                        if ui.button("🔄 Refresh List").clicked() {
                            if let Ok(repos) = api_fetch_repos(&self.token_input) {
                                self.my_repos = repos;
                            }
                        }
                    });
                    ui.add_space(8.0);

                    if let Some(repo_name) = self.confirm_delete.clone() {
                        ui.group(|ui| {
                            ui.label(egui::RichText::new(format!("⚠️ Are you sure you want to PERMANENTLY delete '{}'?", repo_name)).color(egui::Color32::from_rgb(248, 113, 113)).strong());
                            ui.horizontal(|ui| {
                                if ui.button("Confirm Delete").clicked() {
                                    match api_delete_repo(&self.token_input, &self.user.login, &repo_name) {
                                        Ok(_) => {
                                            self.status_message = format!("Deleted repository '{}'.", repo_name);
                                            self.is_error = false;
                                            self.my_repos.retain(|r| r.name != repo_name);
                                        }
                                        Err(err) => {
                                            self.status_message = err;
                                            self.is_error = true;
                                        }
                                    }
                                    self.confirm_delete = None;
                                }
                                if ui.button("Cancel").clicked() {
                                    self.confirm_delete = None;
                                }
                            });
                        });
                        ui.add_space(8.0);
                    }

                    if !self.status_message.is_empty() {
                        let col = if self.is_error {
                            egui::Color32::from_rgb(248, 113, 113)
                        } else {
                            egui::Color32::from_rgb(74, 222, 128)
                        };
                        ui.label(egui::RichText::new(&self.status_message).size(13.0).color(col));
                        ui.add_space(8.0);
                    }

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for repo in &self.my_repos {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    let badge = if repo.private { "🔒 Private" } else { "🌐 Public" };
                                    ui.label(egui::RichText::new(&repo.name).strong().size(15.0));
                                    ui.label(egui::RichText::new(badge).size(12.0).color(egui::Color32::GRAY));
                                    ui.label(egui::RichText::new(format!("★ {}", repo.stargazers_count)).size(12.0).color(egui::Color32::KHAKI));

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.add(egui::Button::new(egui::RichText::new("🗑 Delete").color(egui::Color32::from_rgb(248, 113, 113)))).clicked() {
                                            self.confirm_delete = Some(repo.name.clone());
                                        }
                                        ui.hyperlink_to("🔗 Open", &repo.html_url);
                                    });
                                });
                                if let Some(ref desc) = repo.description {
                                    ui.label(egui::RichText::new(desc).size(12.0).color(egui::Color32::LIGHT_GRAY));
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
                }
                ActiveTab::Profile => {
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new("👤 GitHub Profile").size(18.0).strong().color(egui::Color32::WHITE));
                    ui.add_space(16.0);
                    egui::Grid::new("profile_grid").num_columns(2).spacing([16.0, 12.0]).show(ui, |ui| {
                        ui.label(egui::RichText::new("Username:").strong());
                        ui.label(&self.user.login);
                        ui.end_row();

                        if let Some(ref name) = self.user.name {
                            ui.label(egui::RichText::new("Name:").strong());
                            ui.label(name);
                            ui.end_row();
                        }

                        ui.label(egui::RichText::new("Public Repositories:").strong());
                        ui.label(self.user.public_repos.to_string());
                        ui.end_row();

                        ui.label(egui::RichText::new("Profile URL:").strong());
                        ui.hyperlink(&self.user.html_url);
                        ui.end_row();
                    });
                }
                ActiveTab::Credits => {
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new("📜 Credits").size(18.0).strong().color(egui::Color32::WHITE));
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new("Soar GitHub Uploader").size(16.0).color(egui::Color32::from_rgb(56, 189, 248)));
                    ui.add_space(8.0);
                    ui.label("Developer : i2tt");
                    ui.label("Developer : K2CV");
                    ui.label("Team      : Soar Team");
                    ui.add_space(8.0);
                    ui.hyperlink_to("🔗 GitHub Repository", "https://github.com/i2tt/Soar-GitHub-Uploader");
                }
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([880.0, 620.0])
            .with_min_inner_size([640.0, 480.0])
            .with_title("Soar GitHub Uploader - Made By i2tt & K2CV"),
        ..Default::default()
    };

    eframe::run_native(
        "Soar GitHub Uploader",
        native_options,
        Box::new(|_cc| Ok(Box::new(SoarApp::default()))),
    )
}
