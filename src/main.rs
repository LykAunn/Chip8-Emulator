use std::fs::File;
use std::io::Read;
use std::path::Path;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::TextureCreator;
use sdl2::video::WindowContext;
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::time::{Duration, Instant};

const START_ADDRESS: usize = 0x200;
const FONTSET_START_ADDRESS: usize = 0x50;
const FONTSET_SIZE: usize = 80;
const VIDEO_WIDTH: usize = 64;
const VIDEO_HEIGHT: usize = 32;

type Chip8Fn = fn(&mut Chip8);

fn op_null(_chip8: &mut Chip8) {}

pub struct Chip8 {
    registers: [u8; 16],
    memory: [u8; 4096],
    index: u16,
    pc: u16,
    stack: [u16; 16],
    sp: usize,
    delay_timer: u8,
    sound_timer: u8,
    pub keypad: [u8; 16],
    pub video: [u32; 64 * 32],
    opcode: u16,
    rng: StdRng,

    table:   [Chip8Fn; 0xF + 1],
    table0:  [Chip8Fn; 0xE + 1],
    table8:  [Chip8Fn; 0xE + 1],
    table_e: [Chip8Fn; 0xE + 1],
    table_f: [Chip8Fn; 0x65 + 1],
}

impl Chip8 {
    pub fn new() -> Self {
        let fontset: [u8; FONTSET_SIZE] = [
            0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
            0x20, 0x60, 0x20, 0x20, 0x70, // 1
            0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
            0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
            0x90, 0x90, 0xF0, 0x10, 0x10, // 4
            0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
            0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
            0xF0, 0x10, 0x20, 0x40, 0x40, // 7
            0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
            0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
            0xF0, 0x90, 0xF0, 0x90, 0x90, // A
            0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
            0xF0, 0x80, 0x80, 0x80, 0xF0, // C
            0xE0, 0x90, 0x90, 0x90, 0xE0, // D
            0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
            0xF0, 0x80, 0xF0, 0x80, 0x80, // F
        ];

        let mut memory = [0u8; 4096];
        memory[FONTSET_START_ADDRESS..FONTSET_START_ADDRESS + FONTSET_SIZE]
            .copy_from_slice(&fontset);

        let mut table:   [Chip8Fn; 0xF + 1]  = [op_null; 0xF + 1];
        let mut table0:  [Chip8Fn; 0xE + 1]  = [op_null; 0xE + 1];
        let mut table8:  [Chip8Fn; 0xE + 1]  = [op_null; 0xE + 1];
        let mut table_e: [Chip8Fn; 0xE + 1]  = [op_null; 0xE + 1];
        let mut table_f: [Chip8Fn; 0x65 + 1] = [op_null; 0x65 + 1];

        // Primary table
        table[0x0] = Chip8::table0_dispatch;
        table[0x1] = Chip8::op_1nnn;
        table[0x2] = Chip8::op_2nnn;
        table[0x3] = Chip8::op_3xkk;
        table[0x4] = Chip8::op_4xkk;
        table[0x5] = Chip8::op_5xy0;
        table[0x6] = Chip8::op_6xkk;
        table[0x7] = Chip8::op_7xkk;
        table[0x8] = Chip8::table8_dispatch;
        table[0x9] = Chip8::op_9xy0;
        table[0xA] = Chip8::op_annn;
        table[0xB] = Chip8::op_bnnn;
        table[0xC] = Chip8::op_cxkk;
        table[0xD] = Chip8::op_dxyn;
        table[0xE] = Chip8::table_e_dispatch;
        table[0xF] = Chip8::table_f_dispatch;

        // $0 sub-table
        table0[0x0] = Chip8::op_00e0;
        table0[0xE] = Chip8::op_00ee;

        // $8 sub-table
        table8[0x0] = Chip8::op_8xy0;
        table8[0x1] = Chip8::op_8xy1;
        table8[0x2] = Chip8::op_8xy2;
        table8[0x3] = Chip8::op_8xy3;
        table8[0x4] = Chip8::op_8xy4;
        table8[0x5] = Chip8::op_8xy5;
        table8[0x6] = Chip8::op_8xy6;
        table8[0x7] = Chip8::op_8xy7;
        table8[0xE] = Chip8::op_8xye;

        // $E sub-table — indexed by last nibble: 9E -> 0xE, A1 -> 0x1
        table_e[0xE] = Chip8::op_ex9e;
        table_e[0x1] = Chip8::op_exa1;

        // $F sub-table — indexed by last byte
        table_f[0x07] = Chip8::op_fx07;
        table_f[0x0A] = Chip8::op_fx0a;
        table_f[0x15] = Chip8::op_fx15;
        table_f[0x18] = Chip8::op_fx18;
        table_f[0x1E] = Chip8::op_fx1e;
        table_f[0x29] = Chip8::op_fx29;
        table_f[0x33] = Chip8::op_fx33;
        table_f[0x55] = Chip8::op_fx55;
        table_f[0x65] = Chip8::op_fx65;

        Self {
            registers: [0; 16],
            memory,
            index: 0,
            pc: START_ADDRESS as u16,
            stack: [0; 16],
            sp: 0,
            delay_timer: 0,
            sound_timer: 0,
            keypad: [0; 16],
            video: [0; 64 * 32],
            opcode: 0,
            rng: StdRng::from_entropy(),
            table,
            table0,
            table8,
            table_e,
            table_f,
        }
    }

