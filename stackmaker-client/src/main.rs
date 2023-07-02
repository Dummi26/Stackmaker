use std::{
    any::Any,
    collections::HashMap,
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::Instant,
};

use image::{imageops, RgbaImage};
use speedy2d::{
    color::Color,
    dimen::{UVec2, Vec2},
    font::{Font, FormattedTextBlock, TextLayout, TextOptions},
    image::{ImageDataType, ImageHandle, ImageSmoothingMode},
    shape::Rectangle,
    window::{
        MouseButton, MouseScrollDistance, UserEventSender, WindowCreationOptions, WindowHandler,
        WindowHelper,
    },
    Graphics2D,
};
use stackmaker::{
    runner::{self, Runner},
    world::{Block, World},
};

fn main() {
    let window = speedy2d::Window::new_with_user_events(
        "Stackmaker - Starting",
        WindowCreationOptions::new_fullscreen_borderless(),
    )
    .unwrap();
    let ue_sender = window.create_user_event_sender();
    window.run_loop(Window::new(ue_sender));
}

impl Window {
    pub fn new(user_event_sender: UserEventSender<Event>) -> Self {
        let loader = ThreadedLoading::new(user_event_sender).unwrap();
        Self {
            thread_loading: Some(loader),
            events: vec![],
            font_monospace: None,
            font_main: None,
            size: UVec2::ZERO,
            mouse_pos: Vec2::new(0.0, 0.0),
            mouse_down_l: false,
            mouse_down_m: false,
            mouse_down_r: false,
            redraw: true,
            state: WindowState::MainMenu(WSMainMenu {
                singleplayer_world_box: Rectangle::new(Vec2::new(0.1, 0.4), Vec2::new(0.3, 0.9)),
                singleplayer_new_world_button: Rectangle::new(
                    Vec2::new(0.7, 0.4),
                    Vec2::new(0.9, 0.5),
                ),
                singleplayer_new_world_button_brightness: 0.0,
                title_text: None,
                desired_world_height: 0.0,
                worlds_texts: vec![],
                world_scroll: 0,
            }),
            saves: vec![],
            images: Default::default(),
        }
    }
}

struct Window {
    thread_loading: Option<ThreadedLoading>,
    events: Vec<Event>,
    size: UVec2,
    mouse_pos: Vec2,
    mouse_down_l: bool,
    mouse_down_m: bool,
    mouse_down_r: bool,
    /// if true, we need a full redraw (state changed, window resized, etc.)
    redraw: bool,

    font_monospace: Option<Font>,
    font_main: Option<Font>,

    state: WindowState,

    saves: Vec<(PathBuf, String)>,

    images: WindowImages,
}
#[derive(Default)]
struct WindowImages {
    main_menu_background_image: LoadableImage,
    main_menu_singleplayer_new_world_image: LoadableImage,
    world_block_color: LoadableImage,
    world_block_char: LoadableImage,
    world_block_delay: [LoadableImage; 6],
    world_block_storage_sto: [LoadableImage; 6],
    world_block_storage_or: [LoadableImage; 6],
    world_block_storage_and: [LoadableImage; 6],
    world_block_storage_xor: [LoadableImage; 6],
    world_block_storage_add: [LoadableImage; 6],
    world_block_storage_sub: [LoadableImage; 6],
    world_block_storage_mul: [LoadableImage; 6],
    world_block_storage_div: [LoadableImage; 6],
    world_block_storage_mod: [LoadableImage; 6],
    world_block_storage_default: [LoadableImage; 6],
    world_block_gate_open: [LoadableImage; 6],
    world_block_gate_closed: [LoadableImage; 6],
    world_block_move: [LoadableImage; 6],
    world_block_swap: [LoadableImage; 6],
}

enum Event {
    LoadFontMain(Vec<u8>),
    LoadFontMono(Vec<u8>),
    AddWorld(PathBuf, String),
    SetMainMenuBackgroundImage(RgbaImage),
    SetMainMenuSingleplayerNewWorldImage(RgbaImage),
    SetWorldBlockColor(RgbaImage),
    SetWorldBlockChar(RgbaImage),
    SetWorldBlockDelay([Option<RgbaImage>; 6]),
    SetWorldBlockStorageSto([Option<RgbaImage>; 6]),
    SetWorldBlockStorageOr([Option<RgbaImage>; 6]),
    SetWorldBlockStorageAnd([Option<RgbaImage>; 6]),
    SetWorldBlockStorageXor([Option<RgbaImage>; 6]),
    SetWorldBlockStorageAdd([Option<RgbaImage>; 6]),
    SetWorldBlockStorageSub([Option<RgbaImage>; 6]),
    SetWorldBlockStorageMul([Option<RgbaImage>; 6]),
    SetWorldBlockStorageDiv([Option<RgbaImage>; 6]),
    SetWorldBlockStorageMod([Option<RgbaImage>; 6]),
    SetWorldBlockStorageDefault([Option<RgbaImage>; 6]),
    SetWorldBlockGateOpen([Option<RgbaImage>; 6]),
    SetWorldBlockGateClosed([Option<RgbaImage>; 6]),
    SetWorldBlockMove([Option<RgbaImage>; 6]),
    SetWorldBlockSwap([Option<RgbaImage>; 6]),
}

enum WindowState {
    Nothing,
    MainMenu(WSMainMenu),
    LoadingWorld(Arc<Mutex<f32>>, Option<JoinHandle<Runner>>),
    Singleplayer(WSInGame, Runner),
}
impl WindowState {
    fn take(&mut self) -> Self {
        std::mem::replace(self, Self::Nothing)
    }
    fn setnew(&mut self, state: Self) {
        if let Self::Nothing = self {
            *self = state;
        }
    }
}

