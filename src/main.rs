use base64::Engine;
use image::{imageops::FilterType, DynamicImage, ImageFormat};
use rdev::{simulate, Button, EventType, Key};
use screenshots::Screen;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::thread;
use std::time::Duration;

// ============================================================================
// MCP Protocol Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

// ============================================================================
// Tool Definitions
// ============================================================================

fn get_tools() -> Value {
    json!([
        {
            "name": "screen_capture",
            "description": "Capture a screenshot of a specific screen or region. Returns base64-encoded JPEG with automatic downscaling for efficiency. Use get_screen_info to list available screens.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "screen_index": {
                        "type": "integer",
                        "description": "Index of the screen to capture (default: 0, the primary screen). Use get_screen_info to see available screens.",
                        "default": 0
                    },
                    "region": {
                        "type": "object",
                        "description": "Optional region to capture (coordinates relative to the selected screen). If not provided, captures entire screen.",
                        "properties": {
                            "x": { "type": "integer", "description": "X coordinate of top-left corner" },
                            "y": { "type": "integer", "description": "Y coordinate of top-left corner" },
                            "width": { "type": "integer", "description": "Width of region" },
                            "height": { "type": "integer", "description": "Height of region" }
                        },
                        "required": ["x", "y", "width", "height"]
                    },
                    "max_width": {
                        "type": "integer",
                        "description": "Maximum width for downscaling (default: 1280)",
                        "default": 1280
                    },
                    "max_height": {
                        "type": "integer",
                        "description": "Maximum height for downscaling (default: 720)",
                        "default": 720
                    },
                    "quality": {
                        "type": "integer",
                        "description": "JPEG quality 1-100 (default: 80)",
                        "default": 80,
                        "minimum": 1,
                        "maximum": 100
                    }
                }
            }
        },
        {
            "name": "mouse_move",
            "description": "Move the mouse cursor to a specific position",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": { "type": "number", "description": "X coordinate" },
                    "y": { "type": "number", "description": "Y coordinate" }
                },
                "required": ["x", "y"]
            }
        },
        {
            "name": "mouse_click",
            "description": "Click the mouse at the current position or a specific location",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": { "type": "number", "description": "X coordinate (optional, uses current position if not provided)" },
                    "y": { "type": "number", "description": "Y coordinate (optional, uses current position if not provided)" },
                    "button": {
                        "type": "string",
                        "enum": ["left", "right", "middle"],
                        "description": "Mouse button to click (default: left)",
                        "default": "left"
                    },
                    "double_click": {
                        "type": "boolean",
                        "description": "Whether to double-click (default: false)",
                        "default": false
                    }
                }
            }
        },
        {
            "name": "mouse_drag",
            "description": "Drag the mouse from one position to another",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from_x": { "type": "number", "description": "Starting X coordinate" },
                    "from_y": { "type": "number", "description": "Starting Y coordinate" },
                    "to_x": { "type": "number", "description": "Ending X coordinate" },
                    "to_y": { "type": "number", "description": "Ending Y coordinate" },
                    "button": {
                        "type": "string",
                        "enum": ["left", "right", "middle"],
                        "description": "Mouse button to hold during drag (default: left)",
                        "default": "left"
                    },
                    "duration_ms": {
                        "type": "integer",
                        "description": "Duration of drag in milliseconds (default: 500)",
                        "default": 500
                    }
                },
                "required": ["from_x", "from_y", "to_x", "to_y"]
            }
        },
        {
            "name": "key_tap",
            "description": "Press and release a single key, optionally with modifiers",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key to press (e.g., 'a', 'Enter', 'Tab', 'F1', 'Escape')"
                    },
                    "modifiers": {
                        "type": "array",
                        "items": {
                            "type": "string",
                            "enum": ["shift", "control", "alt", "meta"]
                        },
                        "description": "Modifier keys to hold during the key press"
                    }
                },
                "required": ["key"]
            }
        },
        {
            "name": "type_text",
            "description": "Type a string of text character by character",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to type"
                    },
                    "delay_ms": {
                        "type": "integer",
                        "description": "Delay between keystrokes in milliseconds (default: 20)",
                        "default": 20
                    }
                },
                "required": ["text"]
            }
        },
        {
            "name": "get_screen_info",
            "description": "Get information about available screens",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }
    ])
}

