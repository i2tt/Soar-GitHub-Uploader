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
    #[serde(default)]
    avatar_url: String,
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

mod theme {
    use eframe::egui::Color32;

    pub const BG: Color32 = Color32::from_rgb(10, 10, 10);
    pub const SIDEBAR: Color32 = Color32::from_rgb(14, 14, 14);
    pub const CARD: Color32 = Color32::from_rgb(19, 19, 19);
    pub const RAISED: Color32 = Color32::from_rgb(28, 28, 28);
    pub const INPUT: Color32 = Color32::from_rgb(12, 12, 12);
    pub const BORDER: Color32 = Color32::from_rgb(38, 38, 38);
    pub const BORDER_STRONG: Color32 = Color32::from_rgb(64, 64, 64);

    pub const INK: Color32 = Color32::from_rgb(240, 238, 232);
    pub const INK_HI: Color32 = Color32::from_rgb(255, 255, 255);
    pub const ON_INK: Color32 = Color32::from_rgb(10, 10, 10);

    pub const TEXT: Color32 = Color32::from_rgb(240, 238, 232);
    pub const MUTED: Color32 = Color32::from_rgb(150, 150, 150);
    pub const FAINT: Color32 = Color32::from_rgb(96, 96, 96);

    pub const DANGER: Color32 = Color32::from_rgb(229, 96, 90);

    /// Premultiplies in gamma space, which is how egui blends, so `alpha` reads as expected.
    pub fn tint(c: Color32, alpha: u8) -> Color32 {
        let m = |x: u8| (x as u16 * alpha as u16 / 255) as u8;
        Color32::from_rgba_premultiplied(m(c.r()), m(c.g()), m(c.b()), alpha)
    }

    pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
    }
}

mod icons {
    use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};
    use std::f32::consts::{PI, TAU};

    #[derive(Clone, Copy)]
    pub enum Icon {
        Upload,
        Layers,
        Repo,
        User,
        Award,
        Key,
        Lock,
        Globe,
        Folder,
        Refresh,
        Star,
        External,
        Logout,
        Trash,
        Check,
        Alert,
        Minimize,
        Maximize,
        Restore,
        Close,
    }

    // Coordinates below are on a 24x24 grid, scaled to the target rect.
    struct Pen<'a> {
        painter: &'a Painter,
        origin: Pos2,
        s: f32,
        stroke: Stroke,
    }

    impl Pen<'_> {
        fn p(&self, x: f32, y: f32) -> Pos2 {
            self.origin + Vec2::new(x * self.s, y * self.s)
        }
        fn line(&self, pts: &[(f32, f32)]) {
            let pts = pts.iter().map(|&(x, y)| self.p(x, y)).collect();
            self.painter.add(Shape::line(pts, self.stroke));
        }
        fn closed(&self, pts: &[(f32, f32)]) {
            let pts = pts.iter().map(|&(x, y)| self.p(x, y)).collect();
            self.painter.add(Shape::closed_line(pts, self.stroke));
        }
        fn circle(&self, x: f32, y: f32, r: f32) {
            self.painter.circle_stroke(self.p(x, y), r * self.s, self.stroke);
        }
        fn dot(&self, x: f32, y: f32, r: f32) {
            self.painter.circle_filled(self.p(x, y), r * self.s, self.stroke.color);
        }
    }

    fn arc(cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32) -> Vec<(f32, f32)> {
        let n = 32;
        (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                (cx + rx * a.cos(), cy + ry * a.sin())
            })
            .collect()
    }

    pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
        let s = rect.width().min(rect.height()) / 24.0;
        let pen = Pen {
            painter,
            origin: rect.center() - Vec2::splat(12.0 * s),
            s,
            stroke: Stroke::new((1.75 * s).max(1.25), color),
        };

        match icon {
            Icon::Upload => {
                pen.line(&[(12.0, 15.0), (12.0, 4.0)]);
                pen.line(&[(7.0, 9.0), (12.0, 4.0), (17.0, 9.0)]);
                pen.line(&[(4.0, 14.0), (4.0, 20.0), (20.0, 20.0), (20.0, 14.0)]);
            }
            Icon::Layers => {
                pen.closed(&[(12.0, 3.0), (21.0, 8.0), (12.0, 13.0), (3.0, 8.0)]);
                pen.line(&[(3.0, 12.0), (12.0, 17.0), (21.0, 12.0)]);
                pen.line(&[(3.0, 16.0), (12.0, 21.0), (21.0, 16.0)]);
            }
            Icon::Repo => {
                pen.closed(&[(5.0, 3.0), (19.0, 3.0), (19.0, 21.0), (5.0, 21.0)]);
                pen.line(&[(9.0, 3.0), (9.0, 21.0)]);
                pen.line(&[(12.5, 8.0), (16.0, 8.0)]);
            }
            Icon::User => {
                pen.circle(12.0, 8.0, 4.0);
                pen.line(&arc(12.0, 21.0, 7.5, 7.0, PI, TAU));
            }
            Icon::Award => {
                pen.circle(12.0, 9.0, 6.0);
                pen.line(&[(8.5, 13.8), (7.0, 21.5), (12.0, 19.0), (17.0, 21.5), (15.5, 13.8)]);
            }
            Icon::Key => {
                pen.circle(7.5, 15.5, 4.5);
                pen.line(&[(10.7, 12.3), (20.0, 3.0)]);
                pen.line(&[(16.5, 6.5), (19.0, 9.0)]);
                pen.line(&[(14.0, 9.0), (16.0, 11.0)]);
            }
            Icon::Lock => {
                let mut shackle = vec![(7.5, 11.0)];
                shackle.extend(arc(12.0, 8.5, 4.5, 4.5, PI, TAU));
                shackle.push((16.5, 11.0));
                pen.line(&shackle);
                pen.closed(&[(5.0, 11.0), (19.0, 11.0), (19.0, 21.0), (5.0, 21.0)]);
                pen.line(&[(12.0, 15.0), (12.0, 17.0)]);
            }
            Icon::Globe => {
                pen.circle(12.0, 12.0, 9.0);
                pen.closed(&arc(12.0, 12.0, 4.0, 9.0, 0.0, TAU));
                pen.line(&[(3.0, 12.0), (21.0, 12.0)]);
            }
            Icon::Folder => {
                pen.closed(&[(3.0, 6.0), (9.5, 6.0), (11.5, 8.5), (21.0, 8.5), (21.0, 19.0), (3.0, 19.0)]);
            }
            Icon::Refresh => {
                let (a0, a1) = (-PI * 0.5 + 0.6, -PI * 0.5 + TAU - 0.35);
                let pts = arc(12.0, 12.0, 8.0, 8.0, a0, a1);
                let (ex, ey) = *pts.last().unwrap();
                pen.line(&pts);
                let (tx, ty) = (-a1.sin(), a1.cos());
                let head = |rot: f32| {
                    let (c, s) = (rot.cos(), rot.sin());
                    (ex + 4.5 * (-tx * c + ty * s), ey + 4.5 * (-tx * s - ty * c))
                };
                pen.line(&[head(0.65), (ex, ey), head(-0.65)]);
            }
            Icon::Star => {
                let pts: Vec<(f32, f32)> = (0..10)
                    .map(|i| {
                        let r = if i % 2 == 0 { 9.5 } else { 4.2 };
                        let a = -PI * 0.5 + i as f32 * PI / 5.0;
                        (12.0 + r * a.cos(), 12.8 + r * a.sin())
                    })
                    .collect();
                pen.closed(&pts);
            }
            Icon::External => {
                pen.line(&[(13.0, 4.0), (20.0, 4.0), (20.0, 11.0)]);
                pen.line(&[(20.0, 4.0), (11.0, 13.0)]);
                pen.line(&[(17.0, 14.0), (17.0, 20.0), (4.0, 20.0), (4.0, 7.0), (10.0, 7.0)]);
            }
            Icon::Logout => {
                pen.line(&[(10.0, 4.0), (5.0, 4.0), (5.0, 20.0), (10.0, 20.0)]);
                pen.line(&[(10.0, 12.0), (20.0, 12.0)]);
                pen.line(&[(16.0, 8.0), (20.0, 12.0), (16.0, 16.0)]);
            }
            Icon::Trash => {
                pen.line(&[(4.0, 7.0), (20.0, 7.0)]);
                pen.line(&[(9.0, 7.0), (9.0, 4.0), (15.0, 4.0), (15.0, 7.0)]);
                pen.line(&[(6.0, 7.0), (7.0, 20.0), (17.0, 20.0), (18.0, 7.0)]);
                pen.line(&[(10.0, 11.0), (10.0, 16.0)]);
                pen.line(&[(14.0, 11.0), (14.0, 16.0)]);
            }
            Icon::Check => {
                pen.circle(12.0, 12.0, 9.0);
                pen.line(&[(8.0, 12.5), (11.0, 15.5), (16.5, 9.5)]);
            }
            Icon::Alert => {
                pen.closed(&[(12.0, 3.5), (21.5, 20.0), (2.5, 20.0)]);
                pen.line(&[(12.0, 9.5), (12.0, 14.0)]);
                pen.dot(12.0, 17.0, 1.1);
            }
            Icon::Minimize => pen.line(&[(4.0, 12.0), (20.0, 12.0)]),
            Icon::Maximize => pen.closed(&[(4.0, 4.0), (20.0, 4.0), (20.0, 20.0), (4.0, 20.0)]),
            Icon::Restore => {
                pen.closed(&[(4.0, 8.0), (16.0, 8.0), (16.0, 20.0), (4.0, 20.0)]);
                pen.line(&[(8.0, 8.0), (8.0, 4.0), (20.0, 4.0), (20.0, 16.0), (16.0, 16.0)]);
            }
            Icon::Close => {
                pen.line(&[(4.0, 4.0), (20.0, 20.0)]);
                pen.line(&[(20.0, 4.0), (4.0, 20.0)]);
            }
        }
    }
}

