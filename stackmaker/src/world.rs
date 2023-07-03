use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{Read, Write},
    path::Path,
};

pub struct World {
    pub layers: [Layer; 32],
    /// (signal, (dir (3b) + layer (5b)), target_chunk, target_pos)
    pub signals_queue: VecDeque<Vec<(u32, u8, u64, u8)>>,
}

pub struct Layer {
    pub chunks: HashMap<u64, [Vec<Block>; 256]>,
}

#[derive(Debug)]
pub enum Block {
    // < Output >
    /// A single-color block; format is argb. If it receives any signal, its internal value will be set to that of the signal.
    Color(u32),
    /// A character-display block. If it receives any signal, its internal value will be set to that of the signal.
    Char(u32),

    // < Logic >
    /// Passes a received signal on after the given amount of game ticks.
    /// Side-signals set the amount of ticks to wait.
    Delay(u32, u8),
    /// Stores a signal value. If it receives a signal, the current value is output and the mode is set according to the signal.
    /// If it receives a signal on its side, the stored value is modified according to the set mode:
    /// - 0 (sto): storage. signals from the side will overwrite the stored value.
    /// - 1 (or) : performs a bitwise or: all `1` bits from the side-signal value will be set to `1` on the stored value.
    /// - 2 (and): performs a bitwise and: all `0` bits from the side-signal value will be set to `0` on the stored value.
    /// - 3 (xor): performs a bitwise xor: all `1` bits from the side-signal value will invert the stored value's bit at that position.
    /// - 4 (add): side-signal values will be added to the stored value. value saturates at integer boundaries.
    /// - 5 (sub): side-signal values will be subtracted from the stored value. value saturates at integer boundaries.
    /// - 6 (mul): the stored value will be multiplied with the value from the side-signal. value saturates at integer boundaries.
    /// - 7 (div): the stored value will be divided by the value from the side-signal. dividing by zero gives the max value.
    /// - 8 (mod): the stored value will be divided by the value from the side-signal, and the remainder will be stored.
    /// - default: the stored value will not be changed at all if the mode was set to any other value.
    /// Stored as (value, mode, direction)
    Storage(u32, u8, u8),
    /// Only lets a signal pass if it is open, that is, the last side-signal received was `0`.
    /// In combination with the Storage Block, this can be used to implement all kinds of conditions.
    Gate(bool, u8),
    /// Outputs two identical signals upon receiving one.
    /// This block is triggered exclusively by side-signals.
    Splitter(u8),

    // < World >
    /// Upon receiving a `0` side-signal, takes a block from one stack and puts it on another, following the provided direction. If it receives any other signal, moves a block back.
    Move(u8),
    /// Upon receiving any side-signal, swaps the blocks in front/behind itself
    Swap(u8),
}

impl World {
    pub fn new_empty() -> Self {
        Self {
            layers: Default::default(),
            signals_queue: VecDeque::new(),
        }
    }
    pub fn signals_mut(&mut self, delta_t: usize) -> &mut Vec<(u32, u8, u64, u8)> {
        while delta_t >= self.signals_queue.len() {
            self.signals_queue.push_back(vec![]);
        }
        &mut self.signals_queue[delta_t]
    }
}

impl Layer {
    pub fn get(&self, chunk: &u64) -> Option<&[Vec<Block>; 256]> {
        self.chunks.get(chunk)
    }
    pub fn get_where(&self, x: i64, y: i64) -> (u64, u8) {
        let x2 = u32::from_ne_bytes(
            (if x.is_negative() {
                -((-x - 1) / 16) - 1
            } else {
                x / 16
            } as i32)
                .to_ne_bytes(),
        ) as u64;
        let y2 = u32::from_ne_bytes(
            (if y.is_negative() {
                -((-y - 1) / 16) - 1
            } else {
                y / 16
            } as i32)
                .to_ne_bytes(),
        ) as u64;
        let chunk = y2 << 32 | x2;
        let inchunk = (((y % 16 + 16) % 16) << 4) | ((x % 16 + 16) % 16);
        (chunk, inchunk as u8)
    }
    /// Will create the chunk if it doesn't exist
    pub fn get_mut(&mut self, chunk: &u64) -> &mut [Vec<Block>; 256] {
        if !self.chunks.contains_key(chunk) {
            self.chunks.insert(*chunk, create_empty_chunk());
        }
        self.chunks.get_mut(chunk).unwrap()
    }
}

impl Block {}

fn create_empty_chunk<T>() -> [Vec<T>; 256] {
    eprintln!("Creating empty chunk...");
    unsafe {
        #[allow(invalid_value)]
        let mut arr: [Vec<T>; 256] = std::mem::MaybeUninit::uninit().assume_init();
        for item in &mut arr[..] {
            std::ptr::write(item, vec![]);
        }
        arr
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            chunks: HashMap::new(),
        }
    }
}