// ============================================================================
// Screenshot Implementation
// ============================================================================

fn capture_screenshot(
    screen_index: Option<usize>,
    region: Option<(i32, i32, u32, u32)>,
    max_width: u32,
    max_height: u32,
    _quality: u8, // TODO: implement quality control for JPEG encoder
) -> Result<String, String> {
    // Capture screenshot
    let screens = Screen::all().map_err(|e| format!("Failed to get screens: {:?}", e))?;
    let idx = screen_index.unwrap_or(0);
    let screen = screens.get(idx).ok_or_else(|| {
        format!("Screen index {} not found. Available screens: 0-{}", idx, screens.len().saturating_sub(1))
    })?;
    let capture = screen
        .capture()
        .map_err(|e| format!("Failed to capture: {:?}", e))?;

    // Convert to DynamicImage
    let img = DynamicImage::ImageRgba8(
        image::RgbaImage::from_raw(capture.width(), capture.height(), capture.into_vec())
            .ok_or("Failed to create image from buffer")?,
    );

    // Crop if region specified
    let img = if let Some((x, y, w, h)) = region {
        img.crop_imm(x as u32, y as u32, w, h)
    } else {
        img
    };

    // Resize if needed
    let resized = if img.width() > max_width || img.height() > max_height {
        img.resize(max_width, max_height, FilterType::Lanczos3)
    } else {
        img
    };

    // Convert to JPEG
    let mut jpeg_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);

    resized
        .write_to(&mut cursor, ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode JPEG: {:?}", e))?;

    // Encode to base64
    let base64_str = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);

    Ok(base64_str)
}

fn get_screen_info() -> Result<Value, String> {
    let screens = Screen::all().map_err(|e| format!("Failed to get screens: {:?}", e))?;

    let screen_info: Vec<Value> = screens
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let info = &s.display_info;
            json!({
                "index": i,
                "id": info.id,
                "x": info.x,
                "y": info.y,
                "width": info.width,
                "height": info.height,
                "scale_factor": info.scale_factor,
                "is_primary": info.is_primary,
            })
        })
        .collect();

    Ok(json!({ 
        "screens": screen_info,
        "count": screens.len()
    }))
}

// ============================================================================
// Input Simulation Implementation
// ============================================================================

fn do_mouse_move(x: f64, y: f64) -> Result<(), String> {
    simulate(&EventType::MouseMove { x, y }).map_err(|e| format!("Mouse move failed: {:?}", e))
}