use icons::Icon;

const LOGO_PNG: &[u8] = include_bytes!("../assets/logo.png");
const ICON_PNG: &[u8] = include_bytes!("../assets/icon_small.png");

fn decode_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

fn load_avatar(ctx: &egui::Context, url: &str) -> Option<egui::TextureHandle> {
    if url.is_empty() {
        return None;
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    let bytes = reqwest::blocking::Client::builder()
        .user_agent("Soar-GitHub-Uploader")
        .build()
        .ok()?
        .get(format!("{url}{sep}s=160"))
        .send()
        .ok()?
        .bytes()
        .ok()?;
    let image = decode_image(&bytes)?;
    Some(ctx.load_texture("avatar", image, egui::TextureOptions::LINEAR))
}

fn f_regular(size: f32) -> egui::FontId {
    egui::FontId::proportional(size)
}

fn f_semi(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("semibold".into()))
}

fn f_serif(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("serif".into()))
}

/// Segoe UI for text and Palatino for titles when the system has them, egui's bundled font otherwise.
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let fallback = fonts.families[&egui::FontFamily::Proportional].clone();
    let dir = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts");

    let mut family = |name: &str, file: &str| -> Vec<String> {
        let mut list = Vec::new();
        if let Ok(bytes) = fs::read(dir.join(file)) {
            fonts.font_data.insert(name.to_string(), egui::FontData::from_owned(bytes));
            list.push(name.to_string());
        }
        list.extend(fallback.iter().cloned());
        list
    };
    let regular = family("segoe", "segoeui.ttf");
    let semibold = family("segoe-semibold", "seguisb.ttf");
    let serif = family("palatino", "pala.ttf");

    fonts.families.insert(egui::FontFamily::Proportional, regular);
    fonts.families.insert(egui::FontFamily::Name("semibold".into()), semibold);
    fonts.families.insert(egui::FontFamily::Name("serif".into()), serif);
    ctx.set_fonts(fonts);
}

