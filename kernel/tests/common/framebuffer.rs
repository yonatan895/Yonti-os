use yonti_os::font::{CHAR_HEIGHT, CHAR_WIDTH, FALLBACK_INDEX, FONT_BASIC, FONT_OFFSET};
use yonti_os::framebuffer;

#[test_case]
fn test_clear_screen() {
    framebuffer::with_framebuffer_mut(|fb| {
        fb.clear_screen();
    });

    let pixels = framebuffer::with_framebuffer(|fb| {
        let mut samples = [(0u8, 0u8, 0u8); 5];
        let (w, h) = fb.dimensions();
        let positions = [(0, 0), (1, 0), (w / 2, h / 2), (w - 1, 0), (0, h - 1)];
        for (i, &(x, y)) in positions.iter().enumerate() {
            samples[i] = fb.read_pixel(x, y).unwrap_or((0xff, 0xff, 0xff));
        }
        samples
    });

    assert!(pixels.is_some());
    for pixel in pixels.unwrap() {
        assert_eq!(pixel, (0, 0, 0), "pixel should be black after clear");
    }
}

#[test_case]
fn test_char_rendering() {
    let test_char: u8 = b'A';
    let glyph_idx = (test_char - FONT_OFFSET as u8) as usize;
    let glyph_start = glyph_idx * CHAR_HEIGHT;

    framebuffer::with_framebuffer_mut(|fb| {
        fb.clear_screen();
        fb.set_cursor(0, 0);
        fb.write_byte(test_char);
    });

    let result = framebuffer::with_framebuffer(|fb| {
        let (fg_r, fg_g, fg_b) = fb.fg_color();
        let (bg_r, bg_g, bg_b) = fb.bg_color();
        let (w, h) = fb.dimensions();
        if w < CHAR_WIDTH || h < CHAR_HEIGHT {
            return false;
        }

        let mut all_correct = true;
        for row in 0..CHAR_HEIGHT {
            let glyph_byte = FONT_BASIC[glyph_start + row];
            for col in 0..CHAR_WIDTH {
                let expected_fg = glyph_byte & (1 << (7 - col)) != 0;
                let pixel = fb.read_pixel(col, row).unwrap();
                let expected = if expected_fg {
                    (fg_r, fg_g, fg_b)
                } else {
                    (bg_r, bg_g, bg_b)
                };
                if pixel != expected {
                    all_correct = false;
                }
            }
        }
        all_correct
    });

    assert_eq!(result, Some(true), "glyph pixels should match font bitmap");
}

#[test_case]
fn test_cursor_advances() {
    framebuffer::with_framebuffer_mut(|fb| {
        fb.clear_screen();
        fb.set_cursor(0, 0);
        fb.write_byte(b'X');
    });

    let pos = framebuffer::with_framebuffer(|fb| fb.cursor_position());
    assert_eq!(pos, Some((CHAR_WIDTH, 0)));
}

#[test_case]
fn test_newline() {
    framebuffer::with_framebuffer_mut(|fb| {
        fb.clear_screen();
        fb.set_cursor(0, 0);
        fb.write_byte(b'X');
        fb.write_byte(b'\n');
    });

    let pos = framebuffer::with_framebuffer(|fb| fb.cursor_position());
    assert_eq!(pos, Some((0, CHAR_HEIGHT)));
}

#[test_case]
fn test_line_wrap() {
    let (width, _) = framebuffer::with_framebuffer(|fb| fb.dimensions()).unwrap();

    framebuffer::with_framebuffer_mut(|fb| {
        fb.clear_screen();
        fb.set_cursor(0, 0);

        let chars_per_line = width / CHAR_WIDTH;
        for _ in 0..chars_per_line {
            fb.write_byte(b'-');
        }
        fb.write_byte(b'!');
    });

    let pos = framebuffer::with_framebuffer(|fb| fb.cursor_position());
    assert_eq!(
        pos,
        Some((CHAR_WIDTH, CHAR_HEIGHT)),
        "cursor should wrap to next line when reaching end of row"
    );
}