fn do_mouse_click(
    x: Option<f64>,
    y: Option<f64>,
    button: &str,
    double_click: bool,
) -> Result<(), String> {
    // Move if coordinates provided
    if let (Some(x), Some(y)) = (x, y) {
        do_mouse_move(x, y)?;
        thread::sleep(Duration::from_millis(10));
    }

    let btn = match button {
        "right" => Button::Right,
        "middle" => Button::Middle,
        _ => Button::Left,
    };

    // Click
    simulate(&EventType::ButtonPress(btn)).map_err(|e| format!("Button press failed: {:?}", e))?;
    thread::sleep(Duration::from_millis(10));
    simulate(&EventType::ButtonRelease(btn))
        .map_err(|e| format!("Button release failed: {:?}", e))?;

    // Double click if requested
    if double_click {
        thread::sleep(Duration::from_millis(50));
        simulate(&EventType::ButtonPress(btn))
            .map_err(|e| format!("Button press failed: {:?}", e))?;
        thread::sleep(Duration::from_millis(10));
        simulate(&EventType::ButtonRelease(btn))
            .map_err(|e| format!("Button release failed: {:?}", e))?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn do_drag_move(x: f64, y: f64, button: Button) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventType, CGMouseButton};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGPoint;

    let event_type = match button {
        Button::Left => CGEventType::LeftMouseDragged,
        Button::Right => CGEventType::RightMouseDragged,
        _ => CGEventType::OtherMouseDragged,
    };

    let cg_button = match button {
        Button::Left => CGMouseButton::Left,
        Button::Right => CGMouseButton::Right,
        _ => CGMouseButton::Center,
    };

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create event source")?;

    let point = CGPoint::new(x, y);
    let event = CGEvent::new_mouse_event(source, event_type, point, cg_button)
        .map_err(|_| "Failed to create drag event")?;

    event.post(core_graphics::event::CGEventTapLocation::HID);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn do_drag_move(x: f64, y: f64, _button: Button) -> Result<(), String> {
    simulate(&EventType::MouseMove { x, y }).map_err(|e| format!("Mouse move failed: {:?}", e))
}

fn do_mouse_drag(
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
    button: &str,
    duration_ms: u64,
) -> Result<(), String> {
    let btn = match button {
        "right" => Button::Right,
        "middle" => Button::Middle,
        _ => Button::Left,
    };

    // Move to start position
    do_mouse_move(from_x, from_y)?;
    thread::sleep(Duration::from_millis(20));

    // Press button
    simulate(&EventType::ButtonPress(btn)).map_err(|e| format!("Button press failed: {:?}", e))?;
    thread::sleep(Duration::from_millis(20));

    // Interpolate drag movement
    let steps = 20;
    let step_delay = duration_ms / steps;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let x = from_x + (to_x - from_x) * t;
        let y = from_y + (to_y - from_y) * t;
        do_drag_move(x, y, btn)?;
        thread::sleep(Duration::from_millis(step_delay));
    }

    // Release button
    simulate(&EventType::ButtonRelease(btn))
        .map_err(|e| format!("Button release failed: {:?}", e))?;

    Ok(())
}