fn apply_style(ctx: &egui::Context) {
    use egui::{Rounding, Stroke, TextStyle};

    let mut v = egui::Visuals::dark();
    v.panel_fill = theme::BG;
    v.window_fill = theme::RAISED;
    v.window_stroke = Stroke::new(1.0_f32, theme::BORDER);
    v.extreme_bg_color = theme::INPUT;
    v.faint_bg_color = theme::CARD;
    v.code_bg_color = theme::INPUT;
    v.hyperlink_color = theme::INK;
    v.selection.bg_fill = theme::tint(theme::INK, 60);
    v.selection.stroke = Stroke::new(1.0_f32, theme::INK);
    v.text_cursor.stroke = Stroke::new(2.0_f32, theme::INK);

    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, theme::BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, theme::TEXT);
    v.widgets.inactive.bg_fill = theme::RAISED;
    v.widgets.inactive.weak_bg_fill = theme::RAISED;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, theme::BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, theme::MUTED);
    v.widgets.hovered.bg_fill = theme::RAISED;
    v.widgets.hovered.weak_bg_fill = theme::RAISED;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, theme::BORDER_STRONG);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, theme::TEXT);
    v.widgets.active.bg_fill = theme::RAISED;
    v.widgets.active.weak_bg_fill = theme::RAISED;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, theme::INK);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, theme::TEXT);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.rounding = Rounding::same(8.0);
    }

    let mut style = (*ctx.style()).clone();
    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.scroll = egui::style::ScrollStyle::floating();
    style.text_styles = [
        (TextStyle::Heading, f_serif(24.0)),
        (TextStyle::Body, f_regular(14.0)),
        (TextStyle::Button, f_semi(14.0)),
        (TextStyle::Small, f_regular(11.5)),
        (TextStyle::Monospace, egui::FontId::monospace(13.0)),
    ]
    .into();
    ctx.set_style(style);
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Primary,
    Ghost,
    Danger,
    DangerGhost,
}

/// `size.x <= 0` sizes the button to its content.
fn button(ui: &mut egui::Ui, icon: Option<Icon>, label: &str, size: egui::Vec2, kind: Kind) -> egui::Response {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), f_semi(13.5), egui::Color32::PLACEHOLDER);
    let icon_w = if icon.is_some() { 24.0 } else { 0.0 };
    let content_w = galley.size().x + icon_w;
    let width = if size.x > 0.0 { size.x } else { content_w + 32.0 };

    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, size.y), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if !ui.is_rect_visible(rect) {
        return resp;
    }

    let h = ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.12);
    let body = if resp.is_pointer_button_down_on() { rect.shrink(1.0) } else { rect };

    let (fill, stroke, fg) = match kind {
        Kind::Primary => (theme::mix(theme::INK, theme::INK_HI, h), egui::Stroke::NONE, theme::ON_INK),
        Kind::Ghost => (
            theme::tint(theme::INK, (h * 14.0) as u8),
            egui::Stroke::new(1.0_f32, theme::mix(theme::BORDER_STRONG, theme::INK, h * 0.7)),
            theme::mix(theme::TEXT, theme::INK_HI, h),
        ),
        Kind::Danger => (
            theme::mix(theme::DANGER, egui::Color32::from_rgb(245, 120, 114), h),
            egui::Stroke::NONE,
            theme::INK_HI,
        ),
        Kind::DangerGhost => (
            theme::tint(theme::DANGER, (h * 30.0) as u8),
            egui::Stroke::new(1.0_f32, theme::mix(theme::BORDER_STRONG, theme::DANGER, h)),
            theme::mix(theme::MUTED, theme::DANGER, h),
        ),
    };

    let painter = ui.painter();
    painter.rect(body, egui::Rounding::same(8.0), fill, stroke);
    let start_x = rect.center().x - content_w / 2.0;
    if let Some(icon) = icon {
        let r = egui::Rect::from_center_size(egui::pos2(start_x + 8.0, rect.center().y), egui::vec2(16.0, 16.0));
        icons::paint(painter, r, icon, fg);
    }
    let text_pos = egui::pos2(start_x + icon_w, rect.center().y - galley.size().y / 2.0);
    painter.galley(text_pos, galley, fg);
    resp
}

fn chip(ui: &mut egui::Ui, icon: Option<Icon>, label: &str, selected: bool) -> egui::Response {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), f_semi(13.0), egui::Color32::PLACEHOLDER);
    let icon_w = if icon.is_some() { 22.0 } else { 0.0 };
    let size = egui::vec2(galley.size().x + icon_w + 30.0, 36.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);

    let h = ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.12);
    let s = ui.ctx().animate_bool_with_time(resp.id.with("sel"), selected, 0.15);

    let fill = theme::mix(theme::INPUT, theme::INK, s);
    let border = theme::mix(theme::mix(theme::BORDER, theme::BORDER_STRONG, h), theme::INK, s);
    let fg = theme::mix(theme::mix(theme::MUTED, theme::TEXT, h), theme::ON_INK, s);

    let painter = ui.painter();
    painter.rect(rect, egui::Rounding::same(18.0), fill, egui::Stroke::new(1.0_f32, border));
    let mut x = rect.left() + 15.0;
    if let Some(icon) = icon {
        let r = egui::Rect::from_center_size(egui::pos2(x + 7.0, rect.center().y), egui::vec2(15.0, 15.0));
        icons::paint(painter, r, icon, fg);
        x += icon_w;
    }
    painter.galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    resp
}

fn pill(ui: &mut egui::Ui, icon: Option<Icon>, text: &str, color: egui::Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), f_semi(11.5), egui::Color32::PLACEHOLDER);
    let icon_w = if icon.is_some() { 16.0 } else { 0.0 };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(galley.size().x + icon_w + 18.0, 22.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect(rect, egui::Rounding::same(11.0), egui::Color32::TRANSPARENT, egui::Stroke::new(1.0_f32, theme::BORDER_STRONG));
    let mut x = rect.left() + 9.0;
    if let Some(icon) = icon {
        let r = egui::Rect::from_center_size(egui::pos2(x + 6.0, rect.center().y), egui::vec2(12.0, 12.0));
        icons::paint(painter, r, icon, color);
        x += icon_w;
    }
    painter.galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, color);
}

