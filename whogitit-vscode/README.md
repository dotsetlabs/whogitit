# whogitit VS Code Extension

Track and visualize AI-generated code at line level directly in VS Code.

## Features

- **Gutter Markers**: See AI attribution at a glance with colored markers
  - 🟢 Green: AI-generated lines (unchanged)
  - 🟡 Yellow: AI-generated lines (modified by human)
  - 🔵 Blue: Human-written lines

- **Hover Tooltips**: Hover over any AI-generated line to see:
  - Source type (AI, AI Modified, Human)
  - Prompt preview that generated the code
  - Commit and author information

- **CodeLens**: Above AI-generated regions, see:
  - Number of AI-generated lines
  - Quick access to view the full prompt

- **Status Bar**: At-a-glance view of AI percentage for current file

## Requirements

- [whogitit](https://github.com/dotsetlabs/whogitit) CLI installed and in PATH
- Git repository with whogitit notes

## Installation

1. Install from VS Code Marketplace (coming soon)
2. Or install from VSIX:
   ```bash
   cd whogitit-vscode
   npm install
   npm run package
   code --install-extension whogitit-0.1.0.vsix
   ```

## Commands

- `whogitit: Show AI Prompt for Line` - View the full prompt for the current line
- `whogitit: Toggle AI Attribution Decorations` - Enable/disable decorations
- `whogitit: Refresh AI Attribution` - Refresh attribution data
- `whogitit: Show AI Statistics for File` - View detailed stats

## Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `whogitit.enabled` | `true` | Enable AI attribution decorations |
| `whogitit.showGutterMarkers` | `true` | Show markers in the gutter |
| `whogitit.showHoverTooltips` | `true` | Show details on hover |
| `whogitit.showCodeLens` | `true` | Show CodeLens above AI regions |
| `whogitit.showStatusBar` | `true` | Show AI % in status bar |
| `whogitit.whogititPath` | `whogitit` | Path to whogitit binary |

## Development

```bash
# Install dependencies
npm install

# Compile
npm run compile

# Watch for changes
npm run watch

# Package
npm run package
```

## License

MIT