fn parse_key(key_str: &str) -> Option<Key> {
    match key_str.to_lowercase().as_str() {
        "a" => Some(Key::KeyA),
        "b" => Some(Key::KeyB),
        "c" => Some(Key::KeyC),
        "d" => Some(Key::KeyD),
        "e" => Some(Key::KeyE),
        "f" => Some(Key::KeyF),
        "g" => Some(Key::KeyG),
        "h" => Some(Key::KeyH),
        "i" => Some(Key::KeyI),
        "j" => Some(Key::KeyJ),
        "k" => Some(Key::KeyK),
        "l" => Some(Key::KeyL),
        "m" => Some(Key::KeyM),
        "n" => Some(Key::KeyN),
        "o" => Some(Key::KeyO),
        "p" => Some(Key::KeyP),
        "q" => Some(Key::KeyQ),
        "r" => Some(Key::KeyR),
        "s" => Some(Key::KeyS),
        "t" => Some(Key::KeyT),
        "u" => Some(Key::KeyU),
        "v" => Some(Key::KeyV),
        "w" => Some(Key::KeyW),
        "x" => Some(Key::KeyX),
        "y" => Some(Key::KeyY),
        "z" => Some(Key::KeyZ),
        "0" => Some(Key::Num0),
        "1" => Some(Key::Num1),
        "2" => Some(Key::Num2),
        "3" => Some(Key::Num3),
        "4" => Some(Key::Num4),
        "5" => Some(Key::Num5),
        "6" => Some(Key::Num6),
        "7" => Some(Key::Num7),
        "8" => Some(Key::Num8),
        "9" => Some(Key::Num9),
        "enter" | "return" => Some(Key::Return),
        "tab" => Some(Key::Tab),
        "space" | " " => Some(Key::Space),
        "backspace" => Some(Key::Backspace),
        "delete" => Some(Key::Delete),
        "escape" | "esc" => Some(Key::Escape),
        "up" | "uparrow" => Some(Key::UpArrow),
        "down" | "downarrow" => Some(Key::DownArrow),
        "left" | "leftarrow" => Some(Key::LeftArrow),
        "right" | "rightarrow" => Some(Key::RightArrow),
        "home" => Some(Key::Home),
        "end" => Some(Key::End),
        "pageup" => Some(Key::PageUp),
        "pagedown" => Some(Key::PageDown),
        "f1" => Some(Key::F1),
        "f2" => Some(Key::F2),
        "f3" => Some(Key::F3),
        "f4" => Some(Key::F4),
        "f5" => Some(Key::F5),
        "f6" => Some(Key::F6),
        "f7" => Some(Key::F7),
        "f8" => Some(Key::F8),
        "f9" => Some(Key::F9),
        "f10" => Some(Key::F10),
        "f11" => Some(Key::F11),
        "f12" => Some(Key::F12),
        "capslock" => Some(Key::CapsLock),
        "shift" => Some(Key::ShiftLeft),
        "control" | "ctrl" => Some(Key::ControlLeft),
        "alt" | "option" => Some(Key::Alt),
        "meta" | "command" | "cmd" | "super" | "win" => Some(Key::MetaLeft),
        "-" | "minus" => Some(Key::Minus),
        "=" | "equal" => Some(Key::Equal),
        "[" | "leftbracket" => Some(Key::LeftBracket),
        "]" | "rightbracket" => Some(Key::RightBracket),
        "\\" | "backslash" => Some(Key::BackSlash),
        ";" | "semicolon" => Some(Key::SemiColon),
        "'" | "quote" => Some(Key::Quote),
        "," | "comma" => Some(Key::Comma),
        "." | "dot" | "period" => Some(Key::Dot),
        "/" | "slash" => Some(Key::Slash),
        "`" | "backquote" | "grave" => Some(Key::BackQuote),
        _ => None,
    }
}

fn get_modifier_key(modifier: &str) -> Option<Key> {
    match modifier.to_lowercase().as_str() {
        "shift" => Some(Key::ShiftLeft),
        "control" | "ctrl" => Some(Key::ControlLeft),
        "alt" | "option" => Some(Key::Alt),
        "meta" | "command" | "cmd" | "super" | "win" => Some(Key::MetaLeft),
        _ => None,
    }
}

fn do_key_tap(key_str: &str, modifiers: &[String]) -> Result<(), String> {
    let key = parse_key(key_str).ok_or_else(|| format!("Unknown key: {}", key_str))?;

    // Press modifiers
    for modifier in modifiers {
        if let Some(mod_key) = get_modifier_key(modifier) {
            simulate(&EventType::KeyPress(mod_key))
                .map_err(|e| format!("Modifier press failed: {:?}", e))?;
        }
    }

    thread::sleep(Duration::from_millis(10));

    // Press and release key
    simulate(&EventType::KeyPress(key)).map_err(|e| format!("Key press failed: {:?}", e))?;
    thread::sleep(Duration::from_millis(10));
    simulate(&EventType::KeyRelease(key)).map_err(|e| format!("Key release failed: {:?}", e))?;

    thread::sleep(Duration::from_millis(10));

    // Release modifiers (in reverse order)
    for modifier in modifiers.iter().rev() {
        if let Some(mod_key) = get_modifier_key(modifier) {
            simulate(&EventType::KeyRelease(mod_key))
                .map_err(|e| format!("Modifier release failed: {:?}", e))?;
        }
    }

    Ok(())
}