fn icon_box(ui: &mut egui::Ui, icon: Icon, color: egui::Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter()
        .rect(rect, egui::Rounding::same(size * 0.26), theme::RAISED, egui::Stroke::new(1.0_f32, theme::BORDER_STRONG));
    icons::paint(ui.painter(), rect.shrink(size * 0.25), icon, color);
}

/// Shows the GitHub avatar when it loaded, the first letter of the login otherwise.
fn avatar(ui: &mut egui::Ui, name: &str, image: Option<&egui::TextureHandle>, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    match image {
        Some(tex) => {
            egui::Image::new((tex.id(), rect.size()))
                .rounding(size / 2.0)
                .paint_at(ui, rect);
        }
        None => {
            let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            ui.painter().circle_filled(rect.center(), size / 2.0, theme::RAISED);
            ui.painter()
                .text(rect.center(), egui::Align2::CENTER_CENTER, initial, f_serif(size * 0.5), theme::TEXT);
        }
    }
    ui.painter()
        .circle_stroke(rect.center(), size / 2.0 + 2.0, egui::Stroke::new(1.0_f32, theme::BORDER_STRONG));
}

fn logo(ui: &mut egui::Ui, tex: &egui::TextureHandle, height: f32) {
    let [w, h] = tex.size();
    let size = egui::vec2(height * w as f32 / h as f32, height);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    egui::Image::new((tex.id(), size)).paint_at(ui, rect);
}

fn is_maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false))
}

/// The window has no OS decorations, so its edges start the resize themselves.
fn handle_resize(ctx: &egui::Context) {
    use egui::viewport::ResizeDirection as Dir;
    use egui::CursorIcon as Cursor;

    if is_maximized(ctx) {
        return;
    }
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    let rect = ctx.screen_rect();
    let edge = 5.0;
    let left = pos.x - rect.left() < edge;
    let right = rect.right() - pos.x < edge;
    let top = pos.y - rect.top() < edge;
    let bottom = rect.bottom() - pos.y < edge;

    let (dir, cursor) = match (left, right, top, bottom) {
        (true, _, true, _) => (Dir::NorthWest, Cursor::ResizeNorthWest),
        (_, true, true, _) => (Dir::NorthEast, Cursor::ResizeNorthEast),
        (true, _, _, true) => (Dir::SouthWest, Cursor::ResizeSouthWest),
        (_, true, _, true) => (Dir::SouthEast, Cursor::ResizeSouthEast),
        (true, ..) => (Dir::West, Cursor::ResizeWest),
        (_, true, ..) => (Dir::East, Cursor::ResizeEast),
        (_, _, true, _) => (Dir::North, Cursor::ResizeNorth),
        (_, _, _, true) => (Dir::South, Cursor::ResizeSouth),
        _ => return,
    };
    ctx.set_cursor_icon(cursor);
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
    }
}

fn title_button(ui: &mut egui::Ui, rect: egui::Rect, icon: Icon, close: bool) -> bool {
    let resp = ui.interact(rect, ui.id().with(rect.left() as i32), egui::Sense::click());
    let hover = if close { theme::DANGER } else { theme::tint(theme::INK, 18) };
    if resp.hovered() {
        ui.painter().rect_filled(rect, 0.0, hover);
    }
    let color = if resp.hovered() { theme::INK_HI } else { theme::MUTED };
    icons::paint(ui.painter(), egui::Rect::from_center_size(rect.center(), egui::vec2(11.0, 11.0)), icon, color);
    resp.clicked()
}

fn title_bar(ctx: &egui::Context, logo_tex: &egui::TextureHandle) {
    egui::TopBottomPanel::top("title_bar")
        .exact_height(38.0)
        .show_separator_line(false)
        .frame(egui::Frame::none().fill(theme::SIDEBAR))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            let maximized = is_maximized(ctx);

            // Registered before the buttons so the buttons, drawn on top, win the clicks.
            let drag = ui.interact(rect, ui.id().with("drag"), egui::Sense::click_and_drag());
            if drag.double_clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            } else if drag.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            let painter = ui.painter();
            painter.rect_filled(
                egui::Rect::from_min_max(rect.left_bottom() - egui::vec2(0.0, 1.0), rect.right_bottom()),
                0.0,
                theme::BORDER,
            );

            let [w, h] = logo_tex.size();
            let mark = egui::vec2(16.0 * w as f32 / h as f32, 16.0);
            let mark_rect = egui::Rect::from_center_size(egui::pos2(rect.left() + 20.0, rect.center().y), mark);
            egui::Image::new((logo_tex.id(), mark)).paint_at(ui, mark_rect);

            let title = painter.layout_no_wrap("Soar GitHub Uploader".into(), f_semi(12.5), theme::TEXT);
            let title_pos = egui::pos2(mark_rect.right() + 12.0, rect.center().y - title.size().y / 2.0);
            let byline_x = title_pos.x + title.size().x + 8.0;
            painter.galley(title_pos, title, theme::TEXT);
            painter.text(
                egui::pos2(byline_x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "by i2tt & K2CV",
                f_regular(12.0),
                theme::FAINT,
            );

            let size = egui::vec2(46.0, rect.height());
            let slot = |i: f32| egui::Rect::from_min_size(egui::pos2(rect.right() - size.x * i, rect.top()), size);
            if title_button(ui, slot(1.0), Icon::Close, true) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            let max_icon = if maximized { Icon::Restore } else { Icon::Maximize };
            if title_button(ui, slot(2.0), max_icon, false) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            if title_button(ui, slot(3.0), Icon::Minimize, false) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        });
}

fn window_border(ctx: &egui::Context) {
    if is_maximized(ctx) {
        return;
    }
    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("window_border"));
    ctx.layer_painter(layer)
        .rect_stroke(ctx.screen_rect().shrink(0.5), 0.0, egui::Stroke::new(1.0_f32, theme::BORDER_STRONG));
}

