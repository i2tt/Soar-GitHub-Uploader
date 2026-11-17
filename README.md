# Soar Team - GitHub Uploader

<p align="center">
  <img src="https://img.shields.io/badge/Rust-GUI_App-DEA584?style=for-the-badge&logo=rust&logoColor=white" alt="Rust GUI" />
  <img src="https://img.shields.io/badge/GitHub-REST_API-181717?style=for-the-badge&logo=github&logoColor=white" alt="GitHub API" />
  <img src="https://img.shields.io/badge/Windows-Supported-0078D6?style=for-the-badge&logo=windows&logoColor=white" alt="Windows" />
  <img src="https://img.shields.io/badge/Linux-Supported-FCC624?style=for-the-badge&logo=linux&logoColor=black" alt="Linux" />
  <img src="https://img.shields.io/badge/macOS-Supported-000000?style=for-the-badge&logo=apple&logoColor=white" alt="macOS" />
</p>

A fast, lightweight, and modern desktop GUI application written in Rust to create, publish, and delete GitHub repositories directly through a visual interface.

---

## Features

- Modern visual desktop GUI interface with dark theme and custom styling (no CMD needed)
- Automatic authentication with GitHub Personal Access Token (saved locally in `config.json`)
- Create public or private repositories via the GitHub REST API
- One-click automatic license generation (MIT, Apache-2.0, GPL-3.0, Unlicense)
- Automated local Git initialization, commit, and push with automatic author configuration
- Repository management tab: view all your repositories, star counts, and delete unwanted repos with confirmation
- Automatic security cleanup: credentials are never stored in plain text inside git configurations

---

## Prerequisites

- Windows 10/11, Linux, or macOS
- Git CLI installed and available in PATH
- GitHub Personal Access Token with `repo` and `delete_repo` scopes

---

## Installation

Clone the repository and build:

```bash
git clone https://github.com/i2tt/Soar-GitHub-Uploader.git
cd Soar-GitHub-Uploader
cargo build --release
```

---

## Usage

Double-click `run.bat` or run directly:

```bash
run.bat
```

Or run via Cargo:

```bash
cargo run --release
```

### Application Tabs

- `⚡ Publish Repository`: Create new repo, select license, specify folder path, and push
- `🗑 Manage & Delete Repos`: List all your repositories with star counts and delete any repo with one click
- `👤 Account Profile`: View authenticated account stats and details
- `📜 Credits`: Developer credits and project links

---

## Developers

<p align="center">
  <a href="https://github.com/i2tt">
    <img src="https://img.shields.io/badge/i2tt-GitHub-181717?style=for-the-badge&logo=github&logoColor=white" alt="i2tt" />
  </a>
  <a href="https://github.com/K2CV">
    <img src="https://img.shields.io/badge/K2CV-GitHub-181717?style=for-the-badge&logo=github&logoColor=white" alt="K2CV" />
  </a>
</p>

Co-Authored-By: [K2CV](https://github.com/K2CV)

---

## Disclaimer

This software is for developer productivity and repository management.
Use entirely at your own risk.
