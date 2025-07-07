use bracket_lib::prelude::*;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

enum GameMode {
    Menu,
    Playing,
    End,
}

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;
const FRAME_DURATION: f32 = 75.0;

#[derive(Debug)]
pub enum PlayerError {
    AlreadyDead,
    FallingTooFast,
}

struct Player {
    x: i32,
    y: i32,
    velocity: f32,
    alive: bool,
}

impl Player {
    pub fn new(x: i32, y: i32) -> Self {
        Player {
            x,
            y,
            velocity: 0.0,
            alive: true,
        }
    }

    pub fn try_move(&mut self) -> Option<()> {
        if !self.alive {
            return None;
        }
        if self.velocity < 2.0 {
            self.velocity += 0.2;
        }

        self.x += 1;
        self.y += self.velocity as i32;

        if self.y < 0 {
            self.y = 0;
            self.alive = false;
            return None;
        }
        Some(())
    }

    pub fn flap(&mut self) -> Result<(), PlayerError> {
        if !self.alive {
            return Err(PlayerError::AlreadyDead);
        }
        if self.velocity > 5.0 {
            return Err(PlayerError::FallingTooFast);
        }

        self.velocity = -2.0;
        Ok(())
    }

    pub fn render(&mut self, ctx: &mut BTerm) {
        if !self.alive {
            return;
        }
        let glyph = match self.velocity {
            v if v < -1.0 => to_cp437('↑'),
            v if v > 1.0 => to_cp437('↓'),
            _ => to_cp437('@'),
        };

        ctx.set(0, self.y, YELLOW, BLACK, glyph);
    }

    pub fn reset(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
        self.velocity = 0.0;
        self.alive = true;
    }

    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    pub fn kill(&mut self) {
        self.alive = false;
    }
}

struct HighScore {
    score: i32,
}

struct State {
    player: Player,
    frame_time: f32,
    mode: GameMode,
    obstacles: Vec<Obstacle>,
    score: i32,
    obstacle_receiver: mpsc::Receiver<Obstacle>,
    high_score: Arc<Mutex<HighScore>>,
}

impl GameState for State {
    fn tick(&mut self, ctx: &mut BTerm) {
        match self.mode {
            GameMode::Menu => self.main_menu(ctx),
            GameMode::Playing => self.play(ctx),
            GameMode::End => self.dead(ctx),
        }
    }
}

impl State {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        let high_score = Arc::new(Mutex::new(HighScore { score: 0 }));
        
        let mut obstacles = Vec::new();
        let mut rng = RandomNumberGenerator::new();
        let mut x = SCREEN_WIDTH;
        for _ in 0..3 {
            let gap_y = rng.range(10, 40);
            let size = rng.range(10, 20);
            obstacles.push(Obstacle { x, gap_y, size });
            x += SCREEN_WIDTH / 2;
        }

        let thread_x = x;
        thread::spawn(move || {
            let mut rng = RandomNumberGenerator::new();
            let mut x = thread_x;
            loop {
                let gap_y = rng.range(10, 40);
                let size = rng.range(10, 20);
                if sender.send(Obstacle { x, gap_y, size }).is_err() {
                    break;
                }
                x += SCREEN_WIDTH / 2;
                thread::sleep(std::time::Duration::from_millis(1500));
            }
        });