fn wordmark(ui: &mut egui::Ui, size: f32, spacing: f32) {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        "SOAR",
        0.0,
        egui::TextFormat {
            font_id: f_serif(size),
            color: theme::TEXT,
            extra_letter_spacing: spacing,
            ..Default::default()
        },
    );
    ui.label(job);
}

fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .fill(theme::CARD)
        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::same(22.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

fn card_header(ui: &mut egui::Ui, icon: Icon, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        icon_box(ui, icon, theme::TEXT, 36.0);
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(egui::RichText::new(title).font(f_semi(15.5)).color(theme::TEXT));
            ui.label(egui::RichText::new(subtitle).font(f_regular(12.5)).color(theme::MUTED));
        });
    });
    ui.add_space(14.0);
}

fn hairline(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::BORDER);
}

fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(egui::RichText::new(title).font(f_serif(34.0)).color(theme::TEXT));
    ui.add_space(-4.0);
    ui.label(egui::RichText::new(subtitle).font(f_regular(13.5)).color(theme::MUTED));
}

fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).font(f_semi(11.0)).color(theme::FAINT));
}

fn text_field(ui: &mut egui::Ui, value: &mut String, hint: &str, width: f32) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(egui::RichText::new(hint).color(theme::FAINT))
            .desired_width(width)
            .margin(egui::Margin::symmetric(12.0, 9.0))
            .font(f_regular(14.0)),
    )
}

fn status_banner(ui: &mut egui::Ui, message: &str, is_error: bool) {
    let color = if is_error { theme::DANGER } else { theme::INK };
    egui::Frame::none()
        .fill(theme::tint(color, 14))
        .stroke(egui::Stroke::new(1.0_f32, theme::tint(color, 80)))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(14.0, 11.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                icons::paint(ui.painter(), r, if is_error { Icon::Alert } else { Icon::Check }, color);
                ui.add(egui::Label::new(egui::RichText::new(message).font(f_regular(13.5)).color(theme::TEXT)).wrap());
            });
        });
}

fn nav_item(ui: &mut egui::Ui, icon: Icon, label: &str, active: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 42.0), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let h = ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.12);
    let a = ui.ctx().animate_bool_with_time(resp.id.with("active"), active, 0.18);

    let painter = ui.painter();
    let fill = theme::mix(theme::tint(theme::INK, (h * 8.0) as u8), theme::RAISED, a);
    painter.rect_filled(rect, egui::Rounding::same(8.0), fill);
    if a > 0.01 {
        let bar = egui::Rect::from_center_size(egui::pos2(rect.left() + 1.5, rect.center().y), egui::vec2(3.0, 20.0 * a));
        painter.rect_filled(bar, egui::Rounding::same(2.0), theme::INK);
    }

    let color = theme::mix(theme::mix(theme::MUTED, theme::TEXT, h), theme::INK_HI, a);
    let icon_rect = egui::Rect::from_center_size(egui::pos2(rect.left() + 22.0, rect.center().y), egui::vec2(18.0, 18.0));
    icons::paint(painter, icon_rect, icon, color);
    painter.text(egui::pos2(rect.left() + 44.0, rect.center().y), egui::Align2::LEFT_CENTER, label, f_semi(14.0), color);
    resp.clicked()
}

#[derive(PartialEq, Clone, Copy)]
enum ActiveTab {
    Publish,
    Manage,
    Profile,
    Credits,
}

const DEVELOPERS: [&str; 2] = ["i2tt", "K2CV"];

const LICENSES: [(&str, &str); 5] = [
    ("None", ""),
    ("MIT", "mit"),
    ("Apache-2.0", "apache-2.0"),
    ("GPL-3.0", "gpl-3.0"),
    ("Unlicense", "unlicense"),
];

struct SoarApp {
    token_input: String,
    authenticated: bool,
    user: GitHubUser,
    avatar: Option<egui::TextureHandle>,
    logo: egui::TextureHandle,
    /// Filled the first time Credits opens, in `DEVELOPERS` order.
    dev_avatars: Vec<Option<egui::TextureHandle>>,
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

impl SoarApp {
    fn new(ctx: &egui::Context) -> Self {
        let logo_image = decode_image(LOGO_PNG).expect("bundled logo is a valid PNG");
        let mut app = Self {
            token_input: String::new(),
            authenticated: false,
            user: GitHubUser::default(),
            avatar: None,
            logo: ctx.load_texture("logo", logo_image, egui::TextureOptions::LINEAR),
            dev_avatars: Vec::new(),
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
                app.sign_in(ctx, user);
            }
        }

        app
    }

    fn sign_in(&mut self, ctx: &egui::Context, user: GitHubUser) {
        self.avatar = load_avatar(ctx, &user.avatar_url);
        self.user = user;
        self.authenticated = true;
    }