fn do_type_text(text: &str, delay_ms: u64) -> Result<(), String> {
    for c in text.chars() {
        let (key, needs_shift) = char_to_key(c);

        if let Some(k) = key {
            if needs_shift {
                simulate(&EventType::KeyPress(Key::ShiftLeft))
                    .map_err(|e| format!("Shift press failed: {:?}", e))?;
                thread::sleep(Duration::from_millis(5));
            }

            simulate(&EventType::KeyPress(k)).map_err(|e| format!("Key press failed: {:?}", e))?;
            thread::sleep(Duration::from_millis(5));
            simulate(&EventType::KeyRelease(k))
                .map_err(|e| format!("Key release failed: {:?}", e))?;

            if needs_shift {
                thread::sleep(Duration::from_millis(5));
                simulate(&EventType::KeyRelease(Key::ShiftLeft))
                    .map_err(|e| format!("Shift release failed: {:?}", e))?;
            }

            thread::sleep(Duration::from_millis(delay_ms));
        }
    }

    Ok(())
}

fn char_to_key(c: char) -> (Option<Key>, bool) {
    match c {
        'a'..='z' => (parse_key(&c.to_string()), false),
        'A'..='Z' => (parse_key(&c.to_lowercase().to_string()), true),
        '0'..='9' => (parse_key(&c.to_string()), false),
        ' ' => (Some(Key::Space), false),
        '\n' => (Some(Key::Return), false),
        '\t' => (Some(Key::Tab), false),
        '-' => (Some(Key::Minus), false),
        '=' => (Some(Key::Equal), false),
        '[' => (Some(Key::LeftBracket), false),
        ']' => (Some(Key::RightBracket), false),
        '\\' => (Some(Key::BackSlash), false),
        ';' => (Some(Key::SemiColon), false),
        '\'' => (Some(Key::Quote), false),
        ',' => (Some(Key::Comma), false),
        '.' => (Some(Key::Dot), false),
        '/' => (Some(Key::Slash), false),
        '`' => (Some(Key::BackQuote), false),
        // Shifted characters
        '!' => (Some(Key::Num1), true),
        '@' => (Some(Key::Num2), true),
        '#' => (Some(Key::Num3), true),
        '$' => (Some(Key::Num4), true),
        '%' => (Some(Key::Num5), true),
        '^' => (Some(Key::Num6), true),
        '&' => (Some(Key::Num7), true),
        '*' => (Some(Key::Num8), true),
        '(' => (Some(Key::Num9), true),
        ')' => (Some(Key::Num0), true),
        '_' => (Some(Key::Minus), true),
        '+' => (Some(Key::Equal), true),
        '{' => (Some(Key::LeftBracket), true),
        '}' => (Some(Key::RightBracket), true),
        '|' => (Some(Key::BackSlash), true),
        ':' => (Some(Key::SemiColon), true),
        '"' => (Some(Key::Quote), true),
        '<' => (Some(Key::Comma), true),
        '>' => (Some(Key::Dot), true),
        '?' => (Some(Key::Slash), true),
        '~' => (Some(Key::BackQuote), true),
        _ => (None, false),
    }
}

// ============================================================================
// Tool Execution
// ============================================================================

