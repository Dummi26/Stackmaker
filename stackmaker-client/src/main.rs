use std::{
    env::current_dir,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::Instant,
};

use image::RgbaImage;
use loading::ThreadedLoading;
use speedy2d::{
    color::Color,
    dimen::{IVec2, UVec2, Vec2},
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

mod loading;

fn main() {
    let window = speedy2d::Window::new_with_user_events(
        "Stackmaker",
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
            state: WindowState::MainMenu(WSMainMenu::new()),
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
    world_menu_arrow_selected: LoadableImage,
    world_menu_arrow_source: LoadableImage,
    world_menu_arrow_target: LoadableImage,
    world_menu_button_pause: LoadableImage,
    world_menu_button_paused: LoadableImage,
    world_menu_button_tick: LoadableImage,
    world_menu_button_signalzero: LoadableImage,
    world_signal: [LoadableImage; 6],
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
    world_block_splitter: [LoadableImage; 6],
    world_block_move: [LoadableImage; 6],
    world_block_swap: [LoadableImage; 6],
}

pub enum Event {
    LoadFontMain(Vec<u8>),
    LoadFontMono(Vec<u8>),
    AddWorld(PathBuf, String),
    SetMainMenuBackgroundImage(RgbaImage),
    SetMainMenuSingleplayerNewWorldImage(RgbaImage),
    SetWorldMenuArrowSelected(RgbaImage),
    SetWorldMenuArrowSource(RgbaImage),
    SetWorldMenuArrowTarget(RgbaImage),
    SetWorldMenuButtonPause(RgbaImage),
    SetWorldMenuButtonPaused(RgbaImage),
    SetWorldMenuButtonTick(RgbaImage),
    SetWorldMenuButtonSignalzero(RgbaImage),
    SetWorldSignal([Option<RgbaImage>; 6]),
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
    SetWorldBlockSplitter([Option<RgbaImage>; 6]),
    SetWorldBlockMove([Option<RgbaImage>; 6]),
    SetWorldBlockSwap([Option<RgbaImage>; 6]),
}

enum WindowState {
    Nothing,
    MainMenu(WSMainMenu),
    LoadingWorld(Arc<Mutex<f32>>, Option<JoinHandle<Option<Runner>>>),
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
                    Event::SetWorldMenuArrowSelected(img) => {
                        Self::load_img(&mut self.images.world_menu_arrow_selected, img, graphics);
                    }
                    Event::SetWorldMenuArrowSource(img) => {
                        Self::load_img(&mut self.images.world_menu_arrow_source, img, graphics);
                    }
                    Event::SetWorldMenuArrowTarget(img) => {
                        Self::load_img(&mut self.images.world_menu_arrow_target, img, graphics);
                    }
                    Event::SetWorldMenuButtonPause(img) => {
                        Self::load_img(&mut self.images.world_menu_button_pause, img, graphics);
                    }
                    Event::SetWorldMenuButtonPaused(img) => {
                        Self::load_img(&mut self.images.world_menu_button_paused, img, graphics);
                    }
                    Event::SetWorldMenuButtonTick(img) => {
                        Self::load_img(&mut self.images.world_menu_button_tick, img, graphics);
                    }
                    Event::SetWorldMenuButtonSignalzero(img) => {
                        Self::load_img(
                            &mut self.images.world_menu_button_signalzero,
                            img,
                            graphics,
                        );
                    }
                    Event::SetWorldSignal(img) => {
                        Self::load_imgs(&mut self.images.world_signal, img, graphics);
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
                    Event::SetWorldBlockSplitter(img) => {
                        Self::load_imgs(&mut self.images.world_block_splitter, img, graphics);
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
                        {
                            let layout = font.layout_text(
                                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
                                1.0,
                                TextOptions::default(),
                            );
                            state.desired_world_height =
                                48.0 * (self.size.y as f32 / 1080.0).sqrt();
                            state.world_display_font_scale =
                                state.desired_world_height / layout.height();
                        }
                        state.worlds_texts = vec![None; self.saves.len()];
                        for (i, save) in self.saves.iter().enumerate() {
                            state.worlds_texts[i] = Some(font.layout_text(
                                &save.1,
                                state.world_display_font_scale,
                                TextOptions::default(),
                            ));
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
                for (i, text) in state
                    .worlds_texts
                    .iter_mut()
                    .enumerate()
                    .skip(state.world_scroll)
                {
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
                    } else {
                        if let Some(font) = &self.font_main {
                            *text = Some(font.layout_text(
                                &self.saves[i].1,
                                state.world_display_font_scale,
                                TextOptions::default(),
                            ));
                        }
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
                helper.request_redraw();
                if handle.as_ref().unwrap().is_finished() {
                    if let Some(runner) = handle.take().unwrap().join().unwrap() {
                        self.state = WindowState::Singleplayer(WSInGame::default(), runner);
                    } else {
                        self.state = WindowState::MainMenu(WSMainMenu::new())
                    }
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
                        Color::from_int_rgb(40, 0, 20),
                    );
                }
            }
            WindowState::Singleplayer(state, runner) => {
                if state.run {
                    runner.tick();
                }
                graphics.clear_screen(Color::BLACK);
                // draw the blocks
                state.pixels_per_block = 2.0f32.powf(state.zoom);
                let pixels_per_block = state.pixels_per_block;
                let top_left_x = state.position.x - self.size.x as f32 / pixels_per_block / 2.0;
                let top_left_y = state.position.y - self.size.y as f32 / pixels_per_block / 2.0;
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
                // overlay the signal indicator
                for (_, dir_layer, chunk, pos) in &runner.world.signals_queue[0] {
                    let chunk_y = i64::from_ne_bytes((*chunk >> 32).to_ne_bytes());
                    let chunk_x = i64::from_ne_bytes((*chunk & 0xFFFFFFFF).to_ne_bytes());
                    let x = chunk_x * 16 + (*pos as i64) % 16;
                    let y = chunk_y * 16 + (*pos as i64) / 16;
                    let x =
                        (x as f32 - state.position.x) * pixels_per_block + self.size.x as f32 / 2.0;
                    let y =
                        (y as f32 - state.position.y) * pixels_per_block + self.size.y as f32 / 2.0;
                    let signal_area = Rectangle::new(
                        Vec2::new(x - pixels_per_block, y - pixels_per_block),
                        Vec2::new(x + 2.0 * pixels_per_block, y + 2.0 * pixels_per_block),
                    );
                    if signal_area.bottom_right().x >= 0.0
                        && signal_area.bottom_right().y >= 0.0
                        && signal_area.top_left().x <= self.size.x as f32
                        && signal_area.top_left().y <= self.size.y as f32
                    {
                        if let Some(handle) =
                            Self::index_by_dir(*dir_layer & 0b11100000, &self.images.world_signal)
                                .handle()
                        {
                            graphics.draw_rectangle_image(signal_area, handle);
                        }
                    }
                }
                // draw the menu, if there is one
                'draw_menu: {
                    if let Some((_pos, menu)) = &mut state.open_menu {
                        match menu {
                            WSInGameMenu::BlockStackChanger {
                                changing,
                                block,
                                scroll_l,
                                current,
                                target,
                            } => {
                                let (left, right) = if let Some((closing, since_when)) = changing {
                                    let prog = since_when.elapsed().as_secs_f32() * 3.0;
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
                                // left
                                {
                                    let area = Rectangle::new(
                                        Vec2::new(
                                            self.size.y as f32 * left,
                                            self.size.y as f32 * 0.05,
                                        ),
                                        Vec2::new(
                                            self.size.y as f32 * right,
                                            self.size.y as f32 * 0.95,
                                        ),
                                    );
                                    graphics.draw_rectangle(
                                        area.clone(),
                                        Color::from_rgba(0.2, 0.2, 0.2, 0.8),
                                    );
                                    graphics.set_clip(Some(Rectangle::new(
                                        IVec2::new(
                                            area.top_left().x.ceil() as _,
                                            area.top_left().y.ceil() as _,
                                        ),
                                        IVec2::new(
                                            area.bottom_right().x.floor() as _,
                                            area.bottom_right().y.floor() as _,
                                        ),
                                    )));
                                    // get blocks info
                                    let (chunk, inchunk) = runner.world.layers[state.layer]
                                        .get_where(block.0, block.1);
                                    let chunk = runner.world.layers[state.layer].get_mut(&chunk);
                                    let blocks = &mut chunk[inchunk as usize];
                                    // draw blocks
                                    if scroll_l.is_sign_negative() {
                                        *scroll_l = 0.0;
                                    }
                                    let pixels_per_block = area.height() / 9.0;
                                    let mut height =
                                        area.top_left().y - pixels_per_block * (*scroll_l % 1.0);
                                    let block_list_left = area.top_left().x;
                                    let block_list_right = block_list_left + pixels_per_block;
                                    for block in
                                        blocks.iter().rev().skip(scroll_l.floor() as _).take(10)
                                    {
                                        let nheight = height + pixels_per_block;
                                        self.draw_block(
                                            graphics,
                                            Rectangle::new(
                                                Vec2::new(block_list_left, height),
                                                Vec2::new(block_list_right, nheight),
                                            ),
                                            block,
                                        );
                                        height = nheight;
                                    }
                                    // draw arrows
                                    // arrow 1: selected/source
                                    current.1 = 0.6 * current.1 + 0.4 * current.0 as f32;
                                    let arrow_y = area.top_left().y
                                        + pixels_per_block * (current.1 - *scroll_l);
                                    let arrow_area = Rectangle::new(
                                        Vec2::new(block_list_right, arrow_y),
                                        Vec2::new(
                                            block_list_right + pixels_per_block,
                                            arrow_y + pixels_per_block,
                                        ),
                                    );
                                    if area.top_left().y <= arrow_area.bottom_right().y
                                        && area.bottom_right().y >= arrow_area.top_left().y
                                    {
                                        if target.is_none() {
                                            &mut self.images.world_menu_arrow_selected
                                        } else {
                                            &mut self.images.world_menu_arrow_source
                                        }
                                        .draw_image_aspect_ratio_tinted(
                                            graphics,
                                            helper,
                                            arrow_area,
                                            Color::WHITE,
                                            false,
                                        );
                                    }
                                    // arrow 2: target
                                    graphics.set_clip(None);
                                    if let Some((target_block, is_move, target_arr_height)) = target
                                    {
                                        *target_arr_height = 0.6 * *target_arr_height
                                            + 0.4
                                                * if *is_move {
                                                    *target_block as f32 - 0.5
                                                } else {
                                                    *target_block as f32
                                                };
                                        let arrow_y = area.top_left().y
                                            + pixels_per_block * (*target_arr_height - *scroll_l);
                                        let arrow_area = Rectangle::new(
                                            Vec2::new(block_list_right, arrow_y),
                                            Vec2::new(
                                                block_list_right + pixels_per_block,
                                                arrow_y + pixels_per_block,
                                            ),
                                        );
                                        if area.top_left().y <= arrow_area.bottom_right().y
                                            && area.bottom_right().y >= arrow_area.top_left().y
                                        {
                                            self.images
                                                .world_menu_arrow_target
                                                .draw_image_aspect_ratio_tinted(
                                                    graphics,
                                                    helper,
                                                    arrow_area,
                                                    Color::WHITE,
                                                    false,
                                                );
                                        }
                                    }
                                    // buttons
                                    let button_area = |nr: f32| {
                                        Rectangle::new(
                                            Vec2::new(
                                                area.bottom_right().x - pixels_per_block,
                                                area.top_left().y + pixels_per_block * nr,
                                            ),
                                            Vec2::new(
                                                area.bottom_right().x,
                                                area.top_left().y + pixels_per_block * (nr + 1.0),
                                            ),
                                        )
                                    };
                                    // button 1: pause/unpause
                                    let ba = button_area(0.0);
                                    let mi = ba.contains(self.mouse_pos);
                                    if state.run {
                                        &mut self.images.world_menu_button_pause
                                    } else {
                                        &mut self.images.world_menu_button_paused
                                    }
                                    .draw_image_aspect_ratio_tinted(
                                        graphics,
                                        helper,
                                        ba,
                                        if mi { Color::WHITE } else { Color::LIGHT_GRAY },
                                        false,
                                    );
                                    // button 2: tick
                                    let ba = button_area(1.0);
                                    let mi = ba.contains(self.mouse_pos);
                                    self.images
                                        .world_menu_button_tick
                                        .draw_image_aspect_ratio_tinted(
                                            graphics,
                                            helper,
                                            ba,
                                            if mi { Color::WHITE } else { Color::LIGHT_GRAY },
                                            false,
                                        );
                                    // button 3: send signal `0`
                                    let ba = button_area(2.0);
                                    let mi = ba.contains(self.mouse_pos);
                                    self.images
                                        .world_menu_button_signalzero
                                        .draw_image_aspect_ratio_tinted(
                                            graphics,
                                            helper,
                                            ba,
                                            if mi { Color::WHITE } else { Color::LIGHT_GRAY },
                                            false,
                                        );
                                }
                                // right
                                {
                                    let area = Rectangle::new(
                                        Vec2::new(
                                            self.size.x as f32 - self.size.y as f32 * right,
                                            self.size.y as f32 * 0.05,
                                        ),
                                        Vec2::new(
                                            self.size.x as f32 - self.size.y as f32 * left,
                                            self.size.y as f32 * 0.95,
                                        ),
                                    );
                                    graphics.draw_rectangle(
                                        area.clone(),
                                        Color::from_rgba(0.2, 0.2, 0.2, 0.8),
                                    );
                                    let pixels_per_block = area.height() / 9.0 / 2.0;
                                    for (i, block) in state.blocks_for_menu.iter().enumerate() {
                                        let x =
                                            area.top_left().x + pixels_per_block * (i % 6) as f32;
                                        let y =
                                            area.top_left().y + pixels_per_block * (i / 6) as f32;
                                        self.draw_block(
                                            graphics,
                                            Rectangle::new(
                                                Vec2::new(x, y),
                                                Vec2::new(
                                                    x + pixels_per_block,
                                                    y + pixels_per_block,
                                                ),
                                            ),
                                            block,
                                        );
                                    }
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
        match &mut self.state {
            WindowState::Nothing | WindowState::MainMenu(..) | WindowState::LoadingWorld(..) => {}
            WindowState::Singleplayer(state, _) => match &mut state.open_menu {
                None => {}
                Some((
                    _,
                    WSInGameMenu::BlockStackChanger {
                        changing,
                        block,
                        scroll_l: scroll,
                        current,
                        target,
                    },
                )) => {
                    if matches!(button, MouseButton::Left)
                        && self.mouse_pos.y >= self.size.y as f32 * 0.05
                        && self.mouse_pos.y <= self.size.y as f32 * 0.95
                        && self.mouse_pos.x >= 0.0
                        && self.mouse_pos.x <= self.size.y as f32 * 0.2
                    {
                        *target = Some((current.0, false, current.1));
                    }
                }
            },
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
                            let prog = Arc::new(Mutex::new(0.0));
                            let path = save.0.clone();
                            self.state = WindowState::LoadingWorld(
                                Arc::clone(&prog),
                                Some(std::thread::spawn(move || {
                                    match World::load_from_dir(path, Some(prog)) {
                                        Ok(Some(world)) => {
                                            let mut runner = Runner::new(world);
                                            runner.autosave = (100, 1000);
                                            Some(runner)
                                        }
                                        Ok(None) => {
                                            eprintln!("[err] couldn't load world!");
                                            None
                                        }
                                        Err(e) => {
                                            eprintln!("[err] couldn't load world: {e}");
                                            None
                                        }
                                    }
                                })),
                            );
                            self.redraw = true;
                        }
                    } else {
                        let singleplayer_new_world_button =
                            Self::rel_to_abs_rect(self.size, &state.singleplayer_new_world_button);
                        if singleplayer_new_world_button.contains(self.mouse_pos) {
                            eprintln!("Setting up empty world...");
                            let world = World::new_empty();
                            // eprintln!("Adding some blocks for testing...");
                            // {
                            //     let chunk = world.layers[0].get_mut(&0);
                            //     let dirs = [
                            //         runner::DIR_UP,
                            //         runner::DIR_RIGHT,
                            //         runner::DIR_DOWN,
                            //         runner::DIR_LEFT,
                            //         runner::DIR_UP_L,
                            //         runner::DIR_DOWN_L,
                            //     ];
                            //     for ch in (b'A'..=b'Z').rev() {
                            //         chunk[0].push(Block::Char(ch as _));
                            //     }
                            //     for (i, dir) in dirs.iter().enumerate() {
                            //         chunk[16 * 0 + 4 + i].push(Block::Delay(0, *dir));
                            //     }
                            //     for (i, dir) in dirs.iter().enumerate() {
                            //         chunk[16 * 0 + 10 + i].push(Block::Splitter(*dir));
                            //     }
                            //     for mode in 0..=9u8 {
                            //         for (i, dir) in dirs.iter().enumerate() {
                            //             chunk[16 * (1 + mode as usize) + 4 + i]
                            //                 .push(Block::Storage(0, mode, *dir));
                            //         }
                            //     }
                            //     for (i, dir) in dirs.iter().enumerate() {
                            //         chunk[16 * 14 + 4 + i].push(Block::Gate(false, *dir));
                            //     }
                            //     for (i, dir) in dirs.iter().enumerate() {
                            //         chunk[16 * 14 + 10 + i].push(Block::Gate(true, *dir));
                            //     }
                            //     for (i, dir) in dirs.iter().enumerate() {
                            //         chunk[16 * 15 + 4 + i].push(Block::Move(*dir));
                            //     }
                            //     for (i, dir) in dirs.iter().enumerate() {
                            //         chunk[16 * 15 + 10 + i].push(Block::Swap(*dir));
                            //     }
                            // }
                            // // TOP LEFT
                            // {
                            //     let (chunk, pos) = world.layers[0].get_where(-1, -1);
                            //     let chunk = world.layers[0].get_mut(&chunk);
                            //     for (i, blocks) in chunk.iter_mut().enumerate() {
                            //         let (x, y) =
                            //             (15 - (i as u32 & 0xF), 15 - ((i as u32 & 0xF0) >> 4));
                            //         blocks.push(Block::Color(
                            //             0xFF000000 | x << 16 | x << 20 | y << 0 | y << 4,
                            //         ));
                            //     }
                            // }
                            // // TOP RIGHT
                            // {
                            //     let (chunk, pos) = world.layers[0].get_where(0, -1);
                            //     let chunk = world.layers[0].get_mut(&chunk);
                            //     for (i, blocks) in chunk.iter_mut().enumerate() {
                            //         let (x, y) = (i as u32 & 0xF, 15 - ((i as u32 & 0xF0) >> 4));
                            //         blocks.push(Block::Color(
                            //             0xFF000000 | x << 8 | x << 12 | y << 0 | y << 4,
                            //         ));
                            //     }
                            // }
                            // // BOTTOM LEFT
                            // {
                            //     let (chunk, pos) = world.layers[0].get_where(-1, 0);
                            //     let chunk = world.layers[0].get_mut(&chunk);
                            //     for (i, blocks) in chunk.iter_mut().enumerate() {
                            //         let (x, y) = (15 - (i as u32 & 0xF), (i as u32 & 0xF0) >> 4);
                            //         blocks.push(Block::Color(
                            //             0xFF000000 | x << 16 | x << 20 | y << 8 | y << 12,
                            //         ));
                            //     }
                            // }
                            // // BOTTOM 2 RIGHT
                            // {
                            //     let (chunk, _) = world.layers[0].get_where(16, 0);
                            //     let chunk = world.layers[0].get_mut(&chunk);
                            //     chunk[1].push(Block::Color(0xFFFFFFFF));
                            //     chunk[16 + 1].push(Block::Splitter(runner::DIR_UP));
                            //     chunk[32 + 1].push(Block::Delay(0, runner::DIR_DOWN));
                            //     chunk[48 + 1].push(Block::Splitter(runner::DIR_RIGHT));
                            //     chunk[16 + 2].push(Block::Storage(0xFF000000, 4, runner::DIR_LEFT));
                            //     chunk[32 + 2].push(Block::Storage(16, 0, runner::DIR_UP));
                            //     chunk[48 + 2].push(Block::Splitter(runner::DIR_UP));
                            //     chunk[64 + 2].push(Block::Splitter(runner::DIR_RIGHT));
                            //     chunk[16 + 3].push(Block::Splitter(runner::DIR_LEFT));
                            //     chunk[32 + 3].push(Block::Storage(4, 0, runner::DIR_UP));
                            //     chunk[48 + 3].push(Block::Delay(0, runner::DIR_UP));
                            //     chunk[64 + 3].push(Block::Splitter(runner::DIR_UP));
                            // }
                            let mut runner = Runner::new(world);
                            runner.autosave = (500, 0);
                            self.state = WindowState::Singleplayer(WSInGame::default(), runner);
                            self.redraw = true;
                        }
                    }
                }
                WindowState::LoadingWorld(..) => {}
                WindowState::Singleplayer(state, runner) => match &mut state.open_menu {
                    None => {}
                    Some((
                        _,
                        WSInGameMenu::BlockStackChanger {
                            changing,
                            block,
                            scroll_l: scroll,
                            current,
                            target,
                        },
                    )) => {
                        if let Some((which, is_move, _)) = target {
                            let (chunk, inchunk) =
                                runner.world.layers[state.layer].get_where(block.0, block.1);
                            let blocks = &mut runner.world.layers[state.layer].get_mut(&chunk)
                                [inchunk as usize];
                            if *is_move {
                                if current.0 < blocks.len() && *which <= blocks.len() {
                                    let block = blocks.remove(blocks.len() - 1 - current.0);
                                    let which = if *which > current.0 {
                                        blocks.len() + 1 - *which
                                    } else {
                                        blocks.len() - *which
                                    };
                                    blocks.insert(which, block);
                                }
                            } else {
                                if current.0 < blocks.len() && *which < blocks.len() {
                                    let len = blocks.len();
                                    blocks.swap(len - 1 - current.0, len - 1 - *which);
                                }
                            }
                            *target = None;
                        } else if self.mouse_pos.y >= self.size.y as f32 * 0.05
                            && self.mouse_pos.y <= self.size.y as f32 * 0.95
                            && self.mouse_pos.x >= self.size.y as f32 * 0.2
                            && self.mouse_pos.x <= self.size.y as f32 * 0.3
                        {
                            // 0..9
                            let which_button =
                                ((self.mouse_pos.y / self.size.y as f32) - 0.05) * 10.0;
                            match which_button as usize {
                                0 => state.run = !state.run,
                                1 => runner.tick(),
                                2 => {
                                    // send zero-signal from above
                                    let (chunk, inchunk) = runner.world.layers[state.layer]
                                        .get_where(block.0, block.1);
                                    runner.world.signals_queue[0].push((
                                        0,
                                        stackmaker::runner::DIR_DOWN_L | state.layer as u8,
                                        chunk,
                                        inchunk,
                                    ));
                                }
                                _ => {}
                            }
                        } else if self.mouse_pos.y >= self.size.y as f32 * 0.05
                            && self.mouse_pos.y <= self.size.y as f32 * 0.95
                            && self.mouse_pos.x >= self.size.x as f32 - self.size.y as f32 * 0.3
                            && self.mouse_pos.x <= self.size.x as f32
                        {
                            let i = 6
                                * (((self.mouse_pos.y / self.size.y as f32) - 0.05) * 20.0)
                                    as usize
                                + (((self.mouse_pos.x - self.size.x as f32
                                    + self.size.y as f32 * 0.3)
                                    * 6.0
                                    / (self.size.y as f32 * 0.3))
                                    as usize)
                                    .max(0)
                                    .min(5);
                            if let Some(add_block) = state.blocks_for_menu.get(i) {
                                let (chunk, pos) =
                                    runner.world.layers[state.layer].get_where(block.0, block.1);
                                runner.world.layers[state.layer].get_mut(&chunk)[pos as usize]
                                    .push(add_block.clone());
                            }
                        }
                    }
                },
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
                        Some((
                            _,
                            WSInGameMenu::BlockStackChanger {
                                changing,
                                block: _,
                                scroll_l: _,
                                current: _,
                                target: _,
                            },
                        )) => {
                            if !changing.as_ref().is_some_and(|v| v.0) {
                                *changing = Some((true, Instant::now()))
                            }
                        }
                        None => {
                            state.open_menu = Some((
                                self.mouse_pos,
                                WSInGameMenu::BlockStackChanger {
                                    changing: Some((false, Instant::now())),
                                    block: (block_pos.x.floor() as _, block_pos.y.floor() as _),
                                    scroll_l: 0.0,
                                    current: (0, -0.0),
                                    target: None,
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
        let dist = match distance {
            MouseScrollDistance::Lines { y, .. } => y as f32 * 1.0,
            MouseScrollDistance::Pixels { y, .. } => y as f32 * 0.1,
            MouseScrollDistance::Pages { y, .. } => y as f32 * 10.0,
        };
        let mut state = self.state.take();
        match &mut state {
            WindowState::Nothing => {}
            WindowState::MainMenu(..) => {}
            WindowState::LoadingWorld(..) => {}
            WindowState::Singleplayer(state, _) => match &mut state.open_menu {
                Some((
                    _,
                    WSInGameMenu::BlockStackChanger {
                        changing,
                        block,
                        scroll_l: scroll,
                        current,
                        target,
                    },
                )) => {
                    let rel_mouse = Vec2::new(
                        self.mouse_pos.x / self.size.x as f32,
                        self.mouse_pos.y / self.size.y as f32,
                    );
                    if rel_mouse.y >= 0.05
                        && rel_mouse.y <= 0.95
                        && self.mouse_pos.x >= 0.0
                        && self.mouse_pos.x <= self.size.y as f32 * 0.3
                    {
                        *scroll -= dist * 0.25;
                    } else {
                        state.zoom += dist * 0.25;
                    }
                }
                None => state.zoom += dist,
            },
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
            WindowState::Singleplayer(state, _) => 'here: {
                match &mut state.open_menu {
                    Some((
                        _,
                        WSInGameMenu::BlockStackChanger {
                            changing,
                            block,
                            scroll_l: scroll,
                            current,
                            target,
                        },
                    )) => {
                        if self.mouse_pos.y >= self.size.y as f32 * 0.05
                            && self.mouse_pos.y <= self.size.y as f32 * 0.95
                            && self.mouse_pos.x >= 0.0
                            && self.mouse_pos.x <= self.size.y as f32 * 0.3
                        {
                            if self.mouse_pos.x <= self.size.y as f32 * 0.2 {
                                // 0.0..=9.0
                                let height_in_menu_blocks =
                                    ((self.mouse_pos.y / self.size.y as f32) - 0.05) * 10.0
                                        + *scroll;
                                if (height_in_menu_blocks % 1.0 - 0.5).abs() < 0.3 {
                                    if let Some((target, is_move, _)) = target {
                                        *target = height_in_menu_blocks as usize;
                                        *is_move = false;
                                    } else {
                                        current.0 = height_in_menu_blocks as usize;
                                    }
                                } else if let Some((target, is_move, _)) = target {
                                    *target = height_in_menu_blocks.round() as usize;
                                    *is_move = true;
                                }
                            }
                            break 'here;
                        }
                    }
                    None => {}
                };
                if self.mouse_down_l {
                    state.position -= (position - self.mouse_pos) / state.pixels_per_block;
                }
            }
        }
        self.mouse_pos = position;
        helper.request_redraw();
    }
}

pub struct Config {
    main_font: String,
    mono_font: String,
    saves_dir: String,
    assets_dir: String,
}

struct WSMainMenu {
    singleplayer_world_box: Rectangle,
    singleplayer_new_world_button: Rectangle,
    singleplayer_new_world_button_brightness: f32,
    title_text: Option<Rc<FormattedTextBlock>>,
    desired_world_height: f32,
    world_display_font_scale: f32,
    world_scroll: usize,
    worlds_texts: Vec<Option<Rc<FormattedTextBlock>>>,
}
impl WSMainMenu {
    fn new() -> Self {
        Self {
            singleplayer_world_box: Rectangle::new(Vec2::new(0.1, 0.4), Vec2::new(0.3, 0.9)),
            singleplayer_new_world_button: Rectangle::new(Vec2::new(0.7, 0.4), Vec2::new(0.9, 0.5)),
            singleplayer_new_world_button_brightness: 0.0,
            title_text: None,
            desired_world_height: 0.0,
            world_display_font_scale: 0.0,
            worlds_texts: vec![],
            world_scroll: 0,
        }
    }
}
struct WSInGame {
    run: bool,
    layer: usize,
    position: Vec2,
    zoom: f32,
    /// updated on each draw
    pixels_per_block: f32,
    open_menu: Option<(Vec2, WSInGameMenu)>,
    blocks_for_menu: Vec<Block>,
}
impl Default for WSInGame {
    fn default() -> Self {
        Self {
            run: false,
            layer: 0,
            position: Vec2::ZERO,
            zoom: 5.0,
            pixels_per_block: 1.0,
            open_menu: None,
            blocks_for_menu: vec![
                Block::Delay(0, runner::DIR_LEFT),
                Block::Delay(0, runner::DIR_UP),
                Block::Delay(0, runner::DIR_DOWN),
                Block::Delay(0, runner::DIR_RIGHT),
                Block::Delay(0, runner::DIR_UP_L),
                Block::Delay(0, runner::DIR_DOWN_L),
                Block::Splitter(runner::DIR_LEFT),
                Block::Splitter(runner::DIR_UP),
                Block::Splitter(runner::DIR_DOWN),
                Block::Splitter(runner::DIR_RIGHT),
                Block::Splitter(runner::DIR_UP_L),
                Block::Splitter(runner::DIR_DOWN_L),
                Block::Color(0xFFFF0000),
                Block::Color(0xFF00FF00),
                Block::Color(0xFF0000FF),
                Block::Color(0xFF000000),
                Block::Color(0xFFFFFFFF),
                Block::Color(0x00000000),
                Block::Char(b'a' as _),
                Block::Char(b'z' as _),
                Block::Char(b'A' as _),
                Block::Char(b'Z' as _),
                Block::Char('\u{1F980}' as _),
                Block::Char(0),
                Block::Storage(0, 0, runner::DIR_LEFT),
                Block::Storage(0, 0, runner::DIR_UP),
                Block::Storage(0, 0, runner::DIR_DOWN),
                Block::Storage(0, 0, runner::DIR_RIGHT),
                Block::Storage(0, 0, runner::DIR_UP_L),
                Block::Storage(0, 0, runner::DIR_DOWN_L),
                Block::Storage(0, 1, runner::DIR_LEFT),
                Block::Storage(0, 1, runner::DIR_UP),
                Block::Storage(0, 1, runner::DIR_DOWN),
                Block::Storage(0, 1, runner::DIR_RIGHT),
                Block::Storage(0, 1, runner::DIR_UP_L),
                Block::Storage(0, 1, runner::DIR_DOWN_L),
                Block::Storage(0, 2, runner::DIR_LEFT),
                Block::Storage(0, 2, runner::DIR_UP),
                Block::Storage(0, 2, runner::DIR_DOWN),
                Block::Storage(0, 2, runner::DIR_RIGHT),
                Block::Storage(0, 2, runner::DIR_UP_L),
                Block::Storage(0, 2, runner::DIR_DOWN_L),
                Block::Storage(0, 3, runner::DIR_LEFT),
                Block::Storage(0, 3, runner::DIR_UP),
                Block::Storage(0, 3, runner::DIR_DOWN),
                Block::Storage(0, 3, runner::DIR_RIGHT),
                Block::Storage(0, 3, runner::DIR_UP_L),
                Block::Storage(0, 3, runner::DIR_DOWN_L),
                Block::Storage(0, 4, runner::DIR_LEFT),
                Block::Storage(0, 4, runner::DIR_UP),
                Block::Storage(0, 4, runner::DIR_DOWN),
                Block::Storage(0, 4, runner::DIR_RIGHT),
                Block::Storage(0, 4, runner::DIR_UP_L),
                Block::Storage(0, 4, runner::DIR_DOWN_L),
                Block::Storage(0, 5, runner::DIR_LEFT),
                Block::Storage(0, 5, runner::DIR_UP),
                Block::Storage(0, 5, runner::DIR_DOWN),
                Block::Storage(0, 5, runner::DIR_RIGHT),
                Block::Storage(0, 5, runner::DIR_UP_L),
                Block::Storage(0, 5, runner::DIR_DOWN_L),
                Block::Storage(0, 6, runner::DIR_LEFT),
                Block::Storage(0, 6, runner::DIR_UP),
                Block::Storage(0, 6, runner::DIR_DOWN),
                Block::Storage(0, 6, runner::DIR_RIGHT),
                Block::Storage(0, 6, runner::DIR_UP_L),
                Block::Storage(0, 6, runner::DIR_DOWN_L),
                Block::Storage(0, 7, runner::DIR_LEFT),
                Block::Storage(0, 7, runner::DIR_UP),
                Block::Storage(0, 7, runner::DIR_DOWN),
                Block::Storage(0, 7, runner::DIR_RIGHT),
                Block::Storage(0, 7, runner::DIR_UP_L),
                Block::Storage(0, 7, runner::DIR_DOWN_L),
                Block::Storage(0, 8, runner::DIR_LEFT),
                Block::Storage(0, 8, runner::DIR_UP),
                Block::Storage(0, 8, runner::DIR_DOWN),
                Block::Storage(0, 8, runner::DIR_RIGHT),
                Block::Storage(0, 8, runner::DIR_UP_L),
                Block::Storage(0, 8, runner::DIR_DOWN_L),
                Block::Storage(0, 9, runner::DIR_LEFT),
                Block::Storage(0, 9, runner::DIR_UP),
                Block::Storage(0, 9, runner::DIR_DOWN),
                Block::Storage(0, 9, runner::DIR_RIGHT),
                Block::Storage(0, 9, runner::DIR_UP_L),
                Block::Storage(0, 9, runner::DIR_DOWN_L),
                Block::Gate(false, runner::DIR_LEFT),
                Block::Gate(false, runner::DIR_UP),
                Block::Gate(false, runner::DIR_DOWN),
                Block::Gate(false, runner::DIR_RIGHT),
                Block::Gate(false, runner::DIR_UP_L),
                Block::Gate(false, runner::DIR_DOWN_L),
                Block::Gate(true, runner::DIR_LEFT),
                Block::Gate(true, runner::DIR_UP),
                Block::Gate(true, runner::DIR_DOWN),
                Block::Gate(true, runner::DIR_RIGHT),
                Block::Gate(true, runner::DIR_UP_L),
                Block::Gate(true, runner::DIR_DOWN_L),
                Block::Move(runner::DIR_LEFT),
                Block::Move(runner::DIR_UP),
                Block::Move(runner::DIR_DOWN),
                Block::Move(runner::DIR_RIGHT),
                Block::Move(runner::DIR_UP_L),
                Block::Move(runner::DIR_DOWN_L),
                Block::Swap(runner::DIR_LEFT),
                Block::Swap(runner::DIR_UP),
                Block::Swap(runner::DIR_DOWN),
                Block::Swap(runner::DIR_RIGHT),
                Block::Swap(runner::DIR_UP_L),
                Block::Swap(runner::DIR_DOWN_L),
            ],
        }
    }
}
enum WSInGameMenu {
    BlockStackChanger {
        /// false => opening, true => closing
        changing: Option<(bool, Instant)>,
        block: (i64, i64),
        scroll_l: f32,
        /// if target.is_none(), this is which block we are editing.
        /// if target.is_some(), this is the origin of the move/swap operation.
        current: (usize, f32),
        /// if Some((_, false)), swap, if Some((_, true)), move
        target: Option<(usize, bool, f32)>,
    },
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
            Block::Splitter(dir) => {
                if let Some(handle) =
                    Self::index_by_dir(*dir, &self.images.world_block_splitter).handle()
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
            ImageSmoothingMode::NearestNeighbor,
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
