//! The shmup example's enemy bullets in Rust: a native library implementing the same interface
//! as the shmup's `bullets.luau`, documented at the top of that file, so the game runs its bullets
//! in either. It adds the global table `danmaku`, whose functions are the interface's.
//!
//! Its arithmetic is the Luau version's, step for step in `f64`, so from the same calls both
//! produce the same bullets in the same order, to the bit. What it gains is everything around the
//! arithmetic: bullets packed in one `Vec` rather than in arrays of Luau values, a loop compiled
//! ahead of time, and drawing every bullet with a few calls to the engine rather than a call from
//! Luau for each.
//!
//! The shmup lists it as an optional library, so it runs with or without it. Build it and copy it
//! into the shmup's `native` folder to run with it:
//!
//! ```text
//! cargo build --release --package native-danmaku
//! ```
//!
//! then `target/release/danmaku.dll` (`libdanmaku.so` on Linux, `libdanmaku.dylib` on macOS) to
//! `examples/shmup/native/`.

use wingine_native::{
    Call, Canvas, Color, Context, Error, FromArgs, IntoResult, IntoResults, Library, Rect,
    Registry, Result, Sprite, SpriteDraw,
};

/// A look for bullets, registered with `danmaku.kind`.
struct Kind {
    sprite: Sprite,
    size: [f32; 2],
    /// Half the sprite's size, to center it on the bullet.
    half: [f64; 2],
    radius: f64,
    rotates: bool,
}

impl Kind {
    /// How `bullet`, one of this kind, is drawn.
    fn draw(&self, bullet: &Bullet) -> SpriteDraw {
        let [width, height] = self.size;
        SpriteDraw {
            dest: Rect::new(
                (bullet.x - self.half[0]) as f32,
                (bullet.y - self.half[1]) as f32,
                width,
                height,
            ),
            angle: if self.rotates {
                bullet.angle as f32
            } else {
                0.0
            },
            tint: Color::WHITE,
        }
    }
}

#[derive(Clone, Copy)]
struct Bullet {
    x: f64,
    y: f64,
    /// Radians clockwise from the right, and pixels a second.
    angle: f64,
    speed: f64,
    /// What the speed and angle change by each second.
    acceleration: f64,
    turn: f64,
    min_speed: f64,
    max_speed: f64,
    radius: f64,
    /// Its kind's index in `Danmaku::kinds`.
    kind: usize,
    grazed: bool,
}

/// The bullets of a game.
struct Danmaku {
    kinds: Vec<Kind>,
    bullets: Vec<Bullet>,
    /// The area bullets live in: its left, top, right and bottom.
    bounds: [f64; 4],
    /// The sprites a call to the engine draws, kept to save allocating them each frame.
    draws: Vec<SpriteDraw>,
}

/// What `danmaku.spawn` takes, with what it leaves out filled in.
struct Spawn {
    kind: usize,
    x: f64,
    y: f64,
    angle: f64,
    speed: f64,
    count: u32,
    spacing: f64,
    acceleration: f64,
    turn: f64,
    min_speed: f64,
    max_speed: f64,
}

impl FromArgs for Spawn {
    fn from_args(call: &Call) -> Result<Self> {
        let or =
            |index, default| Ok::<_, Error>(call.arg::<Option<f64>>(index)?.unwrap_or(default));
        Ok(Self {
            kind: call.arg(0)?,
            x: call.arg(1)?,
            y: call.arg(2)?,
            angle: call.arg(3)?,
            speed: call.arg(4)?,
            count: call.arg::<Option<u32>>(5)?.unwrap_or(1),
            spacing: or(6, 0.0)?,
            acceleration: or(7, 0.0)?,
            turn: or(8, 0.0)?,
            min_speed: or(9, f64::NEG_INFINITY)?,
            max_speed: or(10, f64::INFINITY)?,
        })
    }
}

/// Everything about a bullet, as `danmaku.bullet` returns it and `danmaku.add` takes it: its
/// kind, counting from 1, its numbers, and whether it grazed the player.
struct Snapshot {
    kind: usize,
    x: f64,
    y: f64,
    angle: f64,
    speed: f64,
    acceleration: f64,
    turn: f64,
    min_speed: f64,
    max_speed: f64,
    grazed: bool,
}

