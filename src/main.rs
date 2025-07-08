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
    // 创建新玩家
    pub fn new(x: i32, y: i32) -> Self {
        Player {
            x,
            y,
            velocity: 0.0,
            alive: true,
        }
    }

    // 移动玩家
    pub fn try_move(&mut self) -> Option<()> {
        if !self.alive { 
            return None; //死亡时不能移动
        }
        // 重力加速度
        if self.velocity < 2.0 {
            self.velocity += 0.2;
        }

        self.x += 1;
        self.y += self.velocity as i32;

        // 是否撞到顶部
        if self.y < 0 {
            self.y = 0;
            self.alive = false;
            return None;
        }
        Some(())
    }

    // 上升
    pub fn flap(&mut self) -> Result<(), PlayerError> { //死亡或下落太快时不能向上移动
        if !self.alive {
            return Err(PlayerError::AlreadyDead);
        }
        if self.velocity > 5.0 {
            return Err(PlayerError::FallingTooFast);
        }

        self.velocity = -2.0;
        Ok(())
    }

    // 渲染玩家
    pub fn render(&mut self, ctx: &mut BTerm) {
        if !self.alive {
            return;
        }
        ctx.set(0, self.y, YELLOW, BLACK, to_cp437('@'));
    }

    // 重置玩家状态
    pub fn reset(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
        self.velocity = 0.0;
        self.alive = true;
    }

    // 获取玩家位置
    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    // 检查是否存活
    pub fn is_alive(&self) -> bool {
        self.alive
    }

    // 玩家死亡
    pub fn kill(&mut self) {
        self.alive = false;
    }
}

// 最高分记录
struct HighScore {
    score: i32,
}

// 游戏主状态
struct State {
    player: Player,
    frame_time: f32,
    mode: GameMode,
    obstacles: Vec<Obstacle>, // 障碍物列表
    score: i32,
    obstacle_receiver: mpsc::Receiver<Obstacle>,
    high_score: Arc<Mutex<HighScore>>,
}

// 游戏主循环接口
impl GameState for State {
    fn tick(&mut self, ctx: &mut BTerm) {
        match self.mode { // 主菜单、游戏中、结束
            GameMode::Menu => self.main_menu(ctx),
            GameMode::Playing => self.play(ctx),
            GameMode::End => self.dead(ctx),
        }
    }
}

impl State {
    // 创建新游戏状态
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();  // 创建障碍物生成通道
        // 初始化共享高分
        let high_score = Arc::new(Mutex::new(HighScore { score: 0 }));
        
        let mut obstacles = Vec::new(); // 生成初始障碍物
        let mut rng = RandomNumberGenerator::new();
        let mut x = SCREEN_WIDTH;
        for _ in 0..3 {  // 障碍物间隙位置、大小
            let gap_y = rng.range(10, 40);
            let size = rng.range(10, 20);
            obstacles.push(Obstacle { x, gap_y, size });
            x += SCREEN_WIDTH / 2;  // 每个障碍物间隔半屏
        }

        // 创建障碍物生成线程
        let thread_x = x;
        thread::spawn(move || {
            let mut rng = RandomNumberGenerator::new();
            let mut x = thread_x;
            loop {
                let gap_y = rng.range(10, 40);
                let size = rng.range(10, 20);
                if sender.send(Obstacle { x, gap_y, size }).is_err() {  // 发送新障碍物到通道
                    break;
                }
                x += SCREEN_WIDTH / 2;
                thread::sleep(std::time::Duration::from_millis(1500));
            }
        });

