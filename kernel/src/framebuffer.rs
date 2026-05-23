use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use core::fmt;
use spin::Mutex;

use crate::font::{CHAR_HEIGHT, CHAR_WIDTH, FALLBACK_INDEX, FONT_BASIC, FONT_OFFSET};

const CURSOR_HEIGHT: usize = 2;
const MIN_SCALE: u8 = 1;
const MAX_SCALE: u8 = 4;

const ANSI_COLORS: [(u8, u8, u8); 8] = [
    (0, 0, 0),
    (170, 0, 0),
    (0, 170, 0),
    (170, 85, 0),
    (0, 0, 170),
    (170, 0, 170),
    (0, 170, 170),
    (170, 170, 170),
];

#[derive(Clone, Copy, PartialEq)]
enum EscapeState {
    Normal,
    SawEsc,
    SawBracket,
    InCsi,
}

pub struct FrameBufferWriter {
    buffer: &'static mut [u8],
    info: FrameBufferInfo,
    x_pos: usize,
    y_pos: usize,
    fg: (u8, u8, u8),
    bg: (u8, u8, u8),
    scale_factor: u8,
    escape_state: EscapeState,
    csi_buf: [u8; 8],
    csi_pos: usize,
    cursor_shown: bool,
}

impl FrameBufferWriter {
    pub fn new(buffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        Self {
            buffer,
            info,
            x_pos: 0,
            y_pos: 0,
            fg: (170, 170, 170),
            bg: (0, 0, 0),
            scale_factor: 1,
            escape_state: EscapeState::Normal,
            csi_buf: [0; 8],
            csi_pos: 0,
            cursor_shown: false,
        }
    }

    fn cell_width(&self) -> usize {
        CHAR_WIDTH * self.scale_factor as usize
    }

    fn cell_height(&self) -> usize {
        CHAR_HEIGHT * self.scale_factor as usize
    }