    pub fn cycle(&mut self) {
        // Fetch
        self.opcode = (self.memory[self.pc as usize] as u16) << 8
                    | self.memory[self.pc as usize + 1] as u16;
        self.pc += 2;

        // Decode and execute via primary table
        let index = ((self.opcode & 0xF000) >> 12) as usize;
        let handler = self.table[index];
        handler(self);

        if self.delay_timer > 0 { self.delay_timer -= 1; }
        if self.sound_timer > 0 { self.sound_timer -= 1; }
    }

    // --- Dispatch functions ---

    fn table0_dispatch(chip8: &mut Chip8) {
        let index = (chip8.opcode & 0x000F) as usize;
        let handler = chip8.table0[index];
        handler(chip8);
    }

    fn table8_dispatch(chip8: &mut Chip8) {
        let index = (chip8.opcode & 0x000F) as usize; 
        let handler = chip8.table8[index];
        handler(chip8);
    }

    fn table_e_dispatch(chip8: &mut Chip8) {
        let index = (chip8.opcode & 0x000F) as usize;
        let handler = chip8.table_e[index];
        handler(chip8);
    }

    fn table_f_dispatch(chip8: &mut Chip8) {
        let index = (chip8.opcode & 0x00FF) as usize;
        let handler = chip8.table_f[index];
        handler(chip8);
    }

    // --- ROM loader ---

    pub fn load_rom<P: AsRef<Path>>(&mut self, filename: P) {
        let mut file = File::open(filename).expect("Failed to open ROM file");
        let metadata = file.metadata().expect("Failed to read file metadata");
        let contents_length = metadata.len() as usize;
        file.read_exact(&mut self.memory[START_ADDRESS..START_ADDRESS + contents_length])
            .expect("Failed to read ROM into memory");
        println!("ROM loaded successfully! Size: {} bytes", contents_length);
    }

    // --- Opcode implementations ---

    fn op_00e0(chip8: &mut Chip8) {
        chip8.video.fill(0);
    }

    fn op_00ee(chip8: &mut Chip8) {
        chip8.sp -= 1;
        chip8.pc = chip8.stack[chip8.sp];
    }

    fn op_1nnn(chip8: &mut Chip8) {
        chip8.pc = chip8.opcode & 0x0FFF;
    }

    fn op_2nnn(chip8: &mut Chip8) {
        chip8.stack[chip8.sp] = chip8.pc;
        chip8.sp += 1;
        chip8.pc = chip8.opcode & 0x0FFF;
    }

    fn op_3xkk(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let byte = (chip8.opcode & 0x00FF) as u8;         
        if chip8.registers[vx] == byte { chip8.pc += 2; } 
    }

    fn op_4xkk(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let byte = (chip8.opcode & 0x00FF) as u8;
        if chip8.registers[vx] != byte { chip8.pc += 2; }
    }

    fn op_5xy0(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let vy = ((chip8.opcode & 0x00F0) >> 4) as usize;
        if chip8.registers[vx] == chip8.registers[vy] { chip8.pc += 2; }
    }

    fn op_6xkk(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let byte = (chip8.opcode & 0x00FF) as u8;
        chip8.registers[vx] = byte;
    }

    fn op_7xkk(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let byte = (chip8.opcode & 0x00FF) as u8;
        chip8.registers[vx] = chip8.registers[vx].wrapping_add(byte);
    }

    fn op_8xy0(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let vy = ((chip8.opcode & 0x00F0) >> 4) as usize;
        chip8.registers[vx] = chip8.registers[vy];
    }

    fn op_8xy1(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let vy = ((chip8.opcode & 0x00F0) >> 4) as usize;
        chip8.registers[vx] |= chip8.registers[vy];
    }

    fn op_8xy2(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let vy = ((chip8.opcode & 0x00F0) >> 4) as usize;
        chip8.registers[vx] &= chip8.registers[vy]; 
    }

    fn op_8xy3(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let vy = ((chip8.opcode & 0x00F0) >> 4) as usize;
        chip8.registers[vx] ^= chip8.registers[vy];
    }