impl WindowHandler<Event> for Window {
    fn on_draw(
        &mut self,
        helper: &mut speedy2d::window::WindowHelper<Event>,
        graphics: &mut speedy2d::Graphics2D,
    ) {
        let start = Instant::now();
        let redraw = std::mem::replace(&mut self.redraw, false);
        // handle loading thread
        if let Some(loading) = &self.thread_loading {
            if loading.thread.is_finished() {
                _ = self
                    .thread_loading
                    .take()
                    .unwrap()
                    .thread
                    .join()
                    .unwrap()
                    .unwrap();
            }
        }
        // handle events
        if !self.events.is_empty() {
            for user_event in std::mem::replace(&mut self.events, vec![]) {
                match user_event {
                    Event::LoadFontMain(bytes) => {
                        self.font_main = match Font::new(&bytes) {
                            Ok(v) => Some(v),
                            Err(e) => {
                                eprintln!("Can't load main font from bytes: {e:?}");
                                None
                            }
                        };
                    }
                    Event::LoadFontMono(bytes) => {
                        self.font_monospace = match Font::new(&bytes) {
                            Ok(v) => Some(v),
                            Err(e) => {
                                eprintln!("Can't load monospace font from bytes: {e:?}");
                                None
                            }
                        };
                    }
                    Event::AddWorld(path, name) => {
                        self.saves.push((path, name));
                        match &mut self.state {
                            WindowState::MainMenu(state) => {
                                state.worlds_texts.push(None);
                                helper.request_redraw();
                            }
                            _ => (),
                        }
                    }
                    Event::SetMainMenuBackgroundImage(img) => {
                        Self::load_img(&mut self.images.main_menu_background_image, img, graphics);
                    }
                    Event::SetMainMenuSingleplayerNewWorldImage(img) => {
                        Self::load_img(
                            &mut self.images.main_menu_singleplayer_new_world_image,
                            img,
                            graphics,
                        );
                    }
                    Event::SetWorldBlockColor(img) => {
                        Self::load_img(&mut self.images.world_block_color, img, graphics);
                    }
                    Event::SetWorldBlockChar(img) => {
                        Self::load_img(&mut self.images.world_block_char, img, graphics);
                    }
                    Event::SetWorldBlockDelay(img) => {
                        Self::load_imgs(&mut self.images.world_block_delay, img, graphics);
                    }
                    Event::SetWorldBlockStorageSto(img) => {
                        Self::load_imgs(&mut self.images.world_block_storage_sto, img, graphics);
                    }
                    Event::SetWorldBlockStorageOr(img) => {
                        Self::load_imgs(&mut self.images.world_block_storage_or, img, graphics);
                    }
                    Event::SetWorldBlockStorageAnd(img) => {
                        Self::load_imgs(&mut self.images.world_block_storage_and, img, graphics);
                    }
                    Event::SetWorldBlockStorageXor(img) => {
                        Self::load_imgs(&mut self.images.world_block_storage_xor, img, graphics);
                    }
                    Event::SetWorldBlockStorageAdd(img) => {
                        Self::load_imgs(&mut self.images.world_block_storage_add, img, graphics);
                    }
                    Event::SetWorldBlockStorageSub(img) => {
                        Self::load_imgs(&mut self.images.world_block_storage_sub, img, graphics);
                    }
                    Event::SetWorldBlockStorageMul(img) => {
                        Self::load_imgs(&mut self.images.world_block_storage_mul, img, graphics);
                    }
                    Event::SetWorldBlockStorageDiv(img) => {
                        Self::load_imgs(&mut self.images.world_block_storage_div, img, graphics);
                    }
                    Event::SetWorldBlockStorageMod(img) => {
                        Self::load_imgs(&mut self.images.world_block_storage_mod, img, graphics);
                    }
                    Event::SetWorldBlockStorageDefault(img) => {
                        Self::load_imgs(
                            &mut self.images.world_block_storage_default,
                            img,
                            graphics,
                        );
                    }
                    Event::SetWorldBlockGateOpen(img) => {
                        Self::load_imgs(&mut self.images.world_block_gate_open, img, graphics);
                    }
                    Event::SetWorldBlockGateClosed(img) => {
                        Self::load_imgs(&mut self.images.world_block_gate_closed, img, graphics);
                    }
                    Event::SetWorldBlockMove(img) => {
                        Self::load_imgs(&mut self.images.world_block_move, img, graphics);
                    }
                    Event::SetWorldBlockSwap(img) => {
                        Self::load_imgs(&mut self.images.world_block_swap, img, graphics);
                    }
                }
            }
        }
        // draw
        let mut state = self.state.take();
        match &mut state {
            WindowState::Nothing => {}
            WindowState::MainMenu(state) => {
                // handle redraws
                if redraw {
                    // perform text layout again
                    state.title_text = None;
                }
                // draw background
                if self.images.main_menu_background_image.loaded() {
                    self.images
                        .main_menu_background_image
                        .draw_image_aspect_ratio_tinted(
                            graphics,
                            helper,
                            Rectangle::new(Vec2::ZERO, self.size.into_f32()),
                            Color::WHITE,
                            true,
                        );
                } else {
                    graphics.clear_screen(Color::BLACK);
                }
                // draw title text
                if state.title_text.is_none() {
                    if let Some(title_font) = &self.font_monospace {
                        let title = "stackmaker";
                        let text_layout =
                            title_font.layout_text(title, 1.0, TextOptions::default());
                        state.title_text = Some(
                            title_font.layout_text(
                                title,
                                (self.size.x as f32 * 0.6 / text_layout.width())
                                    .min(self.size.y as f32 * 0.2 / text_layout.height()),
                                TextOptions::default(),
                            ),
                        );
                    }
                }
                if let Some(text) = &state.title_text {
                    graphics.draw_text(
                        Vec2::new(self.size.x as f32 * 0.2, self.size.y as f32 * 0.1),
                        Color::RED,
                        text,
                    );
                }
                // draw saves list
                if redraw || state.worlds_texts.len() != self.saves.len() {
                    if let Some(font) = &self.font_main {
                        let scale = {
                            let layout = font.layout_text(
                                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
                                1.0,
                                TextOptions::default(),
                            );
                            state.desired_world_height =
                                48.0 * (self.size.y as f32 / 1080.0).sqrt();
                            state.desired_world_height / layout.height()
                        };
                        state.worlds_texts = vec![None; self.saves.len()];
                        for (i, save) in self.saves.iter().enumerate() {
                            state.worlds_texts[i] =
                                Some(font.layout_text(&save.1, scale, TextOptions::default()));
                        }
                    }
                }
                let area = Rectangle::new(
                    Vec2::new(
                        self.size.x as f32 * state.singleplayer_world_box.top_left().x,
                        self.size.y as f32 * state.singleplayer_world_box.top_left().y,
                    ),
                    Vec2::new(
                        self.size.x as f32 * state.singleplayer_world_box.bottom_right().x,
                        self.size.y as f32 * state.singleplayer_world_box.bottom_right().y,
                    ),
                );
                let mouse_in_box = area.contains(self.mouse_pos);
                graphics.set_clip(Some(Rectangle::new(
                    area.top_left().into_i32(),
                    area.bottom_right().into_i32(),
                )));
                let mut height = self.size.y as f32 * 0.4;
                for text in state.worlds_texts.iter().skip(state.world_scroll) {
                    let new_height = height + state.desired_world_height;
                    if let Some(text) = text {
                        graphics.draw_text(
                            Vec2::new(area.top_left().x, height),
                            if mouse_in_box
                                && self.mouse_pos.y >= height
                                && self.mouse_pos.y < new_height
                            {
                                Color::WHITE
                            } else {
                                Color::LIGHT_GRAY
                            },
                            text,
                        );
                    }
                    height = new_height;
                    if height >= area.bottom_right().y {
                        break;
                    }
                }
                graphics.set_clip(None);
                // draw singleplayer new world button
                let area = Rectangle::new(
                    Vec2::new(
                        self.size.x as f32 * state.singleplayer_new_world_button.top_left().x,
                        self.size.y as f32 * state.singleplayer_new_world_button.top_left().y,
                    ),
                    Vec2::new(
                        self.size.x as f32 * state.singleplayer_new_world_button.bottom_right().x,
                        self.size.y as f32 * state.singleplayer_new_world_button.bottom_right().y,
                    ),
                );
                // new singleplayer world button
                if area.contains(self.mouse_pos) {
                    state.singleplayer_new_world_button_brightness =
                        state.singleplayer_new_world_button_brightness * 0.8 + 0.2;
                    if state.singleplayer_new_world_button_brightness < 0.998 {
                        helper.request_redraw();
                    }
                } else {
                    state.singleplayer_new_world_button_brightness =
                        state.singleplayer_new_world_button_brightness * 0.8 + 0.2 * 0.7;
                    if state.singleplayer_new_world_button_brightness > 0.702 {
                        helper.request_redraw();
                    }
                }
                self.images
                    .main_menu_singleplayer_new_world_image
                    .draw_image_aspect_ratio_tinted(
                        graphics,
                        helper,
                        area,
                        Color::from_gray(state.singleplayer_new_world_button_brightness),
                        false,
                    );
                //
            }
            WindowState::LoadingWorld(prog, handle) => {
                if handle.as_ref().unwrap().is_finished() {
                    self.state = WindowState::Singleplayer(
                        WSInGame::default(),
                        handle.take().unwrap().join().unwrap(),
                    );
                    self.redraw = true;
                } else {
                    if redraw {
                        graphics.clear_screen(Color::BLACK);
                    }
                    let lock = prog.lock().unwrap();
                    let prog = *lock;
                    drop(lock);
                    let top = self.size.y as f32 * 0.45;
                    let bottom = self.size.y as f32 * 0.55;
                    let left = self.size.x as f32 * 0.25;
                    let right = self.size.x as f32 * 0.75;
                    let mid = left + (right - left) * prog;
                    graphics.draw_rectangle(
                        Rectangle::new(Vec2::new(left, top), Vec2::new(mid, bottom)),
                        Color::from_int_rgb(0, 40, 100),
                    );
                    graphics.draw_rectangle(
                        Rectangle::new(Vec2::new(mid, top), Vec2::new(right, bottom)),
                        Color::from_int_rgb(50, 0, 20),
                    );
                }
            }
            WindowState::Singleplayer(state, runner) => {
                graphics.clear_screen(Color::BLACK);
                // draw the blocks
                state.pixels_per_block = 2.0f32.powf(state.zoom);
                let pixels_per_block = state.pixels_per_block;
                let top_left_x =
                    state.position.x + 0.5 - self.size.x as f32 / pixels_per_block / 2.0;
                let top_left_y =
                    state.position.y + 0.5 - self.size.y as f32 / pixels_per_block / 2.0;
                let px_x_start = (top_left_x.floor() - top_left_x) * pixels_per_block;
                let mut px_x = px_x_start;
                let mut px_y = (top_left_y.floor() - top_left_y) * pixels_per_block;
                let block_x_start = top_left_x.floor() as _;
                let mut block_x = block_x_start;
                let mut block_y = top_left_y.floor() as _;
                let width = self.size.x as f32;
                let height = self.size.y as f32;
                loop {
                    if px_y >= height {
                        break;
                    }
                    if px_x >= width {
                        px_x = px_x_start;
                        block_x = block_x_start;
                        px_y += pixels_per_block;
                        block_y += 1;
                        continue;
                    }
                    let area = Rectangle::new(
                        Vec2::new(px_x, px_y),
                        Vec2::new(px_x + pixels_per_block, px_y + pixels_per_block),
                    );
                    // TODO: load chunk once for all 256 (or at least 16) blocks
                    let (chunk, block) =
                        runner.world.layers[state.layer].get_where(block_x, block_y);
                    if let Some(chunk) = runner.world.layers[state.layer].get(&chunk) {
                        if let Some(topmost_block) = chunk[block as usize].last() {
                            self.draw_block(graphics, area, topmost_block);
                        }
                    }
                    px_x += pixels_per_block;
                    block_x += 1;
                }
                // draw the menu, if there is one
                'draw_menu: {
                    if let Some((pos, menu)) = &mut state.open_menu {
                        match menu {
                            WSInGameMenu::BlockStackChanger { changing, block } => {
                                let (left, right) = if let Some((closing, since_when)) = changing {
                                    let prog = since_when.elapsed().as_secs_f32();
                                    if prog >= 0.3 {
                                        if *closing {
                                            state.open_menu = None;
                                            break 'draw_menu;
                                        } else {
                                            *changing = None;
                                            (0.0, 0.3)
                                        }
                                    } else if *closing {
                                        (-prog, 0.3 - prog)
                                    } else {
                                        (-0.3 + prog, prog)
                                    }
                                } else {
                                    (0.0, 0.3)
                                };
                                let area = Rectangle::new(
                                    Vec2::new(self.size.x as f32 * left, self.size.y as f32 * 0.05),
                                    Vec2::new(self.size.x as f32 * right, self.size.y as f32 * 0.9),
                                );
                                graphics.draw_rectangle(area, Color::DARK_GRAY);
                                let (chunk, inchunk) =
                                    runner.world.layers[state.layer].get_where(block.0, block.1);
                                let chunk = runner.world.layers[state.layer].get_mut(&chunk);
                                let block = &mut chunk[inchunk as usize];
                                eprintln!("Blocks:");
                                for block in block.iter().rev() {
                                    eprintln!("- {}", block.type_name());
                                }
                            }
                        }
                    }
                }
                helper.request_redraw();
            }
        }
        self.state.setnew(state);
        // eprintln!("Drawing took {}ms", start.elapsed().as_millis());
    }
    fn on_mouse_button_down(&mut self, helper: &mut WindowHelper<Event>, button: MouseButton) {
        match button {
            MouseButton::Left => self.mouse_down_l = true,
            MouseButton::Middle => self.mouse_down_m = true,
            MouseButton::Right => self.mouse_down_r = true,
            MouseButton::Other(..) => {}
        }
    }
    fn on_mouse_button_up(&mut self, helper: &mut WindowHelper<Event>, button: MouseButton) {
        match button {
            MouseButton::Left => self.mouse_down_l = false,
            MouseButton::Middle => self.mouse_down_m = false,
            MouseButton::Right => self.mouse_down_r = false,
            MouseButton::Other(..) => {}
        }
        let mut state = self.state.take();
        match button {
            MouseButton::Left => match &mut state {
                WindowState::Nothing => {}
                WindowState::MainMenu(state) => {
                    let singleplayer_world_box =
                        Self::rel_to_abs_rect(self.size, &state.singleplayer_world_box);
                    if singleplayer_world_box.contains(self.mouse_pos) {
                        let height = (self.mouse_pos.y - singleplayer_world_box.top_left().y)
                            / state.desired_world_height;
                        let index = state.world_scroll + height.floor() as usize;
                        if let Some(save) = self.saves.get(index) {
                            eprintln!("Loading save {save:?}");
                        }
                    } else {
                        let singleplayer_new_world_button =
                            Self::rel_to_abs_rect(self.size, &state.singleplayer_new_world_button);
                        if singleplayer_new_world_button.contains(self.mouse_pos) {
                            eprintln!("Setting up empty world...");
                            let mut world = World::new_empty();
                            eprintln!("Adding some blocks for testing...");
                            {
                                let chunk = world.layers[0].get_mut(&0);
                                let dirs = [
                                    runner::DIR_UP,
                                    runner::DIR_RIGHT,
                                    runner::DIR_DOWN,
                                    runner::DIR_LEFT,
                                    runner::DIR_UP_L,
                                    runner::DIR_DOWN_L,
                                ];
                                chunk[0].push(Block::Char(b'A' as _));
                                for (i, dir) in dirs.iter().enumerate() {
                                    chunk[16 * 0 + 4 + i].push(Block::Delay(0, *dir));
                                }
                                for mode in 0..=9u8 {
                                    for (i, dir) in dirs.iter().enumerate() {
                                        chunk[16 * (1 + mode as usize) + 4 + i]
                                            .push(Block::Storage(0, mode, *dir));
                                    }
                                }
                                for (i, dir) in dirs.iter().enumerate() {
                                    chunk[16 * 14 + 4 + i].push(Block::Gate(false, *dir));
                                }
                                for (i, dir) in dirs.iter().enumerate() {
                                    chunk[16 * 14 + 10 + i].push(Block::Gate(true, *dir));
                                }
                                for (i, dir) in dirs.iter().enumerate() {
                                    chunk[16 * 15 + 4 + i].push(Block::Move(*dir));
                                }
                                for (i, dir) in dirs.iter().enumerate() {
                                    chunk[16 * 15 + 10 + i].push(Block::Swap(*dir));
                                }
                            }
                            // TOP LEFT
                            {
                                let (chunk, pos) = world.layers[0].get_where(-1, -1);
                                let chunk = world.layers[0].get_mut(&chunk);
                                for (i, blocks) in chunk.iter_mut().enumerate() {
                                    let (x, y) =
                                        (15 - (i as u32 & 0xF), 15 - ((i as u32 & 0xF0) >> 4));
                                    blocks.push(Block::Color(
                                        0xFF000000 | x << 16 | x << 20 | y << 0 | y << 4,
                                    ));
                                }
                            }
                            // TOP RIGHT
                            {
                                let (chunk, pos) = world.layers[0].get_where(0, -1);
                                let chunk = world.layers[0].get_mut(&chunk);
                                for (i, blocks) in chunk.iter_mut().enumerate() {
                                    let (x, y) = (i as u32 & 0xF, 15 - ((i as u32 & 0xF0) >> 4));
                                    blocks.push(Block::Color(
                                        0xFF000000 | x << 8 | x << 12 | y << 0 | y << 4,
                                    ));
                                }
                            }
                            // BOTTOM LEFT
                            {
                                let (chunk, pos) = world.layers[0].get_where(-1, 0);
                                let chunk = world.layers[0].get_mut(&chunk);
                                for (i, blocks) in chunk.iter_mut().enumerate() {
                                    let (x, y) = (15 - (i as u32 & 0xF), (i as u32 & 0xF0) >> 4);
                                    blocks.push(Block::Color(
                                        0xFF000000 | x << 16 | x << 20 | y << 8 | y << 12,
                                    ));
                                }
                            }
                            eprintln!("Setting new WindowState.");
                            self.state =
                                WindowState::Singleplayer(WSInGame::default(), Runner::new(world));
                            self.redraw = true;
                            eprintln!("Done.");
                        }
                    }
                }
                WindowState::LoadingWorld(..) => {}
                WindowState::Singleplayer(..) => {}
            },
            MouseButton::Right => match &mut state {
                WindowState::Nothing => {}
                WindowState::MainMenu(..) => {}
                WindowState::LoadingWorld(..) => {}
                WindowState::Singleplayer(state, _runner) => {
                    // where 0|0 is the screen's center
                    let mouse_centered = Vec2::new(
                        self.mouse_pos.x - self.size.x as f32 / 2.0,
                        self.mouse_pos.y - self.size.y as f32 / 2.0,
                    );
                    let block_pos = Vec2::new(
                        state.position.x + mouse_centered.x / state.pixels_per_block,
                        state.position.y + mouse_centered.y / state.pixels_per_block,
                    );
                    match &mut state.open_menu {
                        Some((_, WSInGameMenu::BlockStackChanger { changing, block: _ })) => {
                            if !changing.as_ref().is_some_and(|v| v.0) {
                                *changing = Some((true, Instant::now()))
                            }
                        }
                        None => {
                            state.open_menu = Some((
                                self.mouse_pos,
                                WSInGameMenu::BlockStackChanger {
                                    changing: Some((false, Instant::now())),
                                    block: (block_pos.x.round() as _, block_pos.y.round() as _),
                                },
                            ))
                        }
                    }
                }
            },
            _ => {}
        }
        self.state.setnew(state);
        helper.request_redraw();
    }
    fn on_mouse_wheel_scroll(
        &mut self,
        helper: &mut WindowHelper<Event>,
        distance: MouseScrollDistance,
    ) {
        let mut state = self.state.take();
        match &mut state {
            WindowState::Nothing => {}
            WindowState::MainMenu(..) => {}
            WindowState::LoadingWorld(..) => {}
            WindowState::Singleplayer(state, _) => {
                state.zoom += match distance {
                    MouseScrollDistance::Lines { y, .. } => y as f32 * 0.25,
                    MouseScrollDistance::Pixels { y, .. } => y as f32 * 0.05,
                    MouseScrollDistance::Pages { y, .. } => y as f32,
                }
            }
        }
        self.state.setnew(state);
        helper.request_redraw();
    }
    fn on_user_event(
        &mut self,
        helper: &mut speedy2d::window::WindowHelper<Event>,
        user_event: Event,
    ) {
        self.events.push(user_event);
        helper.request_redraw();
    }
    fn on_resize(
        &mut self,
        helper: &mut speedy2d::window::WindowHelper<Event>,
        size_pixels: speedy2d::dimen::UVec2,
    ) {
        self.size = size_pixels;
        self.redraw = true;
        helper.request_redraw();
    }
    fn on_mouse_move(
        &mut self,
        helper: &mut speedy2d::window::WindowHelper<Event>,
        position: Vec2,
    ) {
        match &mut self.state {
            WindowState::Nothing | WindowState::MainMenu(..) | WindowState::LoadingWorld(..) => {}
            WindowState::Singleplayer(state, _) => {
                if self.mouse_down_l {
                    state.position -= (position - self.mouse_pos) / state.pixels_per_block;
                }
            }
        }
        self.mouse_pos = position;
        helper.request_redraw();
    }
}

