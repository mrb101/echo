<p align="center">
  <img src="data/resources/icons/scalable/apps/com.echo.Echo.svg" width="128" height="128" alt="Echo">
</p>

<h1 align="center">Echo</h1>

<p align="center">
  A native Linux desktop AI chat application for GNOME
</p>

<p align="center">
  <a href="#features">Features</a> &bull;
  <a href="#supported-providers">Providers</a> &bull;
  <a href="#installation">Installation</a> &bull;
  <a href="#building-from-source">Building</a> &bull;
  <a href="#usage">Usage</a> &bull;
  <a href="#keyboard-shortcuts">Shortcuts</a> &bull;
  <a href="#roadmap">Roadmap</a> &bull;
  <a href="#contributing">Contributing</a> &bull;
  <a href="#license">License</a>
</p>

---

## About

Echo is a native GNOME desktop application for chatting with AI models. Built with Rust, GTK4, and libadwaita, it provides a fast, privacy-respecting interface that integrates seamlessly with the GNOME desktop environment. API keys are stored securely in your system keyring — never in plaintext.

## Features

- **Multi-provider support** — Chat with Google Gemini, Anthropic Claude, and local models from a single app
- **Local model support** — Connect to Ollama, Docker Model Runner, LM Studio, vLLM, or any OpenAI-compatible API
- **Streaming responses** — Real-time token streaming with cancel support
- **Markdown rendering** — Native GTK rendering of markdown with fenced code blocks
- **Image attachments** — Attach images to your messages for multimodal conversations
- **System prompts** — Set global defaults or per-conversation system prompts
- **Conversation management** — Pin, rename, search, export, and organize your conversations
- **Message actions** — Copy, regenerate, and edit messages
- **Conversation export** — Export conversations to Markdown
- **Secure key storage** — API keys stored in your system keyring via libsecret
- **Adaptive UI** — Responsive layout that adapts to different window sizes
- **GNOME integration** — Follows your system theme, supports dark mode, uses native GTK4 widgets

## Supported Providers

| Provider | Streaming | Images | Notes |
|----------|-----------|--------|-------|
| **Google Gemini** | Yes | Yes | Requires API key |
| **Anthropic Claude** | Yes | Yes | Requires API key |
| **Local (OpenAI-compatible)** | Yes | No | Ollama, Docker Model Runner, LM Studio, vLLM, etc. |

## Installation

### Building from Source

#### Prerequisites