    fn op_8xy4(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let vy = ((chip8.opcode & 0x00F0) >> 4) as usize;
        let sum = chip8.registers[vx] as u16 + chip8.registers[vy] as u16;
        chip8.registers[0xF] = if sum > 255 { 1 } else { 0 };
        chip8.registers[vx] = (sum & 0xFF) as u8;
    }

    fn op_8xy5(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let vy = ((chip8.opcode & 0x00F0) >> 4) as usize;
        chip8.registers[0xF] = if chip8.registers[vx] > chip8.registers[vy] { 1 } else { 0 };
        chip8.registers[vx] = chip8.registers[vx].wrapping_sub(chip8.registers[vy]);
    }

    fn op_8xy6(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        chip8.registers[0xF] = chip8.registers[vx] & 0x1;
        chip8.registers[vx] >>= 1;
    }

    fn op_8xy7(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let vy = ((chip8.opcode & 0x00F0) >> 4) as usize;
        chip8.registers[0xF] = if chip8.registers[vy] > chip8.registers[vx] { 1 } else { 0 };
        chip8.registers[vx] = chip8.registers[vy].wrapping_sub(chip8.registers[vx]);
    }

    fn op_8xye(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        chip8.registers[0xF] = (chip8.registers[vx] & 0x80) >> 7;
        chip8.registers[vx] <<= 1;
    }

    fn op_9xy0(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let vy = ((chip8.opcode & 0x00F0) >> 4) as usize;
        if chip8.registers[vx] != chip8.registers[vy] { chip8.pc += 2; }
    }

    fn op_annn(chip8: &mut Chip8) {
        chip8.index = chip8.opcode & 0x0FFF;
    }

    fn op_bnnn(chip8: &mut Chip8) {
        chip8.pc = chip8.registers[0] as u16 + (chip8.opcode & 0x0FFF);
    }

    fn op_cxkk(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let byte = (chip8.opcode & 0x00FF) as u8; 
        let rand_byte : u8 = chip8.rng.r#gen();
        chip8.registers[vx] = rand_byte & byte;  
    }

    fn op_dxyn(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let vy = ((chip8.opcode & 0x00F0) >> 4) as usize;
        let height = (chip8.opcode & 0x000F) as usize;
        let x_pos = (chip8.registers[vx] % 64) as usize;
        let y_pos = (chip8.registers[vy] % 32) as usize;

        chip8.registers[0xF] = 0;

        for row in 0..height {
            let sprite_byte = chip8.memory[chip8.index as usize + row];
            for col in 0..8usize {
                let sprite_pixel = sprite_byte & (0x80u8 >> col);
                let screen_index = ((y_pos + row) % 32) * 64 + ((x_pos + col) % 64);
                let screen_pixel = &mut chip8.video[screen_index];
                if sprite_pixel != 0 {
                    if *screen_pixel == 0xFFFFFFFF { chip8.registers[0xF] = 1; }
                    *screen_pixel ^= 0xFFFFFFFF;
                }
            }
        }
    }

    fn op_ex9e(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let key = chip8.registers[vx] as usize;
        if chip8.keypad[key] != 0 { chip8.pc += 2; }
    }

    fn op_exa1(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let key = chip8.registers[vx] as usize;
        if chip8.keypad[key] == 0 { chip8.pc += 2; }
    }

    fn op_fx07(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        chip8.registers[vx] = chip8.delay_timer;
    }

    fn op_fx0a(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        if let Some(key_index) = chip8.keypad.iter().position(|&k| k != 0) {
            chip8.registers[vx] = key_index as u8;
        } else {
            chip8.pc = chip8.pc.saturating_sub(2);
        }
    }

    fn op_fx15(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        chip8.delay_timer = chip8.registers[vx];
    }

    fn op_fx18(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        chip8.sound_timer = chip8.registers[vx];
    }

    fn op_fx1e(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        chip8.index += chip8.registers[vx] as u16;
    }

    fn op_fx29(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let digit = chip8.registers[vx] as u16;
        chip8.index = FONTSET_START_ADDRESS as u16 + 5 * digit;
    }

    fn op_fx33(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        let mut value = chip8.registers[vx];
        chip8.memory[chip8.index as usize + 2] = value % 10; value /= 10;
        chip8.memory[chip8.index as usize + 1] = value % 10; value /= 10;
        chip8.memory[chip8.index as usize]     = value % 10;
    }

    fn op_fx55(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        for i in 0..=vx {
            chip8.memory[chip8.index as usize + i] = chip8.registers[i];
        }
    }

    fn op_fx65(chip8: &mut Chip8) {
        let vx = ((chip8.opcode & 0x0F00) >> 8) as usize;
        for i in 0..=vx {
            chip8.registers[i] = chip8.memory[chip8.index as usize + i];
        }
    }
}

// =============================================================================
// Platform (SDL2 window, renderer, input)
// =============================================================================