    fn write_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }
        let offset = (y * self.info.stride + x) * self.info.bytes_per_pixel;
        if offset + 2 >= self.buffer.len() {
            return;
        }
        match self.info.pixel_format {
            PixelFormat::Rgb => {
                self.buffer[offset] = r;
                self.buffer[offset + 1] = g;
                self.buffer[offset + 2] = b;
            }
            PixelFormat::Bgr => {
                self.buffer[offset] = b;
                self.buffer[offset + 1] = g;
                self.buffer[offset + 2] = r;
            }
            _ => {
                self.buffer[offset] = r;
                self.buffer[offset + 1] = g;
                self.buffer[offset + 2] = b;
            }
        }
    }

    fn draw_char(&mut self, x: usize, y: usize, c: u8) {
        let glyph_idx = if c >= FONT_OFFSET as u8 && c < (FONT_OFFSET + FALLBACK_INDEX) as u8 {
            (c - FONT_OFFSET as u8) as usize
        } else {
            FALLBACK_INDEX
        };
        let glyph_start = glyph_idx * CHAR_HEIGHT;
        let s = self.scale_factor as usize;

        for row in 0..CHAR_HEIGHT {
            let glyph_byte = FONT_BASIC[glyph_start + row];
            for col in 0..CHAR_WIDTH {
                let color = if glyph_byte & (1 << (7 - col)) != 0 {
                    (self.fg.0, self.fg.1, self.fg.2)
                } else {
                    (self.bg.0, self.bg.1, self.bg.2)
                };
                let (r, g, b) = color;
                for sy in 0..s {
                    for sx in 0..s {
                        self.write_pixel(x + col * s + sx, y + row * s + sy, r, g, b);
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    fn clear_row(&mut self, y: usize) {
        let ch = self.cell_height();
        let row_start = y * self.info.stride * self.info.bytes_per_pixel;
        let row_end = (y + ch) * self.info.stride * self.info.bytes_per_pixel;
        if row_end <= self.buffer.len() {
            self.buffer[row_start..row_end].fill(0);
        }
    }

    fn scroll_up(&mut self) {
        let ch = self.cell_height();
        let bytes_per_char_row = self.info.stride * self.info.bytes_per_pixel * ch;
        let total_bytes = self.info.height * self.info.stride * self.info.bytes_per_pixel;

        if total_bytes <= bytes_per_char_row {
            return;
        }

        self.buffer.copy_within(bytes_per_char_row..total_bytes, 0);

        let clear_start = total_bytes - bytes_per_char_row;
        self.buffer[clear_start..total_bytes].fill(0);
    }

    fn new_line(&mut self) {
        let ch = self.cell_height();
        self.y_pos += ch;
        self.x_pos = 0;

        if self.y_pos + ch > self.info.height {
            self.scroll_up();
            self.y_pos -= ch;
        }
    }

    fn apply_sgr(&mut self) {
        let buf = core::str::from_utf8(&self.csi_buf[..self.csi_pos]).unwrap_or("");
        let param: u32 = buf
            .split(';')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        match param {
            0 => {
                self.fg = (170, 170, 170);
                self.bg = (0, 0, 0);
            }
            30..=37 => {
                self.fg = ANSI_COLORS[(param - 30) as usize];
            }
            40..=47 => {
                self.bg = ANSI_COLORS[(param - 40) as usize];
            }
            _ => {}
        }
    }

    fn erase_cursor(&mut self) {
        if !self.cursor_shown {
            return;
        }
        let (r, g, b) = self.bg;
        let cw = self.cell_width();
        let ch = self.cell_height();
        let cursor_h = CURSOR_HEIGHT * self.scale_factor as usize;
        let cursor_y = self.y_pos + ch - cursor_h;
        for y in 0..cursor_h {
            for x in 0..cw {
                self.write_pixel(self.x_pos + x, cursor_y + y, r, g, b);
            }
        }
        self.cursor_shown = false;
    }

    fn draw_cursor(&mut self) {
        if self.cursor_shown {
            return;
        }
        let (r, g, b) = self.fg;
        let cw = self.cell_width();
        let ch = self.cell_height();
        let cursor_h = CURSOR_HEIGHT * self.scale_factor as usize;
        let cursor_y = self.y_pos + ch - cursor_h;
        for y in 0..cursor_h {
            for x in 0..cw {
                self.write_pixel(self.x_pos + x, cursor_y + y, r, g, b);
            }
        }
        self.cursor_shown = true;
    }

    pub fn write_byte(&mut self, byte: u8) {
        match self.escape_state {
            EscapeState::Normal => {
                let cw = self.cell_width();
                match byte {
                    0x1b => self.escape_state = EscapeState::SawEsc,
                    b'\n' => {
                        self.erase_cursor();
                        self.new_line();
                        self.draw_cursor();
                    }
                    0x08 => {
                        self.erase_cursor();
                        if self.x_pos >= cw {
                            self.x_pos -= cw;
                        }
                        self.draw_cursor();
                    }
                    byte => {
                        if self.x_pos + cw > self.info.width {
                            self.erase_cursor();
                            self.new_line();
                        }
                        self.draw_char(self.x_pos, self.y_pos, byte);
                        self.cursor_shown = false;
                        self.x_pos += cw;
                        self.draw_cursor();
                    }
                }
            }
            EscapeState::SawEsc => {
                self.escape_state = if byte == b'[' {
                    EscapeState::SawBracket
                } else {
                    EscapeState::Normal
                };
            }
            EscapeState::SawBracket => {
                if byte.is_ascii_digit() || byte == b';' {
                    self.escape_state = EscapeState::InCsi;
                    self.csi_buf[0] = byte;
                    self.csi_pos = 1;
                } else {
                    self.escape_state = EscapeState::Normal;
                }
            }
            EscapeState::InCsi => {
                if (byte.is_ascii_digit() || byte == b';') && self.csi_pos < self.csi_buf.len() {
                    self.csi_buf[self.csi_pos] = byte;
                    self.csi_pos += 1;
                } else if byte == b'm' {
                    self.apply_sgr();
                    self.escape_state = EscapeState::Normal;
                } else {
                    self.escape_state = EscapeState::Normal;
                }
            }
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' | 0x08 | 0x1b => self.write_byte(byte),
                _ => self.write_byte(0xfe),
            }
        }
    }

    pub fn read_pixel(&self, x: usize, y: usize) -> Option<(u8, u8, u8)> {
        if x >= self.info.width || y >= self.info.height {
            return None;
        }
        let offset = (y * self.info.stride + x) * self.info.bytes_per_pixel;
        if offset + 2 >= self.buffer.len() {
            return None;
        }
        match self.info.pixel_format {
            PixelFormat::Rgb => Some((
                self.buffer[offset],
                self.buffer[offset + 1],
                self.buffer[offset + 2],
            )),
            PixelFormat::Bgr => Some((
                self.buffer[offset + 2],
                self.buffer[offset + 1],
                self.buffer[offset],
            )),
            _ => Some((
                self.buffer[offset],
                self.buffer[offset + 1],
                self.buffer[offset + 2],
            )),
        }
    }

    pub fn cursor_position(&self) -> (usize, usize) {
        (self.x_pos, self.y_pos)
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.info.width, self.info.height)
    }

    pub fn clear_screen(&mut self) {
        self.buffer.fill(0);
        self.x_pos = 0;
        self.y_pos = 0;
        self.cursor_shown = false;
        self.draw_cursor();
    }

    pub fn set_cursor(&mut self, x: usize, y: usize) {
        self.erase_cursor();
        self.x_pos = x;
        self.y_pos = y;
        self.draw_cursor();
    }

    pub fn fg_color(&self) -> (u8, u8, u8) {
        self.fg
    }

    pub fn bg_color(&self) -> (u8, u8, u8) {
        self.bg
    }

    pub fn scale_factor(&self) -> u8 {
        self.scale_factor
    }

    pub fn scale_up(&mut self) -> bool {
        if self.scale_factor >= MAX_SCALE {
            return false;
        }
        self.scale_factor += 1;
        self.clear_screen();
        true
    }

    pub fn scale_down(&mut self) -> bool {
        if self.scale_factor <= MIN_SCALE {
            return false;
        }
        self.scale_factor -= 1;
        self.clear_screen();
        true
    }
}

impl fmt::Debug for FrameBufferWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameBufferWriter")
            .field("buffer", &self.buffer.as_ptr())
            .field("width", &self.info.width)
            .field("height", &self.info.height)
            .field("bytes_per_pixel", &self.info.bytes_per_pixel)
            .field("x_pos", &self.x_pos)
            .field("y_pos", &self.y_pos)
            .finish()
    }
}

impl fmt::Write for FrameBufferWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

static FRAMEBUFFER: Mutex<Option<FrameBufferWriter>> = Mutex::new(None);

pub fn init(buffer: &'static mut [u8], info: FrameBufferInfo) {
    let mut writer = FrameBufferWriter::new(buffer, info);
    writer.clear_screen();
    *FRAMEBUFFER.lock() = Some(writer);
}

pub fn with_framebuffer<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&FrameBufferWriter) -> R,
{
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| FRAMEBUFFER.lock().as_ref().map(f))
}

pub fn with_framebuffer_mut<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut FrameBufferWriter) -> R,
{
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| FRAMEBUFFER.lock().as_mut().map(f))
}

pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        if let Some(writer) = FRAMEBUFFER.lock().as_mut() {
            writer.write_fmt(args).expect("Framebuffer write failed");
        }
    });
}

pub fn scale_up() -> bool {
    with_framebuffer_mut(|fb| fb.scale_up()).unwrap_or(false)
}

pub fn scale_down() -> bool {
    with_framebuffer_mut(|fb| fb.scale_down()).unwrap_or(false)
}

pub fn scale_factor() -> Option<u8> {
    with_framebuffer(|fb| fb.scale_factor())
}
