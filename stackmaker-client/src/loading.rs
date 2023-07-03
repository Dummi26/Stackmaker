use std::{
    collections::HashMap,
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::Arc,
    thread::JoinHandle,
};

use image::{imageops, RgbaImage};
use speedy2d::window::UserEventSender;

use crate::{Config, Event};

pub struct ThreadedLoading {
    pub config: Arc<Config>,
    pub thread: JoinHandle<Result<UserEventSender<Event>, LoadError>>,
}
#[derive(Debug)]
pub enum ConfigLoadError {
    NoConfig(std::io::Error),
    NoSavesDir,
    NoAssetsDir,
    NoMainFont,
    NoMonoFont,
}
#[derive(Debug)]
pub enum LoadError {
    MainFont(std::io::Error),
    MonoFont(std::io::Error),
    CouldNotReadSavesDirectory(std::io::Error),
    /// String is path relative to assets dir
    MissingAsset(String),
}

impl ThreadedLoading {
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
    fn get_first_valid<P: AsRef<Path>, F: Fn(u32, PathBuf) -> Option<R>, R>(
        file_name: &str,
        assets_dir: P,
        table: &HashMap<String, Vec<u32>>,
        func: F,
    ) -> Option<R> {
        if let Some(dirs) = table.get(file_name) {
            for dir in dirs.iter().rev() {
                let path = assets_dir.as_ref().join(dir.to_string()).join(file_name);
                if let Some(v) = func(*dir, path) {
                    return Some(v);
                }
            }
            None
        } else {
            None
        }
    }
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
                    "menu_arrow_selected.png",
                    &assets_path_world,
                    &assets_table_world,
                ) {
                    event_sender
                        .send_event(Event::SetWorldMenuArrowSelected(img))
                        .unwrap();
                }
                if let Some(img) = load_first_image_to_rgba(
                    "menu_arrow_source.png",
                    &assets_path_world,
                    &assets_table_world,
                ) {
                    event_sender
                        .send_event(Event::SetWorldMenuArrowSource(img))
                        .unwrap();
                }
                if let Some(img) = load_first_image_to_rgba(
                    "menu_arrow_target.png",
                    &assets_path_world,
                    &assets_table_world,
                ) {
                    event_sender
                        .send_event(Event::SetWorldMenuArrowTarget(img))
                        .unwrap();
                }
                if let Some(img) = load_first_image_to_rgba(
                    "menu_button_pause.png",
                    &assets_path_world,
                    &assets_table_world,
                ) {
                    event_sender
                        .send_event(Event::SetWorldMenuButtonPause(img))
                        .unwrap();
                }
                if let Some(img) = load_first_image_to_rgba(
                    "menu_button_paused.png",
                    &assets_path_world,
                    &assets_table_world,
                ) {
                    event_sender
                        .send_event(Event::SetWorldMenuButtonPaused(img))
                        .unwrap();
                }
                if let Some(img) = load_first_image_to_rgba(
                    "menu_button_tick.png",
                    &assets_path_world,
                    &assets_table_world,
                ) {
                    event_sender
                        .send_event(Event::SetWorldMenuButtonTick(img))
                        .unwrap();
                }
                if let Some(img) = load_first_image_to_rgba(
                    "menu_button_signalzero.png",
                    &assets_path_world,
                    &assets_table_world,
                ) {
                    event_sender
                        .send_event(Event::SetWorldMenuButtonSignalzero(img))
                        .unwrap();
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
                    "block_splitter_",
                    |v| {
                        event_sender
                            .send_event(Event::SetWorldBlockSplitter(v))
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
}
