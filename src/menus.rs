use std::any::TypeId;
use std::cell::Cell;

use crate::layout::Frame;

type Storage = [usize; 4];

#[derive(Debug, Clone)]
enum MenuOpt {
    Empty,
    Cancel,
    Item(Storage),
}

#[derive(Debug, Clone)]
pub enum TypedMenuOpt<T: Copy> {
    Empty,
    Cancel,
    Item(T),
}

impl<T: Copy> TypedMenuOpt<T> {
    pub fn item(&self) -> Option<T> {
        match self {
            TypedMenuOpt::Item(x) => Some(*x),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MenuEntry<T: Copy> {
    pub coord: [usize; 2],
    pub opt: TypedMenuOpt<T>,
    pub frame: Option<Frame>,
    pub hovered: bool,
    pub confirmed: bool,
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub viewport: [usize; 2],
    pub scroll: [usize; 2],
    pub type_id: TypeId,

    items: Vec<MenuOpt>,
    columns: usize,

    cursor_pos: Cell<[usize; 2]>,
    confirm_pressed: bool,
    mouse_pos: Option<[f32; 2]>,
    mouse_clicked: bool,
    mouse_active: bool,
}

impl Menu {
    pub fn new<T: Copy + 'static, I: IntoIterator<Item = T>>(
        items: I,
        columns: usize,
        viewport: Option<[usize; 2]>,
    ) -> Self {
        assert!(std::mem::size_of::<T>() <= std::mem::size_of::<Storage>());

        let type_id = TypeId::of::<T>();
        let items: Vec<MenuOpt> = items
            .into_iter()
            .map(|item| {
                MenuOpt::Item(unsafe {
                    let src = &item as *const T as *const Storage;
                    let data: Storage = src.read();
                    data
                })
            })
            .collect();

        assert!(items.len() % columns == 0);

        Menu {
            viewport: viewport.unwrap_or([columns, items.len() / columns]),
            scroll: [0, 0],
            type_id,
            items,
            columns,

            cursor_pos: Cell::new([0, 0]),
            confirm_pressed: false,
            mouse_pos: None,
            mouse_clicked: false,
            mouse_active: false,
        }
    }

    pub fn new_with_empties<T: Copy + 'static, I: IntoIterator<Item = Option<T>>>(
        items: I,
        columns: usize,
        viewport: Option<[usize; 2]>,
    ) -> Self {
        assert!(std::mem::size_of::<T>() <= std::mem::size_of::<Storage>());

        let type_id = TypeId::of::<T>();
        let items: Vec<MenuOpt> = items
            .into_iter()
            .map(|item| match item {
                Some(item) => MenuOpt::Item(unsafe {
                    let src = &item as *const T as *const Storage;
                    let data: Storage = src.read();
                    data
                }),
                None => MenuOpt::Empty,
            })
            .collect();

        assert!(items.len() % columns == 0);

        Menu {
            viewport: viewport.unwrap_or([columns, items.len() / columns]),
            scroll: [0, 0],
            type_id,
            items,
            columns,

            cursor_pos: Cell::new([0, 0]),
            confirm_pressed: false,
            mouse_pos: None,
            mouse_clicked: false,
            mouse_active: false,
        }
    }

    pub fn with_cancel(mut self) -> Self {
        self.items.push(MenuOpt::Cancel);
        self
    }

    pub fn with_cancel_prefixed(mut self) -> Self {
        self.items.insert(0, MenuOpt::Cancel);
        self
    }

    pub fn rows(&self) -> usize {
        self.items.len() / self.columns
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn max_scroll(&self) -> [usize; 2] {
        let [vx, vy] = self.viewport;
        let [w, h] = [self.columns, self.rows()];
        [w - vx, h - vy]
    }

    pub fn coord_in_view(&self, coord: [usize; 2]) -> bool {
        let [x, y] = coord;
        let [w, h] = self.viewport;
        let [sx, sy] = self.scroll;
        x >= sx && x < (sx + w) && y >= sy && y < (sy + h)
    }

    // TODO: feedback - did we bonk?
    pub fn interact(
        &mut self,
        direction_input: [isize; 2],
        confirm_pressed: bool,
        mouse_pos: Option<[f32; 2]>,
        mouse_clicked: bool,
    ) {
        use std::cmp::{max, min};

        let mouse_moved = mouse_pos != self.mouse_pos;
        let cursor_moved = direction_input != [0, 0];
        self.mouse_pos = mouse_pos.or(self.mouse_pos);

        if cursor_moved {
            let [dx, dy] = direction_input;
            let [cx, cy] = self.cursor_pos.get();
            let [mut cx, mut cy] = [cx as isize, cy as isize];
            cx += dx;
            cy += dy;

            let [w, h] = [self.columns as isize, self.rows() as isize];
            // TODO: Check for bonk here
            cx = min(max(0, cx), w - 1);
            cy = min(max(0, cy), h - 1);

            let [sx, sy] = self.scroll;
            let [vx, vy] = self.viewport;
            let [min_x, min_y] = [sx as isize, sy as isize];
            let [max_x, max_y] = [(vx + sx) as isize, (vy + sy) as isize];

            let [mut sx, mut sy] = [sx as isize, sy as isize];
            if cx < min_x {
                sx = cx;
            }
            if cx >= max_x {
                sx += cx - max_x;
            }
            if cy < min_y {
                sy = cy;
            }
            if cy >= max_y {
                sy += cy - max_y + 1;
            }
            self.scroll = [sx as usize, sy as usize];

            self.cursor_pos.set([cx as usize, cy as usize]);
        }

        if cursor_moved {
            self.mouse_active = false;
        } else if mouse_moved && self.mouse_pos.is_some() {
            self.mouse_active = true;
        }

        self.confirm_pressed = confirm_pressed;
        self.mouse_clicked = mouse_clicked;
    }

    pub fn process<'a, T: Copy + 'static>(
        &'a self,
        frames: &'a [Frame],
    ) -> impl Iterator<Item = MenuEntry<T>> + '_ {
        assert_eq!(TypeId::of::<T>(), self.type_id);

        let mut frame_index = 0;
        let hovered_frame = frames
            .iter()
            .enumerate()
            .find(|(_, frame)| {
                self.mouse_pos.is_some() && frame.contains_point(self.mouse_pos.unwrap())
            })
            .map(|(i, _)| i);
        let mut any_hovered = false;

        self.items.iter().enumerate().map(move |(i, item)| {
            let y = i / self.columns;
            let x = i % self.columns;
            let coord = [x, y];

            let (frame, frame_hovered) = match self.coord_in_view(coord) {
                true => {
                    let frame = frames.get(frame_index).copied();
                    let frame_hovered = hovered_frame == Some(frame_index);
                    frame_index += 1;
                    (frame, frame_hovered)
                }
                false => (None, false),
            };

            if self.mouse_active && frame_hovered {
                self.cursor_pos.set(coord)
            }

            let hovered = match self.mouse_active {
                true => {
                    frame_hovered || (hovered_frame.is_none() && self.cursor_pos.get() == coord)
                }
                false => self.cursor_pos.get() == coord,
            };

            let hovered = hovered && !any_hovered;
            any_hovered |= hovered;

            let confirmed = (hovered && self.confirm_pressed)
                || (self.mouse_active && self.mouse_clicked && frame_hovered);

            MenuEntry {
                coord,
                opt: match item {
                    MenuOpt::Empty => TypedMenuOpt::Empty,
                    MenuOpt::Cancel => TypedMenuOpt::Cancel,
                    MenuOpt::Item(value) => unsafe {
                        let p = value as *const Storage as *const T;
                        TypedMenuOpt::Item(p.read())
                    },
                },
                frame,
                hovered,
                confirmed,
            }
        })
    }

    pub fn enumerate<T: Copy + 'static>(&self) -> impl Iterator<Item = ([usize; 2], T)> + '_ {
        assert_eq!(TypeId::of::<T>(), self.type_id);

        self.items.iter().enumerate().filter_map(|(i, item)| {
            let y = i / self.columns;
            let x = i % self.columns;
            match item {
                MenuOpt::Empty | MenuOpt::Cancel => None,
                MenuOpt::Item(value) => Some(([x, y], unsafe {
                    let p = value as *const Storage as *const T;
                    p.read()
                })),
            }
        })
    }

    pub fn enumerate_fully<T: Copy + 'static>(
        &self,
    ) -> impl Iterator<Item = ([usize; 2], TypedMenuOpt<T>)> + '_ {
        assert_eq!(TypeId::of::<T>(), self.type_id);

        self.items.iter().enumerate().map(|(i, item)| {
            let y = i / self.columns;
            let x = i % self.columns;
            (
                [x, y],
                match item {
                    MenuOpt::Empty => TypedMenuOpt::Empty,
                    MenuOpt::Cancel => TypedMenuOpt::Cancel,
                    MenuOpt::Item(value) => unsafe {
                        let p = value as *const Storage as *const T;
                        TypedMenuOpt::Item(p.read())
                    },
                },
            )
        })
    }

    pub fn enumerate_visible<T: Copy + 'static>(
        &self,
    ) -> impl Iterator<Item = ([usize; 2], T)> + '_ {
        self.enumerate()
            .filter(|(coord, _)| self.coord_in_view(*coord))
    }

    pub fn enumerate_visible_fully<T: Copy + 'static>(
        &self,
    ) -> impl Iterator<Item = ([usize; 2], TypedMenuOpt<T>)> + '_ {
        self.enumerate_fully()
            .filter(|(coord, _)| self.coord_in_view(*coord))
    }
}
