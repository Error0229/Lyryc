# 🎵 Lyryc - Music Lyrics Synchronization App

A cross-platform desktop application that displays real-time synchronized lyrics for music playing in your browser. Built with Tauri, React, and TypeScript.

## ✨ Features

- **Real-time Lyrics Display**: Shows synchronized lyrics as your music plays
- **Browser Integration**: Detects music from Spotify Web Player and YouTube Music
- **Beautiful UI**: Modern, responsive interface with smooth animations
- **Free API Integration**: Uses LRCLIB for free, high-quality synchronized lyrics
- **Cross-platform**: Runs on Windows, macOS, and Linux
- **Lightweight**: Built with Tauri for minimal resource usage

## 🚀 Quick Start

### Prerequisites

- [Node.js](https://nodejs.org/) (v16 or later)
- [Rust](https://rustup.rs/) (latest stable)
- [Git](https://git-scm.com/)

### Installation

1. **Clone the repository**
   ```bash
   git clone https://github.com/your-username/lyryc.git
   cd lyryc
   ```

2. **Install dependencies**
   ```bash
   npm install
   ```

3. **Run the development server**
   ```bash
   npm run tauri dev
   ```

## 🎯 How It Works

### Architecture Overview

```
Browser Music → Extension → Desktop App → Lyrics Display
     ↓              ↓           ↓            ↓
  Spotify/YT    Track Info   Processing   Sync Effects
```

### Components

- **Browser Extension**: Detects currently playing music
- **Tauri Backend**: Rust-based backend for API calls and system integration
- **React Frontend**: Modern UI with synchronized lyrics display
- **LRCLIB Service**: Free lyrics database with timing information

## 🔧 Development

### Project Structure

```
lyryc/
├── src/                    # React frontend
│   ├── components/         # UI components
│   ├── services/          # API services
│   ├── stores/            # State management
│   └── hooks/             # Custom React hooks
├── src-tauri/             # Rust backend
│   └── src/
├── extension/             # Browser extension
│   ├── manifest.json
│   ├── background.js
│   └── content scripts
└── public/                # Static assets
```

### Available Scripts

- `npm run dev` - Start Vite development server
- `npm run tauri dev` - Run Tauri in development mode
- `npm run build` - Build for production
- `npm run tauri build` - Create production bundle

### Browser Extension Setup

1. **Load the extension**:
   - Open Chrome/Edge: `chrome://extensions/`
   - Enable "Developer mode"
   - Click "Load unpacked" and select the `extension` folder

2. **Test the extension**:
   - Visit Spotify Web Player or YouTube Music
   - Play a song
   - The extension should detect the track

## 🎪 Demo Mode

The app includes a demo mode that loads "Blinding Lights" by The Weeknd with synchronized lyrics. You can:

- ▶️ Play/pause to see lyrics sync in real-time
- 🎯 Click on any lyric line to jump to that position
- 📱 Toggle auto-scroll on/off
- 🎚️ Use the progress bar to scrub through the song

## 🛠️ Configuration

### Tauri Configuration

Key settings in `src-tauri/tauri.conf.json`:

```json
{
  "app": {
    "windows": [
      {
        "title": "Lyryc - Music Lyrics Sync",
        "width": 1200,
        "height": 800,
        "alwaysOnTop": false
      }
    ]
  }
}
```

### Environment Variables

Create `.env` for custom settings:

```bash
VITE_API_BASE_URL=https://lrclib.net/api
VITE_ENABLE_DEBUG=true
```

## 📚 API Reference

### LRCLIB Integration

The app uses [LRCLIB](https://lrclib.net/) for lyrics:

```typescript
// Search for lyrics
const lyrics = await LRCLibService.searchLyrics({
  track_name: "Song Title",
  artist_name: "Artist Name"
});
```

### Tauri Commands

Available Rust commands:

- `fetch_lyrics(track_name, artist_name)` - Get lyrics from backend
- `get_current_track()` - Get currently playing track
- `set_current_track(track)` - Update current track

## 🎨 Customization

### Themes

Modify `src/index.css` to customize the appearance:

```css
.lyrics-container {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
}
```

### Animation Settings

Adjust timing in `src/components/LyricsViewer.tsx`:

```typescript
const animationConfig = {
  duration: 0.3,
  ease: "easeInOut"
};
```

## 🐛 Troubleshooting

### Common Issues

1. **Tauri won't start**
   - Ensure Rust is installed: `rustc --version`
   - Try: `cargo clean` in `src-tauri/`

2. **Extension not detecting music**
   - Check if the extension is loaded
   - Refresh the music player page
   - Check browser console for errors

3. **No lyrics found**
   - Verify track name and artist are correct
   - Check LRCLIB availability
   - Try alternative lyrics sources

### Debug Mode

Enable debug logging:

```bash
RUST_LOG=debug npm run tauri dev
```

## 📈 Roadmap

- [ ] Native messaging for better browser integration
- [ ] Local lyrics file support (.lrc, .srt)
- [ ] AI-powered lyrics alignment
- [ ] Karaoke mode with word-level highlighting
- [ ] Custom theme system
- [ ] Lyrics editing and contribution features
- [ ] Support for more music services

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Commit changes: `git commit -m 'Add amazing feature'`
4. Push to branch: `git push origin feature/amazing-feature`
5. Open a Pull Request

## 📜 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [LRCLIB](https://lrclib.net/) for providing free synchronized lyrics
- [Tauri](https://tauri.app/) for the amazing desktop app framework
- [Framer Motion](https://www.framer.com/motion/) for beautiful animations
- The open-source community for inspiration and tools

## 📞 Support

- 🐛 [Report Issues](https://github.com/your-username/lyryc/issues)
- 💬 [Discussions](https://github.com/your-username/lyryc/discussions)
- 📧 Email: your-email@example.com

---

**Made with ❤️ for music lovers everywhere**