    fn set_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status_message = message.into();
        self.is_error = is_error;
    }

    fn authenticate(&mut self, ctx: &egui::Context) {
        let token = self.token_input.trim().to_string();
        if token.is_empty() {
            self.set_status("Enter a token first.", true);
            return;
        }
        match api_fetch_user(&token) {
            Ok(user) => {
                self.sign_in(ctx, user);
                save_token_to_file(&token);
                let msg = format!("Signed in as {}", self.user.login);
                self.set_status(msg, false);
            }
            Err(e) => self.set_status(e, true),
        }
    }

    fn logout(&mut self) {
        self.authenticated = false;
        self.avatar = None;
        self.token_input.clear();
        self.status_message.clear();
        let _ = fs::remove_file(CONFIG_FILE);
    }

    fn refresh_repos(&mut self) {
        match api_fetch_repos(&self.token_input) {
            Ok(repos) => self.my_repos = repos,
            Err(e) => self.set_status(e, true),
        }
    }

    fn publish(&mut self) {
        let name = self.repo_name.trim().to_string();
        if name.is_empty() {
            self.set_status("Enter a repository name.", true);
            return;
        }
        let folder = PathBuf::from(&self.folder_path);
        if !folder.is_dir() {
            self.set_status(format!("Folder does not exist: {}", folder.display()), true);
            return;
        }

        let lic_key = LICENSES[self.license_choice].1;
        if !lic_key.is_empty() {
            let lic_file = folder.join("LICENSE");
            if !lic_file.exists() {
                if let Some(body) = api_fetch_license(lic_key) {
                    let _ = fs::write(&lic_file, body);
                }
            }
        }

        match api_create_repo(&self.token_input, &name, &self.repo_desc, self.repo_private) {
            Ok(repo) => match git_push_folder(&folder, &self.token_input, &repo.clone_url, &self.user.login) {
                Ok(_) => self.set_status(format!("Published to {}", repo.html_url), false),
                Err(e) => self.set_status(format!("Repo created ({}), but push failed: {}", repo.html_url, e), true),
            },
            Err(e) => self.set_status(e, true),
        }
    }

    fn delete_repo(&mut self, repo_name: &str) {
        match api_delete_repo(&self.token_input, &self.user.login, repo_name) {
            Ok(_) => {
                self.set_status(format!("Deleted '{}'.", repo_name), false);
                self.my_repos.retain(|r| r.name != repo_name);
            }
            Err(err) => self.set_status(err, true),
        }
        self.confirm_delete = None;
    }

    fn show_login(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::BG))
            .show(ctx, |ui| {
                let height = ui.max_rect().height();
                ui.add_space(((height - 580.0) / 2.0).max(24.0));
                ui.vertical_centered(|ui| {
                    logo(ui, &self.logo, 92.0);
                    ui.add_space(14.0);
                    wordmark(ui, 40.0, 12.0);
                    ui.add_space(-6.0);
                    ui.label(egui::RichText::new("GITHUB UPLOADER").font(f_semi(11.5)).color(theme::FAINT));
                    ui.add_space(26.0);

                    egui::Frame::none()
                        .fill(theme::CARD)
                        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                        .rounding(egui::Rounding::same(14.0))
                        .inner_margin(egui::Margin::same(26.0))
                        .show(ui, |ui| {
                            ui.set_width(360.0);
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.label(egui::RichText::new("Sign in").font(f_serif(24.0)).color(theme::TEXT));
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("Use a personal access token with the repo and delete_repo scopes.")
                                            .font(f_regular(13.0))
                                            .color(theme::MUTED),
                                    )
                                    .wrap(),
                                );
                                ui.add_space(12.0);
                                field_label(ui, "ACCESS TOKEN");
                                let resp = ui.add(
                                    egui::TextEdit::singleline(&mut self.token_input)
                                        .password(true)
                                        .hint_text(egui::RichText::new("ghp_xxxxxxxxxxxxxxxxxxxx").color(theme::FAINT))
                                        .desired_width(f32::INFINITY)
                                        .margin(egui::Margin::symmetric(12.0, 10.0))
                                        .font(f_regular(14.0)),
                                );
                                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                ui.add_space(10.0);
                                let width = ui.available_width();
                                let clicked = button(ui, Some(Icon::Key), "Sign in to GitHub", egui::vec2(width, 42.0), Kind::Primary)
                                    .clicked();
                                if clicked || enter {
                                    self.authenticate(ctx);
                                }
                                if !self.status_message.is_empty() {
                                    ui.add_space(6.0);
                                    status_banner(ui, &self.status_message, self.is_error);
                                }
                            });
                        });

                    ui.add_space(18.0);
                    ui.label(egui::RichText::new("Made by i2tt & K2CV  ·  Soar Team").font(f_regular(12.0)).color(theme::FAINT));
                });
            });
    }

    fn show_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("nav")
            .resizable(false)
            .exact_width(244.0)
            .frame(
                egui::Frame::none()
                    .fill(theme::SIDEBAR)
                    .inner_margin(egui::Margin { left: 16.0, right: 16.0, top: 22.0, bottom: 18.0 }),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    logo(ui, &self.logo, 40.0);
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        wordmark(ui, 22.0, 5.0);
                        ui.label(egui::RichText::new("GitHub Uploader").font(f_regular(12.0)).color(theme::MUTED));
                    });
                });

                ui.add_space(30.0);
                ui.label(egui::RichText::new("MENU").font(f_semi(11.0)).color(theme::FAINT));
                ui.add_space(2.0);
                ui.spacing_mut().item_spacing.y = 4.0;

                let items = [
                    (ActiveTab::Publish, Icon::Upload, "Publish"),
                    (ActiveTab::Manage, Icon::Layers, "Repositories"),
                    (ActiveTab::Profile, Icon::User, "Account"),
                    (ActiveTab::Credits, Icon::Award, "Credits"),
                ];
                for (tab, icon, label) in items {
                    if nav_item(ui, icon, label, self.active_tab == tab) {
                        self.active_tab = tab;
                        if tab == ActiveTab::Manage {
                            self.refresh_repos();
                        }
                        if tab == ActiveTab::Credits && self.dev_avatars.is_empty() {
                            self.dev_avatars = DEVELOPERS
                                .iter()
                                .map(|dev| load_avatar(ctx, &format!("https://github.com/{dev}.png")))
                                .collect();
                        }
                    }
                }

                // Pinned to the bottom by hand: Frame lays out wrong inside a bottom_up layout.
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.add_space((ui.available_height() - 116.0).max(12.0));
                egui::Frame::none()
                    .fill(theme::CARD)
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                    .rounding(egui::Rounding::same(12.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.add_space(2.0);
                            avatar(ui, &self.user.login, self.avatar.as_ref(), 38.0);
                            ui.add_space(4.0);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.label(egui::RichText::new(&self.user.login).font(f_semi(14.0)).color(theme::TEXT));
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 5.0;
                                    let (r, _) = ui.allocate_exact_size(egui::vec2(7.0, 14.0), egui::Sense::hover());
                                    ui.painter().circle_filled(r.center(), 3.0, theme::INK);
                                    ui.label(egui::RichText::new("Connected").font(f_regular(11.5)).color(theme::MUTED));
                                });
                            });
                        });
                    });
                let width = ui.available_width();
                if button(ui, Some(Icon::Logout), "Sign out", egui::vec2(width, 36.0), Kind::Ghost).clicked() {
                    self.logout();
                }
            });
    }

    fn page_publish(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Publish", "Create a GitHub repository and push a local folder to it.");
        ui.add_space(14.0);
        hairline(ui);
        ui.add_space(18.0);

        card(ui, |ui| {
            card_header(ui, Icon::Repo, "Repository", "Name, description and visibility");

            field_label(ui, "NAME");
            text_field(ui, &mut self.repo_name, "e.g. My-Awesome-Project", f32::INFINITY);
            ui.add_space(6.0);

            field_label(ui, "DESCRIPTION");
            text_field(ui, &mut self.repo_desc, "Optional", f32::INFINITY);
            ui.add_space(6.0);

            field_label(ui, "VISIBILITY");
            ui.horizontal(|ui| {
                if chip(ui, Some(Icon::Globe), "Public", !self.repo_private).clicked() {
                    self.repo_private = false;
                }
                if chip(ui, Some(Icon::Lock), "Private", self.repo_private).clicked() {
                    self.repo_private = true;
                }
            });
            ui.add_space(6.0);

            field_label(ui, "LICENSE");
            ui.horizontal_wrapped(|ui| {
                for (i, (label, _)) in LICENSES.iter().enumerate() {
                    if chip(ui, None, label, self.license_choice == i).clicked() {
                        self.license_choice = i;
                    }
                }
            });
        });

        ui.add_space(14.0);

        card(ui, |ui| {
            card_header(ui, Icon::Folder, "Source folder", "Its contents are committed and pushed to main");
            field_label(ui, "PATH");
            ui.horizontal(|ui| {
                // desired_width excludes the 12px inner margin on each side.
                let w = ui.available_width() - 140.0 - 24.0;
                text_field(ui, &mut self.folder_path, ". for the current directory", w);
                if button(ui, Some(Icon::Folder), "Use current", egui::vec2(132.0, 38.0), Kind::Ghost).clicked() {
                    self.folder_path = ".".to_string();
                }
            });
        });

        ui.add_space(20.0);
        ui.horizontal(|ui| {
            if button(ui, Some(Icon::Upload), "Create & push", egui::vec2(200.0, 46.0), Kind::Primary).clicked() {
                self.publish();
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!("github.com/{}", self.user.login))
                    .font(f_regular(12.5))
                    .color(theme::FAINT),
            );
        });

        if !self.status_message.is_empty() {
            ui.add_space(14.0);
            status_banner(ui, &self.status_message, self.is_error);
        }
    }

    fn page_manage(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                page_header(ui, "Repositories", "Everything on your account, most recently updated first.");
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.add_space(2.0);
                if button(ui, Some(Icon::Refresh), "Refresh", egui::vec2(0.0, 38.0), Kind::Ghost).clicked() {
                    self.refresh_repos();
                }
            });
        });
        ui.add_space(14.0);
        hairline(ui);
        ui.add_space(18.0);

        if let Some(name) = self.confirm_delete.clone() {
            egui::Frame::none()
                .fill(theme::tint(theme::DANGER, 12))
                .stroke(egui::Stroke::new(1.0_f32, theme::tint(theme::DANGER, 110)))
                .rounding(egui::Rounding::same(12.0))
                .inner_margin(egui::Margin::same(20.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        icon_box(ui, Icon::Alert, theme::DANGER, 38.0);
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Delete '{}' permanently?", name))
                                    .font(f_semi(15.5))
                                    .color(theme::TEXT),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new("GitHub removes the repository and its whole history. This can't be undone.")
                                        .font(f_regular(13.0))
                                        .color(theme::MUTED),
                                )
                                .wrap(),
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if button(ui, Some(Icon::Trash), "Delete repository", egui::vec2(0.0, 36.0), Kind::Danger).clicked() {
                                    self.delete_repo(&name);
                                }
                                if button(ui, None, "Cancel", egui::vec2(0.0, 36.0), Kind::Ghost).clicked() {
                                    self.confirm_delete = None;
                                }
                            });
                        });
                    });
                });
            ui.add_space(14.0);
        }

        if !self.status_message.is_empty() {
            status_banner(ui, &self.status_message, self.is_error);
            ui.add_space(14.0);
        }

        if self.my_repos.is_empty() {
            card(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(18.0);
                    icon_box(ui, Icon::Layers, theme::MUTED, 52.0);
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("No repositories loaded").font(f_semi(15.5)).color(theme::TEXT));
                    ui.label(egui::RichText::new("Press Refresh to fetch them from GitHub.").font(f_regular(13.0)).color(theme::MUTED));
                    ui.add_space(18.0);
                });
            });
            return;
        }

        ui.label(
            egui::RichText::new(format!("{} REPOSITORIES", self.my_repos.len()))
                .font(f_semi(11.0))
                .color(theme::FAINT),
        );
        ui.add_space(2.0);

        let mut to_delete = None;
        for repo in &self.my_repos {
            egui::Frame::none()
                .fill(theme::CARD)
                .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                .rounding(egui::Rounding::same(10.0))
                .inner_margin(egui::Margin::symmetric(18.0, 14.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if button(ui, Some(Icon::Trash), "Delete", egui::vec2(0.0, 32.0), Kind::DangerGhost).clicked() {
                            to_delete = Some(repo.name.clone());
                        }
                        if button(ui, Some(Icon::External), "Open", egui::vec2(0.0, 32.0), Kind::Ghost).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&repo.html_url));
                        }
                        ui.add_space(10.0);
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 22.0), egui::Sense::hover());
                                icons::paint(ui.painter(), r, Icon::Repo, theme::MUTED);
                                ui.label(egui::RichText::new(&repo.name).font(f_semi(15.0)).color(theme::TEXT));
                                if repo.private {
                                    pill(ui, Some(Icon::Lock), "Private", theme::TEXT);
                                } else {
                                    pill(ui, Some(Icon::Globe), "Public", theme::MUTED);
                                }
                                pill(ui, Some(Icon::Star), &repo.stargazers_count.to_string(), theme::MUTED);
                            });
                            if let Some(desc) = repo.description.as_deref().filter(|d| !d.trim().is_empty()) {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(desc).font(f_regular(12.5)).color(theme::MUTED)).wrap(),
                                );
                            }
                        });
                    });
                });
            ui.add_space(2.0);
        }
        if to_delete.is_some() {
            self.confirm_delete = to_delete;
        }
    }

    fn page_profile(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Account", "The GitHub account this app is signed in to.");
        ui.add_space(14.0);
        hairline(ui);
        ui.add_space(18.0);

        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(2.0);
                avatar(ui, &self.user.login, self.avatar.as_ref(), 84.0);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let display_name = self.user.name.clone().unwrap_or_else(|| self.user.login.clone());
                    ui.label(egui::RichText::new(display_name).font(f_serif(30.0)).color(theme::TEXT));
                    ui.label(egui::RichText::new(format!("@{}", self.user.login)).font(f_regular(13.5)).color(theme::MUTED));
                    ui.add_space(6.0);
                    if button(ui, Some(Icon::External), "Open profile", egui::vec2(0.0, 34.0), Kind::Ghost).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(&self.user.html_url));
                    }
                });
            });
        });

        ui.add_space(14.0);
        let stats = [
            (Icon::Globe, "PUBLIC REPOS", self.user.public_repos.to_string()),
            (Icon::Layers, "LOADED", self.my_repos.len().to_string()),
            (Icon::Key, "TOKEN", "Saved".to_string()),
        ];
        ui.columns(3, |cols| {
            for (col, (icon, label, value)) in cols.iter_mut().zip(stats) {
                card(col, |ui| {
                    icon_box(ui, icon, theme::TEXT, 34.0);
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(label).font(f_semi(11.0)).color(theme::FAINT));
                    ui.label(egui::RichText::new(value).font(f_serif(32.0)).color(theme::TEXT));
                });
            }
        });
    }

    fn page_credits(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Credits", "Built by the Soar Team.");
        ui.add_space(14.0);
        hairline(ui);
        ui.add_space(18.0);

        card(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);
                logo(ui, &self.logo, 84.0);
                ui.add_space(12.0);
                wordmark(ui, 34.0, 10.0);
                ui.add_space(-4.0);
                ui.label(
                    egui::RichText::new(format!("GitHub Uploader  ·  v{}", env!("CARGO_PKG_VERSION")))
                        .font(f_regular(13.0))
                        .color(theme::MUTED),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("A Rust desktop app for publishing and managing GitHub repositories.")
                        .font(f_regular(13.5))
                        .color(theme::TEXT),
                );
                ui.add_space(10.0);
                if button(ui, Some(Icon::External), "Source on GitHub", egui::vec2(0.0, 36.0), Kind::Ghost).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab("https://github.com/i2tt/Soar-GitHub-Uploader"));
                }
                ui.add_space(10.0);
            });
        });

        ui.add_space(14.0);
        ui.label(egui::RichText::new("DEVELOPERS").font(f_semi(11.0)).color(theme::FAINT));
        ui.add_space(2.0);
        ui.columns(2, |cols| {
            for (i, (col, name)) in cols.iter_mut().zip(DEVELOPERS).enumerate() {
                let url = format!("https://github.com/{name}");
                card(col, |ui| {
                    ui.horizontal(|ui| {
                        avatar(ui, name, self.dev_avatars.get(i).and_then(Option::as_ref), 46.0);
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.label(egui::RichText::new(name).font(f_semi(15.5)).color(theme::TEXT));
                            ui.label(egui::RichText::new("Soar Team").font(f_regular(12.5)).color(theme::MUTED));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if button(ui, Some(Icon::External), "GitHub", egui::vec2(0.0, 32.0), Kind::Ghost).clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(&url));
                            }
                        });
                    });
                });
            }
        });
    }
}

