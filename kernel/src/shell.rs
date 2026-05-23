use crate::fs::{self, FsError};
use crate::monitor;
use crate::trace;
use crate::{print, println};
use alloc::string::String;
use alloc::vec::Vec;
use pc_keyboard::{DecodedKey, HandleControl, PS2Keyboard, ScancodeSet1, layouts};

pub struct Shell {
    buf: [u8; 256],
    pos: usize,
}

impl Shell {
    pub const fn new() -> Self {
        Self {
            buf: [0; 256],
            pos: 0,
        }
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

impl Shell {

    pub fn handle_key(&mut self, key: DecodedKey) {
        match key {
            DecodedKey::Unicode('\n') => {
                println!();
                self.execute();
                self.print_prompt();
            }
            DecodedKey::Unicode('\x08') | DecodedKey::RawKey(pc_keyboard::KeyCode::Backspace) => {
                self.backspace();
            }
            DecodedKey::Unicode(c) if c.is_ascii_graphic() || c == ' ' => {
                self.push(c);
            }
            _ => {}
        }
    }

    pub fn print_prompt(&self) {
        print!("\x1b[32m$ \x1b[0m");
    }

    fn push(&mut self, c: char) {
        if self.pos < self.buf.len() {
            self.buf[self.pos] = c as u8;
            self.pos += 1;
            print!("{}", c);
        }
    }

    fn backspace(&mut self) {
        if self.pos > 0 {
            self.pos -= 1;
            print!("\x08 \x08");
        }
    }

    fn execute(&mut self) {
        let input = core::str::from_utf8(&self.buf[..self.pos]).unwrap_or("");
        let input = input.trim();
        let mut parts = input.split_whitespace();
        let cmd = parts.next().unwrap_or("");

        match cmd {
            "help" => self.cmd_help(),
            "mem" => self.cmd_mem(),
            "trace" => self.cmd_trace(parts.next()),
            "ls" => self.cmd_ls(parts.next()),
            "cat" => self.cmd_cat(parts.next()),
            "alloc" => self.cmd_alloc(parts.next()),
            "uptime" => self.cmd_uptime(),
            "clear" => self.cmd_clear(),
            "echo" => self.cmd_echo(parts.collect::<Vec<_>>().join(" ")),
            "" => {}
            _ => println!("unknown command: {}", cmd),
        }

        self.pos = 0;
    }

    fn cmd_help(&self) {
        println!(
            "commands: help, mem, trace [n], ls <path>, cat <path>, \
             alloc <n>, uptime, clear, echo <text>"
        );
    }

    fn cmd_mem(&self) {
        let m = monitor::snapshot();
        println!(
            "alloc: {} allocs, {} frees, {} bytes in use (peak: {})",
            m.alloc_count, m.free_count, m.current_allocated, m.peak_allocated,
        );
        println!(
            "frames: {}/{}  tasks: spawned={} completed={} active={}",
            m.allocated_frames, m.total_frames, m.tasks_spawned, m.tasks_completed, m.active_tasks,
        );
        println!(
            "dropped wakes: {}  ticks: {}",
            m.dropped_wakes, m.timer_ticks,
        );
        for (i, &count) in m.interrupt_counts.iter().enumerate() {
            if count > 0 {
                println!("  irq 0x{:02x}: {}", 0x20 + i as u8, count);
            }
        }
    }

    fn cmd_trace(&self, n_arg: Option<&str>) {
        let n: usize = n_arg.and_then(|s| s.parse().ok()).unwrap_or(10);
        println!("dumping last {} trace events to serial...", n);
        trace::dump_last(n);
    }

    fn cmd_ls(&self, path: Option<&str>) {
        let path = path.unwrap_or("/");
        let fs = fs::FS.read();
        match fs.list_dir(path) {
            Ok(children) => {
                println!("{} ({} entries):", path, children.len());
                for name in &children {
                    println!("  {}", name);
                }
            }
            Err(FsError::NotADirectory) => {
                // Try reading as a file
                match fs.read_file(path) {
                    Ok(data) => {
                        println!("{} (file, {} bytes)", path, data.len());
                    }
                    Err(_) => println!("ls: {}: not found", path),
                }
            }
            Err(e) => println!("ls: {}: {:?}", path, e),
        }
    }

    fn cmd_cat(&self, path: Option<&str>) {
        let path = match path {
            Some(p) => p,
            None => {
                println!("cat: missing path");
                return;
            }
        };
        let fs = fs::FS.read();
        match fs.read_file(path) {
            Ok(data) => {
                if let Ok(text) = core::str::from_utf8(&data) {
                    print!("{}", text);
                } else {
                    println!("(binary file, {} bytes)", data.len());
                }
            }
            Err(FsError::NotAFile) => println!("cat: {}: is a directory", path),
            Err(e) => println!("cat: {}: {:?}", path, e),
        }
    }

    fn cmd_alloc(&self, size_arg: Option<&str>) {
        let size: usize = match size_arg.and_then(|s| s.parse().ok()) {
            Some(s) if s > 0 => s,
            _ => {
                println!("alloc: invalid size");
                return;
            }
        };

        let layout = core::alloc::Layout::from_size_align(size, 8).unwrap();
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        if ptr.is_null() {
            println!("alloc: allocation of {} bytes failed", size);
        } else {
            println!("alloc: {} bytes at {:#018x}", size, ptr as usize);
            unsafe {
                alloc::alloc::dealloc(ptr, layout);
            }
        }
    }

    fn cmd_uptime(&self) {
        println!("uptime: {} ticks", monitor::uptime_ticks());
    }

    fn cmd_clear(&self) {
        for _ in 0..50 {
            println!();
        }
    }

    fn cmd_echo(&self, text: String) {
        if text.is_empty() {
            println!();
        } else {
            println!("{}", text);
        }
    }
}

pub async fn shell_task() {
    use crate::async_utils::StreamExt;
    use crate::task::keyboard::ScancodeStream;

    let mut scancodes = ScancodeStream::new();
    let mut keyboard = PS2Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );
    let mut shell = Shell::new();
    shell.print_prompt();

    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode)
            && let Some(key) = keyboard.process_keyevent(key_event)
        {
            shell.handle_key(key);
        }
    }
}