impl FromArgs for Snapshot {
    fn from_args(call: &Call) -> Result<Self> {
        Ok(Self {
            kind: call.arg(0)?,
            x: call.arg(1)?,
            y: call.arg(2)?,
            angle: call.arg(3)?,
            speed: call.arg(4)?,
            acceleration: call.arg(5)?,
            turn: call.arg(6)?,
            min_speed: call.arg(7)?,
            max_speed: call.arg(8)?,
            grazed: call.arg(9)?,
        })
    }
}

impl IntoResults for Snapshot {
    fn push_all(self, call: &Call) -> Result<()> {
        self.kind.push(call)?;
        for number in [
            self.x,
            self.y,
            self.angle,
            self.speed,
            self.acceleration,
            self.turn,
            self.min_speed,
            self.max_speed,
        ] {
            number.push(call)?;
        }
        self.grazed.push(call)
    }
}

/// `value` kept from `min` to `max`, as Luau's `math.clamp` keeps it.
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    let value = if value < min { min } else { value };
    if value > max { max } else { value }
}

impl Danmaku {
    /// The index in `kinds` of the kind numbered `kind`, after checking that a bullet of it may
    /// have these speed limits.
    fn kind(&self, kind: usize, min_speed: f64, max_speed: f64) -> Result<usize> {
        let index = kind
            .checked_sub(1)
            .filter(|&index| index < self.kinds.len())
            .ok_or_else(|| Error::new(format!("there's no bullet kind {kind}")))?;
        if min_speed > max_speed {
            return Err(Error::new("min_speed is more than max_speed"));
        }
        Ok(index)
    }

    fn spawn(&mut self, spawn: Spawn) -> Result<()> {
        let kind = self.kind(spawn.kind, spawn.min_speed, spawn.max_speed)?;
        let first = spawn.angle - spawn.spacing * (f64::from(spawn.count) - 1.0) / 2.0;
        let radius = self.kinds[kind].radius;
        self.bullets.extend((0..spawn.count).map(|i| Bullet {
            x: spawn.x,
            y: spawn.y,
            angle: first + spawn.spacing * f64::from(i),
            speed: spawn.speed,
            acceleration: spawn.acceleration,
            turn: spawn.turn,
            min_speed: spawn.min_speed,
            max_speed: spawn.max_speed,
            radius,
            kind,
            grazed: false,
        }));
        Ok(())
    }

    /// Bullet `index`, counting from 1.
    fn bullet(&self, index: usize) -> Result<Snapshot> {
        let bullet = index
            .checked_sub(1)
            .and_then(|i| self.bullets.get(i))
            .ok_or_else(|| Error::new(format!("there's no bullet {index}")))?;
        Ok(Snapshot {
            kind: bullet.kind + 1,
            x: bullet.x,
            y: bullet.y,
            angle: bullet.angle,
            speed: bullet.speed,
            acceleration: bullet.acceleration,
            turn: bullet.turn,
            min_speed: bullet.min_speed,
            max_speed: bullet.max_speed,
            grazed: bullet.grazed,
        })
    }

    /// Adds a bullet as `snapshot` says, after the others.
    fn add(&mut self, snapshot: Snapshot) -> Result<()> {
        let kind = self.kind(snapshot.kind, snapshot.min_speed, snapshot.max_speed)?;
        self.bullets.push(Bullet {
            x: snapshot.x,
            y: snapshot.y,
            angle: snapshot.angle,
            speed: snapshot.speed,
            acceleration: snapshot.acceleration,
            turn: snapshot.turn,
            min_speed: snapshot.min_speed,
            max_speed: snapshot.max_speed,
            radius: self.kinds[kind].radius,
            kind,
            grazed: snapshot.grazed,
        });
        Ok(())
    }

