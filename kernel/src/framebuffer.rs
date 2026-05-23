use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use core::fmt;
use spin::Mutex;

use crate::font::{CHAR_HEIGHT, CHAR_WIDTH, FALLBACK_INDEX, FONT_BASIC, FONT_OFFSET};

pub struct FrameBufferWriter {
    buffer: &'static mut [u8],
    info: FrameBufferInfo,
    x_pos: usize,
    y_pos: usize,
}

impl FrameBufferWriter {
    pub fn new(buffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        Self {
            buffer,
            info,
            x_pos: 0,
            y_pos: 0,
        }
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

        for row in 0..CHAR_HEIGHT {
            let glyph_byte = FONT_BASIC[glyph_start + row];
            for col in 0..CHAR_WIDTH {
                if glyph_byte & (1 << (7 - col)) != 0 {
                    self.write_pixel(x + col, y + row, 255, 255, 255);
                } else {
                    self.write_pixel(x + col, y + row, 0, 0, 0);
                }
            }
        }
    }

    #[allow(dead_code)]
    fn clear_row(&mut self, y: usize) {
        let row_start = y * self.info.stride * self.info.bytes_per_pixel;
        let row_end = (y + CHAR_HEIGHT) * self.info.stride * self.info.bytes_per_pixel;
        if row_end <= self.buffer.len() {
            self.buffer[row_start..row_end].fill(0);
        }
    }

    fn scroll_up(&mut self) {
        let bytes_per_char_row = self.info.stride * self.info.bytes_per_pixel * CHAR_HEIGHT;
        let total_bytes = self.info.height * self.info.stride * self.info.bytes_per_pixel;

        if total_bytes <= bytes_per_char_row {
            return;
        }

        self.buffer.copy_within(bytes_per_char_row..total_bytes, 0);

        let clear_start = total_bytes - bytes_per_char_row;
        self.buffer[clear_start..total_bytes].fill(0);
    }

    fn new_line(&mut self) {
        self.y_pos += CHAR_HEIGHT;
        self.x_pos = 0;

        if self.y_pos + CHAR_HEIGHT > self.info.height {
            self.scroll_up();
            self.y_pos -= CHAR_HEIGHT;
        }
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.x_pos + CHAR_WIDTH > self.info.width {
                    self.new_line();
                }
                self.draw_char(self.x_pos, self.y_pos, byte);
                self.x_pos += CHAR_WIDTH;
            }
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0xfe),
            }
        }
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
    let writer = FrameBufferWriter::new(buffer, info);
    *FRAMEBUFFER.lock() = Some(writer);
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
