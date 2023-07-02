use crate::world::{Block, World};

pub struct Runner {
    pub world: World,
}

pub enum Changes {}

impl Runner {
    pub fn new(mut world: World) -> Self {
        // self.world.signals_queue will never be empty.
        if world.signals_queue.is_empty() {
            world.signals_queue.push_back(vec![]);
        }
        Self { world }
    }
    pub fn tick(&mut self) {
        if self.world.signals_queue.len() < 2 {
            self.world.signals_queue.push_back(vec![]);
        }
        for (signal, mut dir_layer, mut pos_chunk, mut pos_inner) in
            self.world.signals_queue.pop_front().unwrap()
        {
            let chunk = self.world.layers[(dir_layer & 0b11111) as usize].get_mut(&pos_chunk);
            if let Some(block) = chunk[pos_inner as usize].last_mut() {
                match block {
                    Block::Color(c) => *c = signal,
                    Block::Char(c) => *c = signal,
                    Block::Delay(how_long, direction) => {
                        if is_side(*direction, dir_layer) {
                            *how_long = signal;
                        } else if pos_move(&mut dir_layer, &mut pos_chunk, &mut pos_inner) {
                            let v = *how_long as _;
                            self.world
                                .signals_mut(v)
                                .push((signal, dir_layer, pos_chunk, pos_inner));
                        }
                    }
                    Block::Storage(value, mode, direction) => {
                        if is_side(*direction, dir_layer) {
                            match mode {
                                0 => *value = signal,
                                1 => *value |= signal,
                                2 => *value &= signal,
                                3 => *value ^= signal,
                                4 => *value = value.saturating_add(signal),
                                5 => *value = value.saturating_sub(signal),
                                6 => *value = value.saturating_mul(signal),
                                7 => {
                                    *value = if signal == 0 {
                                        u32::MAX
                                    } else {
                                        value.saturating_div(signal)
                                    }
                                }
                                8 => *value %= signal,
                                _ => {}
                            }
                        } else if is_same_dir(*direction, dir_layer) {
                            *mode = signal.min(u8::MAX as _) as _;
                            if pos_move(&mut dir_layer, &mut pos_chunk, &mut pos_inner) {
                                self.world.signals_queue[0]
                                    .push((*value, dir_layer, pos_chunk, pos_inner));
                            }
                        }
                    }
                    Block::Gate(open, direction) => {
                        if is_side(*direction, dir_layer) {
                            *open = signal == 0;
                        } else if *open {
                            if pos_move(&mut dir_layer, &mut pos_chunk, &mut pos_inner) {
                                self.world.signals_queue[0]
                                    .push((signal, dir_layer, pos_chunk, pos_inner));
                            }
                        }
                    }
                    Block::Move(direction) => {
                        if is_side(*direction, dir_layer) {
                            let layer = dir_layer & 0b11111;
                            let dir_layer_in_front = dir_rev(*direction) | layer;
                            let dir_layer_behind = *direction | layer;
                            let (dir_layer_a, dir_layer_b) = if signal == 0 {
                                (dir_layer_behind, dir_layer_in_front)
                            } else {
                                (dir_layer_in_front, dir_layer_behind)
                            };
                            if let (
                                Some((a_dir_layer, a_pos_chunk, a_pos_inner)),
                                Some((b_dir_layer, b_pos_chunk, b_pos_inner)),
                            ) = (
                                pos_moved(dir_layer_a, pos_chunk, pos_inner),
                                pos_moved(dir_layer_b, pos_chunk, pos_inner),
                            ) {
                                if let Some(origin) = self.world.layers
                                    [(a_dir_layer & 0b11111) as usize]
                                    .get_mut(&a_pos_chunk)
                                    [a_pos_inner as usize]
                                    .pop()
                                {
                                    self.world.layers[(b_dir_layer & 0b11111) as usize]
                                        .get_mut(&b_pos_chunk)
                                        [b_pos_inner as usize]
                                        .push(origin)
                                }
                            }
                        }
                    }
                    Block::Swap(direction) => {
                        if is_side(*direction, dir_layer) {
                            let layer = dir_layer & 0b11111;
                            let dir_layer_a = dir_rev(*direction) | layer;
                            let dir_layer_b = *direction | layer;
                            if let (
                                Some((a_dir_layer, a_pos_chunk, a_pos_inner)),
                                Some((b_dir_layer, b_pos_chunk, b_pos_inner)),
                            ) = (
                                pos_moved(dir_layer_a, pos_chunk, pos_inner),
                                pos_moved(dir_layer_b, pos_chunk, pos_inner),
                            ) {
                                if let Some(mut first) = self.world.layers
                                    [(a_dir_layer & 0b11111) as usize]
                                    .get_mut(&a_pos_chunk)
                                    [a_pos_inner as usize]
                                    .pop()
                                {
                                    if let Some(second) = self.world.layers
                                        [(b_dir_layer & 0b11111) as usize]
                                        .get_mut(&b_pos_chunk)
                                        [b_pos_inner as usize]
                                        .last_mut()
                                    {
                                        std::mem::swap(second, &mut first);
                                    }
                                    // push the remaining value to the first stack.
                                    // if a second value existed, this will be that second value (mem::swap),
                                    // if there was no second value, this will just push back the first value which was removed earlier.
                                    self.world.layers[(a_dir_layer & 0b11111) as usize]
                                        .get_mut(&a_pos_chunk)
                                        [a_pos_inner as usize]
                                        .push(first)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// dir bytes (xor all 3 bytes => reverse direction):
// 001 - up (layer)
// 110 - down (layer)
// 100 - left
// 011 - right
// 010 - up
// 101 - down

pub const DIR_UP_L: u8 = 0b00100000;
pub const DIR_DOWN_L: u8 = 0b11000000;
pub const DIR_LEFT: u8 = 0b10000000;
pub const DIR_RIGHT: u8 = 0b01100000;
pub const DIR_UP: u8 = 0b01000000;
pub const DIR_DOWN: u8 = 0b10100000;

/// reverses the direction, keeping the layer bits intact
fn dir_rev(dir: u8) -> u8 {
    dir ^ 0b11100000
}

/// returns true if a and b point in the same direction
fn is_same_dir(a: u8, b: u8) -> bool {
    (a & 0b11100000) == (b & 0b11100000)
}

/// returns true if a and b have different orientations, meaning if a is a block's direction and b a signal's, it is a side-signal.
fn is_side(a: u8, b: u8) -> bool {
    !(is_same_dir(a, b) || is_same_dir(a, dir_rev(b)))
}

/// same as pos_move, but doesn't modify the original values
fn pos_moved(mut dir_layer: u8, mut pos_chunk: u64, mut pos_inner: u8) -> Option<(u8, u64, u8)> {
    if pos_move(&mut dir_layer, &mut pos_chunk, &mut pos_inner) {
        Some((dir_layer, pos_chunk, pos_inner))
    } else {
        None
    }
}
/// moves according to the first 3 bits of dir_layer.
/// direction is retained, layer, chunk- and inner position may be changed.
/// returns false if the new position would be out of bounds.
fn pos_move(dir_layer: &mut u8, pos_chunk: &mut u64, pos_inner: &mut u8) -> bool {
    let dir = *dir_layer & 0b11100000;
    match dir {
        // left
        0b10000000 => {
            if (*pos_inner & 0b1111) == 0 {
                // we are at the very left of this chunk!
                // set to very right of chunk
                *pos_inner |= 0b1111;
                // move one chunk to the left
                *pos_chunk -= 1;
            } else {
                // move one pos to the left
                *pos_inner -= 1;
            }
        }
        // right
        0b01100000 => {
            if (*pos_inner & 0b1111) == 0b1111 {
                // we are at the very right of this chunk! (all 4 bits of the x-part set to 1)
                // set to very left of chunk
                *pos_inner &= 0b11110000;
                // move one chunk to the right
                *pos_chunk += 1;
            } else {
                // move one pos to the left
                *pos_inner += 1;
            }
        }
        // up
        0b01000000 => {
            if (*pos_inner & 0b11110000) == 0 {
                // we are at the very top of this chunk!
                // set to very bottom of chunk
                *pos_inner |= 0b11110000;
                // move one chunk up
                *pos_chunk -= 1 << 32;
            } else {
                // move one pos up
                *pos_inner -= 1 << 4;
            }
        }
        // down
        0b10100000 => {
            if (*pos_inner & 0b11110000) == 0b11110000 {
                // we are at the very bottom of this chunk!
                // set to very top of chunk
                *pos_inner &= 0b1111;
                // move one chunk down
                *pos_chunk += 1 << 32;
            } else {
                // move one pos down
                *pos_inner += 1 << 4;
            }
        }
        // up (layer)
        0b00100000 => {
            if (*dir_layer & 0b11111) == 0 {
                // we are at the upmost layer!
                return false;
            } else {
                // move up one layer
                *dir_layer -= 1;
            }
        }
        // down (layer)
        0b11000000 => {
            if (*dir_layer & 0b11111) == 0b11111 {
                // we are at the lowest layer (all 5 bits set to 1)
                return false;
            } else {
                // move down one layer
                *dir_layer += 1;
            }
        }
        _ => return false,
    }
    true
}