// SAVING

impl World {
    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> Result<Option<Self>, std::io::Error> {
        let signals_queue = {
            let mut buf = Vec::new();
            fs::File::open(dir.as_ref().join("signals"))?.read_to_end(&mut buf)?;
            if let Some(v) = SaveLoad::load(&mut buf.into_iter()) {
                v
            } else {
                return Ok(None);
            }
        };
        let layers = {
            let mut layers: [Layer; 32] = Default::default();
            for (i, layer) in layers.iter_mut().enumerate() {
                let mut buf = Vec::new();
                fs::File::open(dir.as_ref().join(format!("layer_{i}")))?.read_to_end(&mut buf)?;
                *layer = if let Some(v) = SaveLoad::load(&mut buf.into_iter()) {
                    v
                } else {
                    return Ok(None);
                };
            }
            layers
        };
        Ok(Some(Self {
            layers,
            signals_queue,
        }))
    }
    pub fn save_to_dir<P: AsRef<Path>>(&self, dir: P) -> Result<(), std::io::Error> {
        self.save_signals_queue(dir.as_ref().join("signals"))?;
        for i in 0..self.layers.len() {
            self.save_layer(dir.as_ref().join(format!("layer_{i}")), i)?;
        }
        Ok(())
    }
    pub fn save_signals_queue<P: AsRef<Path>>(&self, path: P) -> Result<(), std::io::Error> {
        let mut buf = vec![];
        self.signals_queue.save(&mut buf);
        fs::File::create(path)?.write_all(&buf)?;
        Ok(())
    }
    pub fn save_layer<P: AsRef<Path>>(&self, path: P, layer: usize) -> Result<(), std::io::Error> {
        let mut buf = vec![];
        self.layers[layer].save(&mut buf);
        fs::File::create(path)?.write_all(&buf)?;
        Ok(())
    }
}

pub trait SaveLoad: Sized {
    fn save(&self, buf: &mut Vec<u8>);
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self>;
}

impl SaveLoad for Layer {
    fn save(&self, buf: &mut Vec<u8>) {
        self.chunks.len().save(buf);
        for (pos, chunk) in self.chunks.iter() {
            pos.save(buf);
            for blocks in chunk {
                blocks.save(buf);
            }
        }
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        let len = SaveLoad::load(src)?;
        let mut chunks = HashMap::with_capacity(len);
        for _ in 0..len {
            let pos = SaveLoad::load(src)?;
            let mut chunk = create_empty_chunk();
            for blocks in &mut chunk {
                *blocks = SaveLoad::load(src)?;
            }
            chunks.insert(pos, chunk);
        }
        Some(Self { chunks })
    }
}

impl SaveLoad for Block {
    fn save(&self, buf: &mut Vec<u8>) {
        match self {
            Self::Color(c) => {
                b'c'.save(buf);
                c.save(buf);
            }
            Self::Char(c) => {
                b'C'.save(buf);
                c.save(buf);
            }
            Self::Delay(t, d) => {
                b'd'.save(buf);
                t.save(buf);
                d.save(buf);
            }
            Self::Storage(val, mode, dir) => {
                b's'.save(buf);
                val.save(buf);
                mode.save(buf);
                dir.save(buf);
            }
            Self::Gate(open, dir) => {
                b'g'.save(buf);
                let as_one = if *open { *dir | 0b1 } else { *dir };
                as_one.save(buf);
            }
            Self::Splitter(dir) => {
                b'G'.save(buf);
                dir.save(buf);
            }
            Self::Move(dir) => {
                b'm'.save(buf);
                dir.save(buf);
            }
            Self::Swap(dir) => {
                b'M'.save(buf);
                dir.save(buf);
            }
        }
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        Some(match src.next()? {
            b'c' => Self::Color(SaveLoad::load(src)?),
            b'C' => Self::Char(SaveLoad::load(src)?),
            b'd' => Self::Delay(SaveLoad::load(src)?, SaveLoad::load(src)?),
            b's' => Self::Storage(
                SaveLoad::load(src)?,
                SaveLoad::load(src)?,
                SaveLoad::load(src)?,
            ),
            b'g' => {
                let as_one: u8 = SaveLoad::load(src)?;
                if as_one & 1 == 1 {
                    // last bit is set, gate is open
                    Self::Gate(true, as_one ^ 1)
                } else {
                    Self::Gate(false, as_one)
                }
            }
            b'G' => Self::Splitter(SaveLoad::load(src)?),
            b'm' => Self::Move(SaveLoad::load(src)?),
            b'M' => Self::Swap(SaveLoad::load(src)?),
            _ => return None,
        })
    }
}