- **Rust** (1.75 or later) — [rustup.rs](https://rustup.rs)
- **GTK4** (4.14+) and **libadwaita** (1.5+) development libraries
- **SQLite3** (bundled via rusqlite, no system dependency needed)
- **libsecret** development libraries (for keyring support)

On Fedora:
```bash
sudo dnf install gtk4-devel libadwaita-devel libsecret-devel
```

On Ubuntu/Debian:
```bash
sudo apt install libgtk-4-dev libadwaita-1-dev libsecret-1-dev
```

On Arch Linux:
```bash
sudo pacman -S gtk4 libadwaita libsecret
```

#### Build and Run

```bash
git clone https://github.com/mrb101/echo.git
cd echo
cargo build --release
```

The compiled binary will be at `target/release/echo`.

To run directly:
```bash
cargo run --release
```

#### Install the Desktop Entry

To integrate Echo with your GNOME application launcher:

```bash
# Copy the binary
sudo cp target/release/echo /usr/local/bin/

# Copy the desktop file
cp data/com.echo.Echo.desktop ~/.local/share/applications/

# Copy the icon
mkdir -p ~/.local/share/icons/hicolor/scalable/apps/
cp data/resources/icons/scalable/apps/com.echo.Echo.svg ~/.local/share/icons/hicolor/scalable/apps/

# Update icon cache
gtk4-update-icon-cache ~/.local/share/icons/hicolor/
```

## Usage

### First Run

On first launch, Echo will guide you through an onboarding wizard to set up your first AI provider account. You'll need an API key from at least one provider:

- **Google Gemini** — Get an API key at [aistudio.google.com](https://aistudio.google.com/apikey)
- **Anthropic Claude** — Get an API key at [console.anthropic.com](https://console.anthropic.com/)
- **Local models** — Point to a running local server (no API key required)

### Starting a Conversation

1. Click the **+** button or press `Ctrl+N` to start a new conversation
2. Select your preferred provider and model from the account selector
3. Type your message and press `Enter` to send

### Attaching Images

Drag and drop an image onto the input area, or use the attachment button to browse for images. Supported formats include PNG, JPEG, and WebP.

### System Prompts

Set a global default system prompt in **Preferences > Chat**, or override it per-conversation using the system prompt button in the chat header.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+N` | New conversation |
| `Ctrl+K` | Quick search / command palette |
| `Ctrl+F` | Search in conversation |
| `Ctrl+/` | Toggle sidebar |
| `Escape` | Cancel streaming response |

## Technology Stack

| Component | Technology |
|-----------|------------|
| Language | Rust |
| UI Framework | GTK4 + libadwaita |
| Architecture | Relm4 (Elm architecture) |
| Database | SQLite (via rusqlite) |
| HTTP Client | reqwest (rustls) |
| Async Runtime | Tokio |
| Markdown | pulldown-cmark |
| Keyring | oo7 (libsecret) |

## Project Structure

```
echo/
├── src/
│   ├── main.rs              # Application entry point
│   ├── app.rs               # Root component and message bus
│   ├── config.rs            # Build-time constants
│   ├── models/              # Data models (Account, Conversation, Message)
│   ├── providers/           # AI provider implementations
│   │   ├── traits.rs        # AiProvider trait
│   │   ├── router.rs        # Provider dispatch
│   │   ├── gemini/          # Google Gemini
│   │   ├── claude/          # Anthropic Claude
│   │   └── local/           # OpenAI-compatible local models
│   ├── services/            # Business logic
│   │   ├── database.rs      # SQLite operations
│   │   ├── keyring.rs       # Secure key storage
│   │   ├── chat.rs          # Chat orchestration
│   │   ├── markdown.rs      # Markdown rendering
│   │   └── ...
│   └── ui/                  # GTK4/Relm4 UI components
│       ├── chat_view.rs     # Message display
│       ├── sidebar.rs       # Conversation list
│       ├── input_area.rs    # Text input
│       ├── dialogs/         # Modal dialogs
│       └── preferences/     # Settings pages
├── data/
│   ├── com.echo.Echo.desktop
│   └── resources/
│       ├── style.css        # Custom GTK styling
│       └── icons/           # Application icon
├── Cargo.toml
└── build.rs                 # GResource compilation
```

## Roadmap

### Coming Soon

- **Syntax highlighting** — Code blocks with language-aware syntax coloring
- **Extended thinking** — Show Claude's thinking process in collapsible sections
- **Conversation branching** — Branch conversations from any message to explore alternatives
- **Full-text search** — Search across all conversations with SQLite FTS5

### Planned

- **Research mode** — Web search grounding for Gemini and Claude with inline citations
- **System prompt library** — Save, import/export, and quick-switch between prompt presets
- **Conversation folders & tags** — Organize conversations beyond pinning
- **Tool use / function calling** — Let models call tools and functions
- **PDF support** — Attach and discuss PDF documents
- **OpenAI provider** — Direct OpenAI API support

### Distribution

- **Flatpak packaging** — Publish on Flathub for easy installation
- **Snap packaging** — Publish on Snap Store
- **AppStream metadata** — Rich app listing in GNOME Software

### Long-term

- **Internationalization** — Translatable UI via gettext
- **Accessibility** — Full screen reader support with proper ATK labels
- **Voice input/output** — Speech-to-text and text-to-speech integration
- **Plugin system** — Extensible tool and provider plugins
- **Usage analytics** — Local per-account cost tracking dashboard

## Contributing

Contributions are welcome! Here's how to get started:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes
4. Build and test (`cargo build`)
5. Commit your changes (`git commit -m 'Add my feature'`)
6. Push to the branch (`git push origin feature/my-feature`)
7. Open a Pull Request

### Development Tips

- Set `RUST_LOG=debug` for verbose logging: `RUST_LOG=debug cargo run`
- The app uses an Elm-style architecture via Relm4 — state flows through messages
- Database operations are async via `spawn_blocking` to avoid blocking the UI thread

## License

Echo is licensed under the [GNU General Public License v3.0 or later](LICENSE).