pub struct Platform {
    canvas: sdl2::render::Canvas<sdl2::video::Window>,
    texture_creator: TextureCreator<WindowContext>,
    event_pump: sdl2::EventPump,
    texture_width: u32,
    texture_height: u32,
}

impl Platform {
    pub fn new(
        title: &str,
        window_width: u32,
        window_height: u32,
        texture_width: u32,
        texture_height: u32,
    ) -> Self {
        let sdl_context = sdl2::init().unwrap();
        let video = sdl_context.video().unwrap();

        let window = video
            .window(title, window_width, window_height)
            .position_centered()
            .build()
            .unwrap();

        let canvas = window.into_canvas().accelerated().build().unwrap();
        let texture_creator = canvas.texture_creator();
        let event_pump = sdl_context.event_pump().unwrap();

        Self { canvas, texture_creator, event_pump, texture_width, texture_height }
    }

    pub fn update(&mut self, buffer: &[u32]) {
        let mut texture = self
            .texture_creator
            .create_texture_streaming(PixelFormatEnum::RGBA8888, self.texture_width, self.texture_height)
            .unwrap();

        texture.with_lock(None, |pixels: &mut [u8], _pitch| {
            let src = unsafe {
                std::slice::from_raw_parts(
                    buffer.as_ptr() as *const u8,
                    buffer.len() * std::mem::size_of::<u32>(),
                )
            };
            pixels.copy_from_slice(src);
        }).unwrap();

        self.canvas.clear();
        self.canvas.copy(&texture, None, None).unwrap();
        self.canvas.present();
    }

    pub fn process_input(&mut self, keys: &mut [u8; 16]) -> bool {
        let mut quit = false;

        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => quit = true,

                Event::KeyDown { keycode: Some(key), .. } => match key {
                    Keycode::Escape => quit = true,
                    Keycode::X     => keys[0x0] = 1,
                    Keycode::Num1  => keys[0x1] = 1,
                    Keycode::Num2  => keys[0x2] = 1,
                    Keycode::Num3  => keys[0x3] = 1,
                    Keycode::Q     => keys[0x4] = 1,
                    Keycode::W     => keys[0x5] = 1,
                    Keycode::E     => keys[0x6] = 1,
                    Keycode::A     => keys[0x7] = 1,
                    Keycode::S     => keys[0x8] = 1,
                    Keycode::D     => keys[0x9] = 1,
                    Keycode::Z     => keys[0xA] = 1,
                    Keycode::C     => keys[0xB] = 1,
                    Keycode::Num4  => keys[0xC] = 1,
                    Keycode::R     => keys[0xD] = 1,
                    Keycode::F     => keys[0xE] = 1,
                    Keycode::V     => keys[0xF] = 1,
                    _ => {}
                },

                Event::KeyUp { keycode: Some(key), .. } => match key {
                    Keycode::X     => keys[0x0] = 0,
                    Keycode::Num1  => keys[0x1] = 0,
                    Keycode::Num2  => keys[0x2] = 0,
                    Keycode::Num3  => keys[0x3] = 0,
                    Keycode::Q     => keys[0x4] = 0,
                    Keycode::W     => keys[0x5] = 0,
                    Keycode::E     => keys[0x6] = 0,
                    Keycode::A     => keys[0x7] = 0,
                    Keycode::S     => keys[0x8] = 0,
                    Keycode::D     => keys[0x9] = 0,
                    Keycode::Z     => keys[0xA] = 0,
                    Keycode::C     => keys[0xB] = 0,
                    Keycode::Num4  => keys[0xC] = 0,
                    Keycode::R     => keys[0xD] = 0,
                    Keycode::F     => keys[0xE] = 0,
                    Keycode::V     => keys[0xF] = 0,
                    _ => {}
                },

                _ => {}
            }
        }

        quit
    }
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rom_path = args.get(1).expect("Usage: chip8 <rom>");

    let scale = 10u32;
    let cycles_per_frame = 1;
    let frame_duration = Duration::from_nanos(1_000_000_000 / 60);

    let mut chip8 = Chip8::new();
    chip8.load_rom(rom_path);

    let mut platform = Platform::new(
        "CHIP-8",
        VIDEO_WIDTH as u32 * scale,
        VIDEO_HEIGHT as u32 * scale,
        VIDEO_WIDTH as u32,
        VIDEO_HEIGHT as u32,
    );

    loop {
        let frame_start = Instant::now();

        if platform.process_input(&mut chip8.keypad) {
            break;
        }

        for _ in 0..cycles_per_frame {
            chip8.cycle();
        }

        platform.update(&chip8.video);

        let elapsed = frame_start.elapsed();
        // if elapsed < frame_duration {
        //     std::thread::sleep(frame_duration - elapsed);
        // }
    }
}