        // 返回初始游戏状态
        State {
            player: Player::new(5, 25),  // 初始玩家位置
            frame_time: 0.0,
            mode: GameMode::Menu,  // 初始状态为主菜单
            obstacles,
            score: 0,
            obstacle_receiver: receiver,
            high_score,
        }
    }

    fn main_menu(&mut self, ctx: &mut BTerm) {
        ctx.cls();  //清屏
        ctx.print_centered(15, "■ WELCOME TO FLAPPY BIRD! ■");
        ctx.print_centered(40, "Avoid obstacles and press SPACE to flap your wings");
        ctx.print_centered(20, "(P) Play Game"); 
        ctx.print_centered(22, "(Q) Quit Game");

        if let Some(key) = ctx.key {
            match key {
                VirtualKeyCode::P => self.restart(),
                VirtualKeyCode::Q => ctx.quitting = true,
                _ => {}
            }
        }
    }

    // 游戏主逻辑
    fn play(&mut self, ctx: &mut BTerm) {
        ctx.cls_bg(LIGHT_BLUE);  // 蓝色背景
        self.frame_time += ctx.frame_time_ms;
        
        // 帧定时器控制
        if self.frame_time > FRAME_DURATION {
            self.frame_time = 0.0;
            if self.player.try_move().is_none() {
                self.mode = GameMode::End;
            }
        }
        
        // 空格键拍打翅膀向上飞
        if let Some(VirtualKeyCode::Space) = ctx.key {
            if let Err(e) = self.player.flap() {
                match e {
                    PlayerError::AlreadyDead => println!("Player is already dead"),
                    PlayerError::FallingTooFast => println!("Can't flap while falling too fast"),
                }
            }
        }
        
        // 渲染玩家和UI
        self.player.render(ctx);
        ctx.print(0, 1, &format!("Score: {}", self.score));

        // 渲染所有障碍物
        for obstacle in &mut self.obstacles {
            obstacle.render(ctx, self.player.x);
        }

        // 移除屏幕外的障碍物
        self.obstacles.retain(|o| o.x - self.player.x > -20);

        // 计分(通过障碍物)
        if let Some(first_obstacle) = self.obstacles.first() {
            if self.player.x > first_obstacle.x {
                self.score += 1;
                self.obstacles.remove(0);
            }
        }

        // 从通道接收新生成的障碍物
        while let Ok(new_obstacle) = self.obstacle_receiver.try_recv() {
            self.obstacles.push(new_obstacle);
        }

        // 碰撞检测（超出屏幕、碰到障碍物）
        let player_dead = self.player.y > SCREEN_HEIGHT || 
            self.obstacles.iter().any(|o| o.hit_obstacle(&self.player));
        
        if player_dead {
            self.mode = GameMode::End;
            // 更新最高分
            let mut high_score = self.high_score.lock().unwrap();
            if self.score > high_score.score {
                high_score.score = self.score;
            }
        }
    }

    // 游戏结束界面
    fn dead(&mut self, ctx: &mut BTerm) {
        let high_score = self.high_score.lock().unwrap().score;
        ctx.cls();
        ctx.print_centered(15, "You're dead! >.<");
        ctx.print_centered(20, &format!("You earned {} points! (Highest: {})", self.score, high_score));
        ctx.print_centered(25, "(P) Play Again");
        ctx.print_centered(27, "(Q) Quit Game");

        // 处理按键输入（P/Q 重新开始/退出）
        if let Some(key) = ctx.key {
            match key {
                VirtualKeyCode::P => self.restart(),
                VirtualKeyCode::Q => ctx.quitting = true,
                _ => {}
            }
        }
    }

    // 重置游戏状态
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

// 障碍物结构体
struct Obstacle {
    x: i32,
    gap_y: i32,  // 间隙中心Y坐标
    size: i32,  // 间隙大小
}

impl Obstacle {
    // 渲染障碍物
    fn render(&mut self, ctx: &mut BTerm, player_x: i32) {
        let screen_x = self.x - player_x;  // 计算屏幕相对坐标
        let half_size = self.size / 2;

        for y in 0..self.gap_y - half_size {  // 渲染上部障碍物
            ctx.set(screen_x, y, RED, BLACK, to_cp437('|'));
        }
        
        for y in self.gap_y + half_size..SCREEN_HEIGHT {  // 渲染下部障碍物
            ctx.set(screen_x, y, RED, BLACK, to_cp437('|'));
        }
    }

    // 碰撞检测
    fn hit_obstacle(&self, player: &Player) -> bool {
        let half_size = self.size / 2;
        let does_x_match = player.x == self.x;  // X坐标匹配
        let player_above_gap = player.y < self.gap_y - half_size;  // 玩家在间隙上方
        let player_below_gap = player.y > self.gap_y + half_size;  // 玩家在间隙下方
        does_x_match && (player_above_gap || player_below_gap)
    }
}

fn main() -> BError {
    // 创建游戏窗口
    let context = BTermBuilder::simple80x50()
        .with_title("Flappy Bird")
        .build()?;

    // 启动游戏主循环
    main_loop(context, State::new())
}