#[test_case]
fn test_scroll() {
    let (_, height) = framebuffer::with_framebuffer(|fb| fb.dimensions()).unwrap();

    framebuffer::with_framebuffer_mut(|fb| {
        fb.clear_screen();
        fb.set_cursor(0, 0);

        let rows = height / CHAR_HEIGHT;
        for _ in 0..=rows {
            fb.write_byte(b'\n');
        }
    });

    let pos = framebuffer::with_framebuffer(|fb| fb.cursor_position());
    let max_y = height - CHAR_HEIGHT;
    assert_eq!(
        pos,
        Some((0, max_y)),
        "cursor should stay within framebuffer bounds after scrolling"
    );
}

#[test_case]
fn test_ansi_color_reset() {
    framebuffer::with_framebuffer_mut(|fb| {
        fb.clear_screen();
        fb.set_cursor(0, 0);
        fb.write_string("\x1b[31mR\x1b[0mN");
    });

    let result = framebuffer::with_framebuffer(|fb| {
        let pixel_r = fb.read_pixel(0, 0).unwrap();
        let pixel_n = fb.read_pixel(CHAR_WIDTH, 0).unwrap();
        (pixel_r, pixel_n)
    });

    let (pixel_r, pixel_n) = result.unwrap();
    assert_eq!(
        pixel_r.0, 170,
        "red glyph: R channel should be 170 (ANSI red)"
    );
    assert_eq!(pixel_r.1, 0, "red glyph: G channel should be 0");
    assert_eq!(pixel_r.2, 0, "red glyph: B channel should be 0");
    assert_eq!(
        pixel_n,
        (170, 170, 170),
        "reset produces light gray fg on black bg"
    );
}

#[test_case]
fn test_fallback_character() {
    framebuffer::with_framebuffer_mut(|fb| {
        fb.clear_screen();
        fb.set_cursor(0, 0);
        fb.write_byte(0x01);
    });

    let result = framebuffer::with_framebuffer(|fb| {
        let (fg_r, fg_g, fg_b) = fb.fg_color();
        let (bg_r, bg_g, bg_b) = fb.bg_color();

        let glyph_start = FALLBACK_INDEX * CHAR_HEIGHT;
        let mut all_correct = true;
        for row in 0..CHAR_HEIGHT {
            let glyph_byte = FONT_BASIC[glyph_start + row];
            for col in 0..CHAR_WIDTH {
                let expected_fg = glyph_byte & (1 << (7 - col)) != 0;
                let pixel = fb.read_pixel(col, row).unwrap();
                let expected = if expected_fg {
                    (fg_r, fg_g, fg_b)
                } else {
                    (bg_r, bg_g, bg_b)
                };
                if pixel != expected {
                    all_correct = false;
                }
            }
        }
        all_correct
    });

    assert_eq!(
        result,
        Some(true),
        "non-printable byte should render as fallback character"
    );
}

#[test_case]
fn test_backspace_cursor() {
    framebuffer::with_framebuffer_mut(|fb| {
        fb.clear_screen();
        fb.set_cursor(0, 0);
        fb.write_byte(b'X');
        fb.write_byte(0x08);
    });

    let pos = framebuffer::with_framebuffer(|fb| fb.cursor_position());
    assert_eq!(pos, Some((0, 0)), "backspace should move cursor left");
}

#[test_case]
fn test_backspace_no_underflow() {
    framebuffer::with_framebuffer_mut(|fb| {
        fb.clear_screen();
        fb.set_cursor(0, 0);
        fb.write_byte(0x08);
    });

    let pos = framebuffer::with_framebuffer(|fb| fb.cursor_position());
    assert_eq!(
        pos,
        Some((0, 0)),
        "backspace at column 0 should not underflow"
    );
}

#[test_case]
fn test_backspace_erase() {
    framebuffer::with_framebuffer_mut(|fb| {
        fb.clear_screen();
        fb.set_cursor(0, 0);
        fb.write_byte(b'X');
        fb.write_byte(0x08);
        fb.write_byte(b' ');
        fb.write_byte(0x08);
    });

    let result = framebuffer::with_framebuffer(|fb| {
        let (_, bg_g, bg_b) = fb.bg_color();
        let pixel = fb.read_pixel(0, 0).unwrap();
        (pixel, bg_g, bg_b)
    });

    let (pixel, bg_g, bg_b) = result.unwrap();
    assert_eq!(
        pixel,
        (0, bg_g, bg_b),
        "backspace-space-backspace should erase character to background"
    );
}