impl<C> SaveLoad for Vec<C>
where
    C: SaveLoad,
{
    fn save(&self, buf: &mut Vec<u8>) {
        self.len().save(buf);
        for v in self {
            v.save(buf);
        }
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        let len = SaveLoad::load(src)?;
        let mut o = Vec::with_capacity(len);
        for _ in 0..len {
            o.push(SaveLoad::load(src)?)
        }
        Some(o)
    }
}
impl<C> SaveLoad for VecDeque<C>
where
    C: SaveLoad,
{
    fn save(&self, buf: &mut Vec<u8>) {
        self.len().save(buf);
        for v in self {
            v.save(buf);
        }
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        let len = SaveLoad::load(src)?;
        let mut o = VecDeque::with_capacity(len);
        for _ in 0..len {
            o.push_back(SaveLoad::load(src)?)
        }
        Some(o)
    }
}
impl SaveLoad for u8 {
    fn save(&self, buf: &mut Vec<u8>) {
        buf.push(*self);
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        src.next()
    }
}
impl SaveLoad for u32 {
    fn save(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.to_be_bytes())
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        Some(Self::from_be_bytes([
            src.next()?,
            src.next()?,
            src.next()?,
            src.next()?,
        ]))
    }
}
impl SaveLoad for u64 {
    fn save(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.to_be_bytes())
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        Some(Self::from_be_bytes([
            src.next()?,
            src.next()?,
            src.next()?,
            src.next()?,
            src.next()?,
            src.next()?,
            src.next()?,
            src.next()?,
        ]))
    }
}
impl SaveLoad for usize {
    fn save(&self, buf: &mut Vec<u8>) {
        (*self as u64).save(buf)
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        Some(u64::load(src)? as _)
    }
}

impl<A, B> SaveLoad for (A, B)
where
    A: SaveLoad,
    B: SaveLoad,
{
    fn save(&self, buf: &mut Vec<u8>) {
        self.0.save(buf);
        self.1.save(buf);
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        Some((SaveLoad::load(src)?, SaveLoad::load(src)?))
    }
}
impl<A, B, C> SaveLoad for (A, B, C)
where
    A: SaveLoad,
    B: SaveLoad,
    C: SaveLoad,
{
    fn save(&self, buf: &mut Vec<u8>) {
        self.0.save(buf);
        self.1.save(buf);
        self.2.save(buf);
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        Some((
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
        ))
    }
}
impl<A, B, C, D> SaveLoad for (A, B, C, D)
where
    A: SaveLoad,
    B: SaveLoad,
    C: SaveLoad,
    D: SaveLoad,
{
    fn save(&self, buf: &mut Vec<u8>) {
        self.0.save(buf);
        self.1.save(buf);
        self.2.save(buf);
        self.3.save(buf);
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        Some((
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
        ))
    }
}
impl<A, B, C, D, E> SaveLoad for (A, B, C, D, E)
where
    A: SaveLoad,
    B: SaveLoad,
    C: SaveLoad,
    D: SaveLoad,
    E: SaveLoad,
{
    fn save(&self, buf: &mut Vec<u8>) {
        self.0.save(buf);
        self.1.save(buf);
        self.2.save(buf);
        self.3.save(buf);
        self.4.save(buf);
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        Some((
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
        ))
    }
}
impl<A, B, C, D, E, F> SaveLoad for (A, B, C, D, E, F)
where
    A: SaveLoad,
    B: SaveLoad,
    C: SaveLoad,
    D: SaveLoad,
    E: SaveLoad,
    F: SaveLoad,
{
    fn save(&self, buf: &mut Vec<u8>) {
        self.0.save(buf);
        self.1.save(buf);
        self.2.save(buf);
        self.3.save(buf);
        self.4.save(buf);
        self.5.save(buf);
    }
    fn load<T: Iterator<Item = u8>>(src: &mut T) -> Option<Self> {
        Some((
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
            SaveLoad::load(src)?,
        ))
    }
}

// text stuffs

impl Block {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Color(..) => "color",
            Self::Char(..) => "char",
            Self::Delay(..) => "delay",
            Self::Storage(_, 0, _) => "storage/sto",
            Self::Storage(_, 1, _) => "storage/or",
            Self::Storage(_, 2, _) => "storage/and",
            Self::Storage(_, 3, _) => "storage/xor",
            Self::Storage(_, 4, _) => "storage/add",
            Self::Storage(_, 5, _) => "storage/sub",
            Self::Storage(_, 6, _) => "storage/mul",
            Self::Storage(_, 7, _) => "storage/div",
            Self::Storage(_, 8, _) => "storage/mod",
            Self::Storage(_, _, _) => "storage/default",
            Self::Gate(true, _) => "gate/open",
            Self::Gate(false, _) => "gate/closed",
            Self::Splitter(_) => "splitter",
            Self::Move(..) => "move",
            Self::Swap(..) => "swap",
        }
    }
}