        State {
            player: Player::new(5, 25),
            frame_time: 0.0,
            mode: GameMode::Menu,
            obstacles,
            score: 0,
            obstacle_receiver: receiver,
            high_score,
        }
    }

    fn main_menu(&mut self, ctx: &mut BTerm) {
        ctx.cls();
        ctx.print_centered(5, "Welcome to Flappy Bird");
        ctx.print_centered(8, "(P) Play Game");
        ctx.print_centered(9, "(Q) Quit Game");

        if let Some(key) = ctx.key {
            match key {
                VirtualKeyCode::P => self.restart(),
                VirtualKeyCode::Q => ctx.quitting = true,
                _ => {}
            }
        }
    }

    fn play(&mut self, ctx: &mut BTerm) {
        ctx.cls_bg(NAVY);
        self.frame_time += ctx.frame_time_ms;
        
        if self.frame_time > FRAME_DURATION {
            self.frame_time = 0.0;
            if self.player.try_move().is_none() {
                self.mode = GameMode::End;
            }
        }
        
        if let Some(VirtualKeyCode::Space) = ctx.key {
            if let Err(e) = self.player.flap() {
                match e {
                    PlayerError::AlreadyDead => println!("Player is already dead"),
                    PlayerError::FallingTooFast => println!("Can't flap while falling too fast"),
                }
            }
        }
        
        self.player.render(ctx);
        ctx.print(0, 0, "Press SPACE to flap");
        ctx.print(0, 1, &format!("Score: {}", self.score));

        for obstacle in &mut self.obstacles {
            obstacle.render(ctx, self.player.x);
        }

        self.obstacles.retain(|o| o.x - self.player.x > -20);

        if let Some(first_obstacle) = self.obstacles.first() {
            if self.player.x > first_obstacle.x {
                self.score += 1;
                self.obstacles.remove(0);
            }
        }

        while let Ok(new_obstacle) = self.obstacle_receiver.try_recv() {
            self.obstacles.push(new_obstacle);
        }

        let player_dead = self.player.y > SCREEN_HEIGHT || 
            self.obstacles.iter().any(|o| o.hit_obstacle(&self.player));
        
        if player_dead {
            self.mode = GameMode::End;
            let mut high_score = self.high_score.lock().unwrap();
            if self.score > high_score.score {
                high_score.score = self.score;
            }
        }
    }

    fn dead(&mut self, ctx: &mut BTerm) {
        let high_score = self.high_score.lock().unwrap().score;
        ctx.cls();
        ctx.print_centered(5, "You are dead!");
        ctx.print_centered(6, &format!("You earned {} points", self.score));
        ctx.print_centered(7, &format!("High Score: {}", high_score));
        ctx.print_centered(8, "(P) Play Again");
        ctx.print_centered(9, "(Q) Quit Game");

        if let Some(key) = ctx.key {
            match key {
                VirtualKeyCode::P => self.restart(),
                VirtualKeyCode::Q => ctx.quitting = true,
                _ => {}
            }
        }
    }

    fn restart(&mut self) {
        // 重置玩家状态
        self.player = Player::new(5, 25);
        self.frame_time = 0.0;
        self.mode = GameMode::Playing;
        self.score = 0;
        
        // 清空现有障碍物
        self.obstacles.clear();
        
        // 创建新的通道
        let (sender, receiver) = mpsc::channel();
        self.obstacle_receiver = receiver;
        
        // 生成初始障碍物
        let mut rng = RandomNumberGenerator::new();
        let mut x = SCREEN_WIDTH;
        for _ in 0..3 {
            let gap_y = rng.range(10, 40);
            let size = rng.range(10, 20);
            self.obstacles.push(Obstacle { x, gap_y, size });
            x += SCREEN_WIDTH / 2;
        }
        
        // 启动新的障碍物生成线程
        let thread_x = x;
        thread::spawn(move || {
            let mut rng = RandomNumberGenerator::new();
            let mut x = thread_x;
            loop {
                let gap_y = rng.range(10, 40);
                let size = rng.range(10, 20);
                if sender.send(Obstacle { x, gap_y, size }).is_err() {
                    break;
                }
                x += SCREEN_WIDTH / 2;
                thread::sleep(std::time::Duration::from_millis(1500));
            }
        });
    }
}

struct Obstacle {
    x: i32,
    gap_y: i32,
    size: i32,
}

impl Obstacle {
    fn render(&mut self, ctx: &mut BTerm, player_x: i32) {
        let screen_x = self.x - player_x;
        let half_size = self.size / 2;

        for y in 0..self.gap_y - half_size {
            ctx.set(screen_x, y, RED, BLACK, to_cp437('|'));
        }
        
        for y in self.gap_y + half_size..SCREEN_HEIGHT {
            ctx.set(screen_x, y, RED, BLACK, to_cp437('|'));
        }
    }

    fn hit_obstacle(&self, player: &Player) -> bool {
        let half_size = self.size / 2;
        let does_x_match = player.x == self.x;
        let player_above_gap = player.y < self.gap_y - half_size;
        let player_below_gap = player.y > self.gap_y + half_size;
        does_x_match && (player_above_gap || player_below_gap)
    }
}

fn main() -> BError {
    let context = BTermBuilder::simple80x50()
        .with_title("Flappy Bird")
        .build()?;

    main_loop(context, State::new())
}