impl eframe::App for SoarApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        handle_resize(ctx);
        title_bar(ctx, &self.logo);
        if self.authenticated {
            self.show_main(ctx);
        } else {
            self.show_login(ctx);
        }
        window_border(ctx);
    }
}

impl SoarApp {
    fn show_main(&mut self, ctx: &egui::Context) {
        self.show_sidebar(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::BG).inner_margin(egui::Margin::symmetric(40.0, 30.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let avail = ui.available_width();
                    let width = avail.min(780.0);
                    let pad = ((avail - width) / 2.0).max(0.0);
                    ui.horizontal_top(|ui| {
                        ui.add_space(pad);
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            match self.active_tab {
                                ActiveTab::Publish => self.page_publish(ui),
                                ActiveTab::Manage => self.page_manage(ui),
                                ActiveTab::Profile => self.page_profile(ui),
                                ActiveTab::Credits => self.page_credits(ui),
                            }
                            ui.add_space(24.0);
                        });
                    });
                });
            });
    }
}

fn main() -> eframe::Result<()> {
    let icon = image::load_from_memory(ICON_PNG).expect("bundled icon is a valid PNG").to_rgba8();
    let icon = egui::IconData {
        width: icon.width(),
        height: icon.height(),
        rgba: icon.into_raw(),
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1060.0, 720.0])
            .with_min_inner_size([860.0, 580.0])
            .with_title("Soar GitHub Uploader - Made By i2tt & K2CV")
            .with_decorations(false)
            .with_icon(std::sync::Arc::new(icon)),
        ..Default::default()
    };

    eframe::run_native(
        "Soar GitHub Uploader",
        native_options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            apply_style(&cc.egui_ctx);
            Ok(Box::new(SoarApp::new(&cc.egui_ctx)))
        }),
    )
}