    /// Moves every bullet on by `dt` seconds, removing those that leave the bounds: the last
    /// bullet takes a removed one's place, and is moved next.
    fn step(&mut self, dt: f64) {
        let [left, top, right, bottom] = self.bounds;
        let mut i = 0;
        while i < self.bullets.len() {
            let bullet = &mut self.bullets[i];
            let speed = clamp(
                bullet.speed + bullet.acceleration * dt,
                bullet.min_speed,
                bullet.max_speed,
            );
            let angle = bullet.angle + bullet.turn * dt;
            let x = bullet.x + angle.cos() * speed * dt;
            let y = bullet.y + angle.sin() * speed * dt;
            if x < left || x > right || y < top || y > bottom {
                self.bullets.swap_remove(i);
            } else {
                bullet.speed = speed;
                bullet.angle = angle;
                bullet.x = x;
                bullet.y = y;
                i += 1;
            }
        }
    }

    /// Whether a bullet touches the circle at `x`, `y` of `radius`, and how many graze it for
    /// the first time.
    fn collide(&mut self, x: f64, y: f64, radius: f64, graze_radius: f64) -> (bool, u32) {
        let (mut hit, mut grazes) = (false, 0);
        for bullet in &mut self.bullets {
            let (dx, dy) = (bullet.x - x, bullet.y - y);
            let distance = dx * dx + dy * dy;
            let reach = graze_radius + bullet.radius;
            if distance < reach * reach {
                let touch = radius + bullet.radius;
                if distance < touch * touch {
                    hit = true;
                } else if !bullet.grazed {
                    bullet.grazed = true;
                    grazes += 1;
                }
            }
        }
        (hit, grazes)
    }

    /// Draws the bullets in order, as the Luau version does, with a call to the engine for each
    /// run of bullets of one kind.
    fn draw(&mut self, canvas: &mut Canvas) {
        for run in self.bullets.chunk_by(|a, b| a.kind == b.kind) {
            let kind = &self.kinds[run[0].kind];
            self.draws.clear();
            self.draws
                .extend(run.iter().map(|bullet| kind.draw(bullet)));
            canvas.draw_sprites(&kind.sprite, &self.draws);
        }
    }
}

impl Library for Danmaku {
    const NAME: &str = "danmaku";

    fn new(_ctx: &mut Context) -> Result<Self> {
        Ok(Self {
            kinds: Vec::new(),
            bullets: Vec::new(),
            bounds: [
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::INFINITY,
            ],
            draws: Vec::new(),
        })
    }

    fn register(registry: &mut Registry<Self>) {
        registry
            .function(
                "kind",
                |danmaku, _, (sprite, radius, rotates): (Sprite, f64, bool)| {
                    let size = sprite.size();
                    danmaku.kinds.push(Kind {
                        half: size.map(|side| f64::from(side) / 2.0),
                        size,
                        sprite,
                        radius,
                        rotates,
                    });
                    Ok(danmaku.kinds.len())
                },
            )
            .function(
                "set_bounds",
                |danmaku, _, (x, y, width, height, margin): (f64, f64, f64, f64, f64)| {
                    danmaku.bounds = [
                        x - margin,
                        y - margin,
                        x + width + margin,
                        y + height + margin,
                    ];
                    Ok(())
                },
            )
            .function("spawn", |danmaku, _, spawn: Spawn| danmaku.spawn(spawn))
            .function("step", |danmaku, _, dt: f64| {
                danmaku.step(dt);
                Ok(())
            })
            .function(
                "collide",
                |danmaku, _, (x, y, radius, graze_radius): (f64, f64, f64, f64)| {
                    Ok(danmaku.collide(x, y, radius, graze_radius))
                },
            )
            .function("clear", |danmaku, _, ()| {
                let cleared = danmaku.bullets.len();
                danmaku.bullets.clear();
                Ok(cleared)
            })
            .function("count", |danmaku, _, ()| Ok(danmaku.bullets.len()))
            .function("draw", |danmaku, call, ()| {
                danmaku.draw(call.canvas()?);
                Ok(())
            })
            .function("bullet", |danmaku, _, index: usize| danmaku.bullet(index))
            .function("add", |danmaku, _, snapshot: Snapshot| {
                danmaku.add(snapshot)
            })
            .luau(
                "positions",
                "local danmaku = ...
                function danmaku.positions(): { number }
                    local count = danmaku.count()
                    local positions = table.create(count * 2)
                    for i = 1, count do
                        local _, x, y = danmaku.bullet(i)
                        positions[i * 2 - 1], positions[i * 2] = x, y
                    end
                    return positions
                end",
            );
    }
}

wingine_native::export!(Danmaku);
