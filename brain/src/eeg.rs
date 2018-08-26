// This file is a hot mess, don't look at it please :)

use collect::ExtendRotation3;
use crossbeam_channel;
use graphics::types::Color;
use graphics::Transformed;
use nalgebra::{Rotation3, Vector3};
use piston_window::{
    clear, ellipse, rectangle, text, AdvancedWindow, Ellipse, Glyphs, OpenGL, PistonWindow,
    Position, Rectangle, TextureSettings, WindowSettings,
};
use rlbot;
use simulate::rl;
use std::mem;
use std::path::PathBuf;
use std::thread;

pub struct EEG {
    tx: Option<crossbeam_channel::Sender<ThreadMessage>>,
    join_handle: Option<thread::JoinHandle<()>>,
    draw_list: Vec<Drawable>,
}

impl EEG {
    pub fn new() -> EEG {
        let (tx, rx) = crossbeam_channel::unbounded();
        let join_handle = thread::spawn(|| thread(rx));
        EEG {
            tx: Some(tx),
            join_handle: Some(join_handle),
            draw_list: Vec::new(),
        }
    }
}

impl Drop for EEG {
    fn drop(&mut self) {
        drop(self.tx.take().unwrap());
        self.join_handle.take().unwrap().join().unwrap();
    }
}

impl EEG {
    pub fn draw(&mut self, drawable: Drawable) {
        self.draw_list.push(drawable);
    }

    pub fn show(&mut self, packet: &rlbot::LiveDataPacket) {
        let draw_list = mem::replace(&mut self.draw_list, Vec::new());
        self.tx.as_ref().unwrap().send(ThreadMessage::Draw(
            packet.clone(),
            draw_list.into_boxed_slice(),
        ));
    }
}

pub enum Drawable {
    GhostBall(Vector3<f32>),
    GhostCar(Vector3<f32>, Rotation3<f32>),
    Print(String, Color),
}

impl Drawable {
    pub fn print(text: impl Into<String>, color: Color) -> Drawable {
        Drawable::Print(text.into(), color)
    }
}

pub mod color {
    pub const TRANSPARENT: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
    pub const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    pub const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    pub const PITCH: [f32; 4] = [0.0, 0.2, 0.0, 1.0];
    pub const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    pub const ORANGE: [f32; 4] = [1.0, 0.5, 0.0, 1.0];
    pub const ORANGE_DARK: [f32; 4] = [0.5, 0.25, 0.0, 1.0];
    pub const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
    pub const YELLOW: [f32; 4] = [1.0, 1.0, 0.0, 1.0];
    pub const BLUE: [f32; 4] = [0.5, 0.5, 1.0, 1.0];
    pub const BLUE_DARK: [f32; 4] = [0.25, 0.25, 0.5, 1.0];
}

enum ThreadMessage {
    Draw(rlbot::LiveDataPacket, Box<[Drawable]>),
}

fn thread(rx: crossbeam_channel::Receiver<ThreadMessage>) {
    let mut window: PistonWindow = WindowSettings::new("Formula nOne", (640, 640))
        .opengl(OpenGL::V3_2)
        .build()
        .unwrap();
    window.set_position(Position { x: 1920, y: 10 });

    let path = PathBuf::from(r"C:\Windows\Fonts\calibri.ttf");
    let factory = window.factory.clone();
    let mut glyphs = Glyphs::new(path, factory, TextureSettings::new()).unwrap();

    loop {
        let event = match window.next() {
            Some(e) => e,
            None => break,
        };

        match rx.recv() {
            None => break, // The channel was closed, so exit the thread.
            Some(ThreadMessage::Draw(packet, drawables)) => {
                window.draw_2d(&event, |c, g| {
                    const GOAL_DEPTH: f64 = 900.0; // This was just estimated visually.
                    let car_rect = rectangle::rectangle_by_corners(-100.0, -50.0, 100.0, 50.0);
                    let ball_rect = ellipse::circle(0.0, 0.0, 92.0);

                    const SCALE: f64 = 0.05;
                    const OUTLINE_RADIUS: f64 = 0.5 / SCALE;

                    clear(color::BLACK, g);

                    let transform = c.transform.scale(SCALE, SCALE).trans(4200.0, 6200.0);

                    rectangle(
                        color::PITCH,
                        rectangle::rectangle_by_corners(
                            -rl::FIELD_MAX_X as f64,
                            -rl::FIELD_MAX_Y as f64,
                            rl::FIELD_MAX_X as f64,
                            rl::FIELD_MAX_Y as f64,
                        ),
                        transform,
                        g,
                    );
                    rectangle(
                        color::BLUE_DARK,
                        rectangle::rectangle_by_corners(
                            -rl::GOALPOST_X as f64,
                            -rl::FIELD_MAX_Y as f64,
                            rl::GOALPOST_X as f64,
                            -rl::FIELD_MAX_Y as f64 - GOAL_DEPTH,
                        ),
                        transform,
                        g,
                    );
                    rectangle(
                        color::ORANGE_DARK,
                        rectangle::rectangle_by_corners(
                            -rl::GOALPOST_X as f64,
                            rl::FIELD_MAX_Y as f64,
                            rl::GOALPOST_X as f64,
                            rl::FIELD_MAX_Y as f64 + GOAL_DEPTH,
                        ),
                        transform,
                        g,
                    );

                    for car in packet.cars() {
                        rectangle(
                            if car.Team == 0 {
                                color::BLUE
                            } else {
                                color::ORANGE
                            },
                            car_rect,
                            transform
                                .trans(car.Physics.Location.X as f64, car.Physics.Location.Y as f64)
                                .rot_rad(car.Physics.Rotation.Yaw as f64),
                            g,
                        );
                    }

                    ellipse(
                        color::WHITE,
                        ball_rect,
                        transform.trans(
                            packet.GameBall.Physics.Location.X as f64,
                            packet.GameBall.Physics.Location.Y as f64,
                        ),
                        g,
                    );

                    let mut prints = Vec::new();

                    for drawable in drawables.into_iter() {
                        match drawable {
                            Drawable::GhostBall(loc) => {
                                Ellipse::new_border(color::WHITE, OUTLINE_RADIUS).draw(
                                    ball_rect,
                                    &Default::default(),
                                    transform.trans(loc.x as f64, loc.y as f64),
                                    g,
                                );
                            }
                            Drawable::GhostCar(loc, rot) => {
                                Rectangle::new_border(color::WHITE, OUTLINE_RADIUS).draw(
                                    car_rect,
                                    &Default::default(),
                                    transform
                                        .trans(loc.x as f64, loc.y as f64)
                                        .rot_rad(rot.yaw() as f64),
                                    g,
                                );
                            }
                            Drawable::Print(txt, color) => {
                                prints.push((txt, *color));
                            }
                        }
                    }

                    let mut y = 20.0;
                    for (txt, color) in prints.into_iter() {
                        text(color, 16, &txt, &mut glyphs, c.transform.trans(420.0, y), g).unwrap();
                        y += 20.0;
                    }
                });
            }
        }
    }
}