struct Config {
    main_font: String,
    mono_font: String,
    saves_dir: String,
    assets_dir: String,
}
struct ThreadedLoading {
    config: Arc<Config>,
    thread: JoinHandle<Result<UserEventSender<Event>, LoadError>>,
}

struct WSMainMenu {
    singleplayer_world_box: Rectangle,
    singleplayer_new_world_button: Rectangle,
    singleplayer_new_world_button_brightness: f32,
    title_text: Option<Rc<FormattedTextBlock>>,
    desired_world_height: f32,
    world_scroll: usize,
    worlds_texts: Vec<Option<Rc<FormattedTextBlock>>>,
}
struct WSInGame {
    layer: usize,
    position: Vec2,
    zoom: f32,
    /// updated on each draw
    pixels_per_block: f32,
    open_menu: Option<(Vec2, WSInGameMenu)>,
}
impl Default for WSInGame {
    fn default() -> Self {
        Self {
            layer: 0,
            position: Vec2::ZERO,
            zoom: 5.0,
            pixels_per_block: 1.0,
            open_menu: None,
        }
    }
}
enum WSInGameMenu {
    BlockStackChanger {
        /// false => opening, true => closing
        changing: Option<(bool, Instant)>,
        block: (i64, i64),
    },
}

impl ThreadedLoading {
    pub fn new(event_sender: UserEventSender<Event>) -> Result<Self, ConfigLoadError> {
        let mut saves_dir = Err(ConfigLoadError::NoSavesDir);
        let mut assets_dir = Err(ConfigLoadError::NoAssetsDir);
        let mut main_font = Err(ConfigLoadError::NoMainFont);
        let mut mono_font = Err(ConfigLoadError::NoMonoFont);
        for (i, line) in match fs::read_to_string("config.txt") {
            Ok(v) => v,
            Err(e) => return Err(ConfigLoadError::NoConfig(e)),
        }
        .lines()
        .enumerate()
        {
            if line.starts_with('#') {
                continue;
            }
            if let Some((key, val)) = line.split_once(' ') {
                match key {
                    "saves-dir" => saves_dir = Ok(val.to_owned()),
                    "assets-dir" => assets_dir = Ok(val.to_owned()),
                    "main-font" => main_font = Ok(val.to_owned()),
                    "mono-font" => mono_font = Ok(val.to_owned()),
                    _ => eprintln!(
                        "Ignoring line {} in config file because key '{key}' is unknown.",
                        i + 1
                    ),
                }
            } else {
                eprintln!(
                    "Ignoring line {} in config file because no ' ' space character was found.",
                    i + 1
                );
            }
        }
        let config = Arc::new(Config {
            main_font: main_font?,
            mono_font: mono_font?,
            saves_dir: saves_dir?,
            assets_dir: assets_dir?,
        });
        Ok(Self {
            config: Arc::clone(&config),
            thread: std::thread::spawn(move || {
                // load fonts
                fn load_font(path: &str) -> Result<Vec<u8>, std::io::Error> {
                    let mut buf = Vec::new();
                    fs::File::open(path)?.read_to_end(&mut buf)?;
                    Ok(buf)
                }
                match load_font(&config.main_font) {
                    Err(e) => return Err(LoadError::MainFont(e)),
                    Ok(v) => event_sender.send_event(Event::LoadFontMain(v)).unwrap(),
                }
                match load_font(&config.mono_font) {
                    Err(e) => return Err(LoadError::MonoFont(e)),
                    Ok(v) => event_sender.send_event(Event::LoadFontMono(v)).unwrap(),
                }
                fn open_image_file(p: &PathBuf) -> Option<RgbaImage> {
                    match fs::File::open(p) {
                        Ok(file) => {
                            match image::load(BufReader::new(file), image::ImageFormat::Png) {
                                Ok(image) => Some(image.into_rgba8()),
                                Err(e) => {
                                    eprintln!("Error loading image {p:?}: {e}");
                                    None
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error opening file {p:?}: {e}");
                            None
                        }
                    }
                }
                fn load_first_image_to_rgba(
                    name: &str,
                    assets_path: &PathBuf,
                    assets_table: &HashMap<String, Vec<u32>>,
                ) -> Option<RgbaImage> {
                    let o = ThreadedLoading::get_first_valid(
                        name,
                        &assets_path,
                        &assets_table,
                        |_, p| open_image_file(&p),
                    );
                    if o.is_none() {
                        eprintln!("No asset named '{name}' found in {assets_path:?}.");
                    }
                    o
                }
                /// inserts up/down/right/left between `name` and `ext`.
                /// it then finds the highest priority directory with at least one of these images.
                /// from there, it uses autorotate to create four images from however many were found.
                /// returns `None` if
                /// - no directory contained any image
                /// - the chosen directory's images couldn't be loaded, but exist on disk
                fn load_four_images_rgba(
                    name: &str,
                    ext: &str,
                    assets_path: &PathBuf,
                    assets_table: &HashMap<String, Vec<u32>>,
                ) -> Option<[RgbaImage; 4]> {
                    let mut found_where = vec![];
                    for dir in ["up", "down", "right", "left"] {
                        let name = format!("{name}{dir}{ext}");
                        found_where.push(ThreadedLoading::get_first_valid(
                            &name,
                            assets_path,
                            assets_table,
                            |id, path| Some((id, path)),
                        ));
                    }
                    if let Some(max) = found_where
                        .iter()
                        .filter_map(|v| v.as_ref())
                        .map(|v| v.0)
                        .max()
                    {
                        let mut found: Vec<_> = found_where
                            .into_iter()
                            .map(|v| {
                                if v.as_ref()?.0 == max {
                                    let path = v?.1;
                                    open_image_file(&path)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        let f4 = found.pop()?;
                        let f3 = found.pop()?;
                        let f2 = found.pop()?;
                        let f1 = found.pop()?;
                        autorotate_rgba_images(f1, f2, f3, f4)
                    } else {
                        eprintln!("No asset named '{name}{{up/down/right/left}}.png' could be found anywhere in {assets_path:?}. (need at least one of four)");
                        None
                    }
                }
                /// given at least one of four images, this method will return four images by rotating the images it was given.
                /// if and only if all four images are `None`, this method also returns `None`.
                fn autorotate_rgba_images(
                    up: Option<RgbaImage>,
                    down: Option<RgbaImage>,
                    right: Option<RgbaImage>,
                    left: Option<RgbaImage>,
                ) -> Option<[RgbaImage; 4]> {
                    let up = if let Some(up) = up {
                        up
                    } else if let Some(down) = &down {
                        imageops::rotate180(down)
                    } else if let Some(left) = &left {
                        imageops::rotate90(left)
                    } else if let Some(right) = &right {
                        imageops::rotate270(right)
                    } else {
                        eprintln!("Cannot autorotate images: There are no images");
                        return None;
                    };
                    let down = if let Some(down) = down {
                        down
                    } else {
                        imageops::rotate180(&up)
                    };
                    let right = if let Some(right) = right {
                        right
                    } else if let Some(left) = &left {
                        imageops::rotate180(left)
                    } else {
                        imageops::rotate90(&up)
                    };
                    let left = if let Some(left) = left {
                        left
                    } else {
                        imageops::rotate180(&right)
                    };
                    Some([up, down, right, left])
                }
                // load menu assets (assets/menu/*/*)
                let assets_path_menu = Path::new(&config.assets_dir).join("menu");
                let assets_table_menu = match Self::assets_priority_table(&assets_path_menu) {
                    Ok(v) => v,
                    Err(_) => return Err(LoadError::MissingAsset("menu".to_owned())),
                };
                if let Some(bg) = load_first_image_to_rgba(
                    "background.png",
                    &assets_path_menu,
                    &assets_table_menu,
                ) {
                    event_sender
                        .send_event(Event::SetMainMenuBackgroundImage(bg))
                        .unwrap();
                }
                if let Some(btn) = load_first_image_to_rgba(
                    "new_singleplayer_world_button.png",
                    &assets_path_menu,
                    &assets_table_menu,
                ) {
                    event_sender
                        .send_event(Event::SetMainMenuSingleplayerNewWorldImage(btn))
                        .unwrap();
                }
                // load worlds
                for dir in match fs::read_dir(&config.saves_dir) {
                    Ok(v) => v,
                    Err(e) => return Err(LoadError::CouldNotReadSavesDirectory(e)),
                } {
                    if let Ok(dir) = dir {
                        if dir.metadata().is_ok_and(|meta| meta.is_dir()) {
                            let path = dir.path();
                            let name = path.file_name().unwrap().to_string_lossy().into_owned();
                            event_sender
                                .send_event(Event::AddWorld(path, name))
                                .unwrap();
                            // match World::load_from_dir(&path) {
                            //     Err(e) => eprintln!("Couldn't load world from {dir:?}: {e:?}"),
                            //     Ok(None) => {
                            //         eprintln!("Couldn't load world from {dir:?} - byte parse error")
                            //     }
                            //     Ok(Some(loaded_world)) => {
                            //         event_sender
                            //             .send_event(Event::AddWorld(
                            //                 path.file_name()
                            //                     .unwrap()
                            //                     .to_string_lossy()
                            //                     .into_owned(),
                            //                 loaded_world,
                            //             ))
                            //             .unwrap();
                            //     }
                            // }
                        }
                    }
                }
                // load world assets (assets/world/*/*)
                let assets_path_world = Path::new(&config.assets_dir).join("world");
                let assets_table_world = match Self::assets_priority_table(&assets_path_world) {
                    Ok(v) => v,
                    Err(_) => return Err(LoadError::MissingAsset("world".to_owned())),
                };
                /// actual file names are "{name}{to/away/up/down/right/left}.png".
                /// value is up, down, right, left, to, away
                fn load_six_images_and_send<F: FnOnce([Option<RgbaImage>; 6])>(
                    name: &str,
                    f: F,
                    assets_path: &PathBuf,
                    assets_table: &HashMap<String, Vec<u32>>,
                ) {
                    let to = load_first_image_to_rgba(
                        &format!("{name}to.png"),
                        assets_path,
                        assets_table,
                    );
                    let away = load_first_image_to_rgba(
                        &format!("{name}away.png"),
                        assets_path,
                        assets_table,
                    );
                    if let Some(imgs) =
                        load_four_images_rgba(name, ".png", &assets_path, &assets_table)
                    {
                        let [f1, f2, f3, f4] = imgs;
                        f([Some(f1), Some(f2), Some(f3), Some(f4), to, away]);
                    } else if to.is_some() || away.is_some() {
                        f([None, None, None, None, to, away])
                    }
                }
                if let Some(img) = load_first_image_to_rgba(
                    "block_color.png",
                    &assets_path_world,
                    &assets_table_world,
                ) {
                    event_sender
                        .send_event(Event::SetWorldBlockColor(img))
                        .unwrap();
                }
                if let Some(img) = load_first_image_to_rgba(
                    "block_char.png",
                    &assets_path_world,
                    &assets_table_world,
                ) {
                    event_sender
                        .send_event(Event::SetWorldBlockChar(img))
                        .unwrap();
                }
                load_six_images_and_send(
                    "block_delay_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockDelay(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_storage_sto_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockStorageSto(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_storage_or_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockStorageOr(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_storage_and_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockStorageAnd(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_storage_xor_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockStorageXor(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_storage_add_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockStorageAdd(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_storage_sub_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockStorageSub(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_storage_mul_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockStorageMul(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_storage_div_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockStorageDiv(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_storage_mod_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockStorageMod(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_storage_default_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockStorageDefault(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_gate_open_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockGateOpen(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_gate_closed_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockGateClosed(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_move_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockMove(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                load_six_images_and_send(
                    "block_swap_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockSwap(v))
                            .unwrap()
                    },
                    &assets_path_world,
                    &assets_table_world,
                );
                Ok(event_sender)
            }),
        })
    }
    fn get_first_valid<P: AsRef<Path>, F: Fn(u32, PathBuf) -> Option<R>, R>(
        file_name: &str,
        assets_dir: P,
        table: &HashMap<String, Vec<u32>>,
        func: F,
    ) -> Option<R> {
        if let Some(dirs) = table.get(file_name) {
            for dir in dirs.iter().rev() {
                let path = assets_dir.as_ref().join(format!("{dir}/{file_name}"));
                if let Some(v) = func(*dir, path) {
                    return Some(v);
                }
            }
            None
        } else {
            None
        }
    }
    fn assets_priority_table<P: AsRef<Path>>(
        dir: P,
    ) -> Result<HashMap<String, Vec<u32>>, std::io::Error> {
        let mut out: HashMap<String, Vec<u32>> = HashMap::new();
        for entry in fs::read_dir(dir)? {
            if let Ok(priority_dir) = entry {
                if priority_dir.metadata().is_ok_and(|meta| meta.is_dir()) {
                    if let Ok(entries) = fs::read_dir(priority_dir.path()) {
                        if let Ok(priority_name) = priority_dir.file_name().into_string() {
                            if let Ok(priority_name) = priority_name.parse() {
                                for entry in entries {
                                    if let Ok(entry) = entry {
                                        if let Ok(file_name) = entry.file_name().into_string() {
                                            if let Some(list) = out.get_mut(&file_name) {
                                                list.push(priority_name);
                                            } else {
                                                out.insert(file_name, vec![priority_name]);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        for (_, sources) in out.iter_mut() {
            sources.sort_unstable();
        }
        Ok(out)
    }
}

#[derive(Debug)]
enum ConfigLoadError {
    NoConfig(std::io::Error),
    NoSavesDir,
    NoAssetsDir,
    NoMainFont,
    NoMonoFont,
}

#[derive(Debug)]
enum LoadError {
    MainFont(std::io::Error),
    MonoFont(std::io::Error),
    CouldNotReadSavesDirectory(std::io::Error),
    /// String is path relative to assets dir
    MissingAsset(String),
}

impl Window {
    fn draw_block(&mut self, graphics: &mut Graphics2D, area: Rectangle<f32>, block: &Block) {
        match block {
            Block::Color(c) => {
                graphics.draw_rectangle(area.clone(), Color::from_hex_argb(*c));
                if let Some(handle) = self.images.world_block_color.handle() {
                    graphics.draw_rectangle_image(area, handle);
                }
            }
            Block::Char(c) => {
                if let Some(handle) = self.images.world_block_char.handle() {
                    graphics.draw_rectangle_image(area.clone(), handle);
                }
                if let Some(font) = &self.font_monospace {
                    if let Some(c) = char::from_u32(*c) {
                        let layout = font.layout_text(&c.to_string(), 1.0, TextOptions::default());
                        let scale =
                            (area.height() / layout.height()).min(area.width() / layout.width());
                        let layout =
                            font.layout_text(&c.to_string(), scale, TextOptions::default());
                        let pos = (area.top_left() + area.bottom_right()
                            - Vec2::new(layout.width(), layout.height()))
                            / 2.0;
                        graphics.draw_text(pos, Color::WHITE, &layout);
                    }
                }
            }
            Block::Delay(_, dir) => {
                if let Some(handle) =
                    Self::index_by_dir(*dir, &self.images.world_block_delay).handle()
                {
                    graphics.draw_rectangle_image(area.clone(), handle);
                }
            }
            Block::Storage(_, mode, dir) => {
                if let Some(handle) = Self::index_by_dir(
                    *dir,
                    match mode {
                        0 => &self.images.world_block_storage_sto,
                        1 => &self.images.world_block_storage_or,
                        2 => &self.images.world_block_storage_and,
                        3 => &self.images.world_block_storage_xor,
                        4 => &self.images.world_block_storage_add,
                        5 => &self.images.world_block_storage_sub,
                        6 => &self.images.world_block_storage_mul,
                        7 => &self.images.world_block_storage_div,
                        8 => &self.images.world_block_storage_mod,
                        _ => &self.images.world_block_storage_default,
                    },
                )
                .handle()
                {
                    graphics.draw_rectangle_image(area.clone(), handle);
                }
            }
            Block::Gate(open, dir) => {
                if let Some(handle) = Self::index_by_dir(
                    *dir,
                    if *open {
                        &self.images.world_block_gate_open
                    } else {
                        &self.images.world_block_gate_closed
                    },
                )
                .handle()
                {
                    graphics.draw_rectangle_image(area.clone(), handle);
                }
            }
            Block::Move(dir) => {
                if let Some(handle) =
                    Self::index_by_dir(*dir, &self.images.world_block_move).handle()
                {
                    graphics.draw_rectangle_image(area.clone(), handle);
                }
            }
            Block::Swap(dir) => {
                if let Some(handle) =
                    Self::index_by_dir(*dir, &self.images.world_block_swap).handle()
                {
                    graphics.draw_rectangle_image(area.clone(), handle);
                }
            }
        }
    }
    fn index_by_dir(dir: u8, dest: &[LoadableImage; 6]) -> &LoadableImage {
        &dest[match dir {
            runner::DIR_UP => 0,
            runner::DIR_DOWN => 1,
            runner::DIR_RIGHT => 2,
            runner::DIR_LEFT => 3,
            runner::DIR_UP_L => 4,
            runner::DIR_DOWN_L => 5,
            _ => panic!("dir was not (just) a direction!"),
        }]
    }
    fn rel_to_abs_rect(size: UVec2, rect: &Rectangle<f32>) -> Rectangle<f32> {
        Rectangle::new(
            Vec2::new(
                size.x as f32 * rect.top_left().x,
                size.y as f32 * rect.top_left().y,
            ),
            Vec2::new(
                size.x as f32 * rect.bottom_right().x,
                size.y as f32 * rect.bottom_right().y,
            ),
        )
    }
    fn load_img(dest: &mut LoadableImage, img: RgbaImage, graphics: &mut Graphics2D) {
        if let Ok(handle) = graphics.create_image_from_raw_pixels(
            ImageDataType::RGBA,
            ImageSmoothingMode::Linear,
            UVec2::new(img.width(), img.height()),
            &img,
        ) {
            dest.load(handle);
        }
    }
    fn load_imgs<const L: usize>(
        dest: &mut [LoadableImage; L],
        img: [Option<RgbaImage>; L],
        graphics: &mut Graphics2D,
    ) {
        for (i, img) in img.into_iter().enumerate() {
            if let Some(img) = img {
                Self::load_img(&mut dest[i], img, graphics);
            }
        }
    }
}
#[derive(Default)]
struct LoadableImage(Option<(ImageHandle, Option<Instant>)>);
impl LoadableImage {
    fn load(&mut self, handle: ImageHandle) {
        self.0 = Some((handle, Some(Instant::now())));
    }
    fn loaded(&self) -> bool {
        self.0.is_some()
    }
    fn handle(&self) -> Option<&ImageHandle> {
        if let Some((v, _)) = &self.0 {
            Some(v)
        } else {
            None
        }
    }
    fn clear(&mut self) {
        self.0 = None;
    }
    fn draw_image_aspect_ratio_tinted(
        &mut self,
        graphics: &mut Graphics2D,
        helper: &mut WindowHelper<Event>,
        pos: Rectangle<f32>,
        tint: Color,
        crop: bool,
    ) {
        if let Some((image, since_when)) = &mut self.0 {
            let tint = if let Some(t) = since_when {
                helper.request_redraw();
                let t = t.elapsed().as_secs_f32();
                if t >= 1.0 {
                    *since_when = None;
                    tint
                } else {
                    Color::from_rgba(t * tint.r(), t * tint.g(), t * tint.b(), t * tint.a())
                }
            } else {
                tint
            };
            let img_aspect_ratio = image.size().x as f32 / image.size().y as f32;
            let area_aspect_ratio = pos.width() / pos.height();
            if crop {
                let subset = if area_aspect_ratio > img_aspect_ratio {
                    let cut = 0.5 - 0.5 * img_aspect_ratio / area_aspect_ratio;
                    Rectangle::new(Vec2::new(0.0, cut), Vec2::new(1.0, 1.0 - cut))
                } else {
                    let cut = 0.5 - 0.5 * area_aspect_ratio / img_aspect_ratio;
                    Rectangle::new(Vec2::new(cut, 0.0), Vec2::new(1.0 - cut, 1.0))
                };
                graphics.draw_rectangle_image_subset_tinted(pos, tint, subset, image);
            } else {
                let area = if area_aspect_ratio > img_aspect_ratio {
                    let w_crop = pos.width() * 0.5 * (1.0 - (img_aspect_ratio / area_aspect_ratio));
                    Rectangle::new(
                        Vec2::new(pos.top_left().x + w_crop, pos.top_left().y),
                        Vec2::new(pos.bottom_right().x - w_crop, pos.bottom_right().y),
                    )
                } else {
                    let h_crop =
                        pos.height() * 0.5 * (1.0 - (area_aspect_ratio / img_aspect_ratio));
                    Rectangle::new(
                        Vec2::new(pos.top_left().x, pos.top_left().y + h_crop),
                        Vec2::new(pos.bottom_right().x, pos.bottom_right().y - h_crop),
                    )
                };
                graphics.draw_rectangle_image_tinted(area, tint, image);
            }
        }
    }
}
