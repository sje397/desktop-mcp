# desktop-mcp

A Model Context Protocol (MCP) server for desktop automation. Capture screenshots, control mouse and keyboard - all through a clean MCP interface.

## Features

- **Screenshot capture** with automatic downscaling (configurable resolution and quality)
- **Mouse control** - move, click, double-click, drag
- **Keyboard input** - key taps with modifiers, text typing
- **Cross-platform** - macOS support (Windows/Linux planned)

## Installation

### Build from source

```bash
git clone https://github.com/scottjellis/desktop-mcp
cd desktop-mcp
cargo build --release
```

The binary will be at `target/release/desktop-mcp`.

## Usage

### With Claude Desktop / MCP Client

Add to your MCP configuration:

```json
{
  "servers": [
    {
      "name": "desktop",
      "type": "stdio",
      "command": "/path/to/desktop-mcp",
      "args": [],
      "enabled": true
    }
  ]
}
```

### Available Tools

#### `screen_capture`
Capture a screenshot of the entire screen or a specific region.

```json
{
  "region": { "x": 0, "y": 0, "width": 800, "height": 600 },
  "max_width": 1280,
  "max_height": 720,
  "quality": 80
}
```

Returns base64-encoded JPEG with automatic downscaling for efficient transmission.

#### `mouse_move`
Move the mouse cursor to a specific position.

```json
{ "x": 100, "y": 200 }
```

#### `mouse_click`
Click the mouse at current or specified position.

```json
{
  "x": 100,
  "y": 200,
  "button": "left",
  "double_click": false
}
```

#### `mouse_drag`
Drag from one position to another.

```json
{
  "from_x": 100,
  "from_y": 200,
  "to_x": 300,
  "to_y": 400,
  "button": "left",
  "duration_ms": 500
}
```

#### `key_tap`
Press a single key with optional modifiers.

```json
{
  "key": "c",
  "modifiers": ["meta"]
}
```

#### `type_text`
Type a string of text.

```json
{
  "text": "Hello, world!",
  "delay_ms": 20
}
```

#### `get_screen_info`
Get information about available screens.

## macOS Permissions

On macOS, you'll need to grant permissions:

1. **Screen Recording** - System Preferences → Privacy & Security → Screen Recording
2. **Accessibility** - System Preferences → Privacy & Security → Accessibility

Add permission for the terminal/application running the MCP server.

## Development

```bash
# Build debug
cargo build

# Build release
cargo build --release

# Run tests
cargo test
```

## License

MIT

## Credits

Built with:
- [rdev](https://github.com/Narsil/rdev) - Cross-platform input simulation
- [screenshots](https://github.com/nicholaslee119/screenshots-rs) - Screen capture
- [image](https://github.com/image-rs/image) - Image processing