fn execute_tool(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "screen_capture" => {
            let screen_index = args
                .get("screen_index")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let region = args.get("region").and_then(|r| {
                Some((
                    r.get("x")?.as_i64()? as i32,
                    r.get("y")?.as_i64()? as i32,
                    r.get("width")?.as_u64()? as u32,
                    r.get("height")?.as_u64()? as u32,
                ))
            });
            let max_width = args
                .get("max_width")
                .and_then(|v| v.as_u64())
                .unwrap_or(1280) as u32;
            let max_height = args
                .get("max_height")
                .and_then(|v| v.as_u64())
                .unwrap_or(720) as u32;
            let quality = args.get("quality").and_then(|v| v.as_u64()).unwrap_or(80) as u8;

            let base64_data = capture_screenshot(screen_index, region, max_width, max_height, quality)?;

            Ok(json!({
                "type": "image",
                "format": "jpeg",
                "encoding": "base64",
                "data": base64_data
            }))
        }

        "mouse_move" => {
            let x = args
                .get("x")
                .and_then(|v| v.as_f64())
                .ok_or("Missing x coordinate")?;
            let y = args
                .get("y")
                .and_then(|v| v.as_f64())
                .ok_or("Missing y coordinate")?;

            do_mouse_move(x, y)?;
            Ok(json!({ "success": true, "position": { "x": x, "y": y } }))
        }

        "mouse_click" => {
            let x = args.get("x").and_then(|v| v.as_f64());
            let y = args.get("y").and_then(|v| v.as_f64());
            let button = args
                .get("button")
                .and_then(|v| v.as_str())
                .unwrap_or("left");
            let double_click = args
                .get("double_click")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            do_mouse_click(x, y, button, double_click)?;
            Ok(json!({
                "success": true,
                "button": button,
                "double_click": double_click
            }))
        }

        "mouse_drag" => {
            let from_x = args
                .get("from_x")
                .and_then(|v| v.as_f64())
                .ok_or("Missing from_x")?;
            let from_y = args
                .get("from_y")
                .and_then(|v| v.as_f64())
                .ok_or("Missing from_y")?;
            let to_x = args
                .get("to_x")
                .and_then(|v| v.as_f64())
                .ok_or("Missing to_x")?;
            let to_y = args
                .get("to_y")
                .and_then(|v| v.as_f64())
                .ok_or("Missing to_y")?;
            let button = args
                .get("button")
                .and_then(|v| v.as_str())
                .unwrap_or("left");
            let duration_ms = args
                .get("duration_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(500);

            do_mouse_drag(from_x, from_y, to_x, to_y, button, duration_ms)?;
            Ok(json!({
                "success": true,
                "from": { "x": from_x, "y": from_y },
                "to": { "x": to_x, "y": to_y }
            }))
        }

        "key_tap" => {
            let key = args
                .get("key")
                .and_then(|v| v.as_str())
                .ok_or("Missing key")?;
            let modifiers: Vec<String> = args
                .get("modifiers")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            do_key_tap(key, &modifiers)?;
            Ok(json!({
                "success": true,
                "key": key,
                "modifiers": modifiers
            }))
        }

        "type_text" => {
            let text = args
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or("Missing text")?;
            let delay_ms = args
                .get("delay_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(20);

            do_type_text(text, delay_ms)?;
            Ok(json!({
                "success": true,
                "length": text.len()
            }))
        }

        "get_screen_info" => get_screen_info(),

        _ => Err(format!("Unknown tool: {}", name)),
    }
}

// ============================================================================
// MCP Protocol Handler
// ============================================================================

fn handle_request(request: &JsonRpcRequest) -> JsonRpcResponse {
    let id = request.id.clone().unwrap_or(Value::Null);

    let result = match request.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": "desktop-mcp",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": {}
            }
        })),

        "notifications/initialized" => {
            // This is a notification, no response needed
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(Value::Null),
                error: None,
            };
        }

        "tools/list" => Ok(json!({
            "tools": get_tools()
        })),

        "tools/call" => {
            let tool_name = request
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or(json!({}));

            match execute_tool(tool_name, &arguments) {
                Ok(result) => Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result).unwrap_or_default()
                    }]
                })),
                Err(e) => Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Error: {}", e)
                    }],
                    "isError": true
                })),
            }
        }

        _ => Err(JsonRpcError {
            code: -32601,
            message: format!("Method not found: {}", request.method),
            data: None,
        }),
    };

    match result {
        Ok(result) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        },
        Err(error) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        },
    }
}

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() {
    eprintln!("desktop-mcp v{} starting...", env!("CARGO_PKG_VERSION"));

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Error reading stdin: {}", e);
                continue;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error parsing JSON: {} - line: {}", e, line);
                continue;
            }
        };

        let response = handle_request(&request);
        let response_json = serde_json::to_string(&response).unwrap();

        writeln!(stdout, "{}", response_json).unwrap();
        stdout.flush().unwrap();
    }
}
