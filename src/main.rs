//! Based on ggez's asteroid blaster example
//! Modified for a more refined gameplay experience
extern crate ggez;

extern crate rand;

use ggez::graphics;
use ggez::conf;
use ggez::event::{self, EventHandler, Keycode, Mod};
use ggez::graphics::{Vector2, Point2};
use ggez::timer;
use ggez::{Context, ContextBuilder, GameResult};

use std::env;
use std::path;


mod actor;
mod game_structs;
mod networking;
mod net_structs;

use actor::Actor;
use game_structs::*;


const PLAYER_SHOT_TIME: f32 = 0.2;
const SHOT_SPEED: f32 = 1100.0;

use std::time::Duration;


/// Create a unit vector representing the
/// given angle (in radians)
fn vec_from_angle(angle: f32) -> Vector2 {
    let vx = angle.sin();
    let vy = angle.cos();
    Vector2::new(vx, vy)
}



/// Translates the world coordinate system, which
/// has Y pointing up and the origin at the center,
/// to the screen coordinate system, which has Y
/// pointing downward and the origin at the top-left,
fn world_to_screen_coords(screen_width: u32, screen_height: u32, point: Point2) -> Point2 {
    let width = screen_width as f32;
    let height = screen_height as f32;
    let x = point.x + width / 2.0;
    let y = height - (point.y + height / 2.0);
    Point2::new(x, y)
}

impl MainState {
    fn new(ctx: &mut Context) -> MainState {
        ctx.print_resource_stats();
        graphics::set_background_color(ctx, (0, 0, 0, 255).into());

        println!("Game resource path: {:?}", ctx.filesystem);

        print_instructions();

        let assets = Assets::new(ctx).expect("Failed to load assets. Terminating");
        let score_disp = graphics::Text::new(ctx, "score", &assets.font).expect("Failed to make text. Terminating");
        let level_disp = graphics::Text::new(ctx, "level", &assets.font).expect("Failed to make text. Terminating");

        let players = Vec::new();
        let rocks = Vec::new();

        let args: std::vec::Vec<String> = env::args().collect();
        let mut diff_mult = 1.0;
        if args.len() > 1 {
            diff_mult = args[1].parse().unwrap_or(1.0);
        }

        println!("Difficulty Multiplier: {:?}", diff_mult);

        let mut s = MainState {
            local_player_index: Some(0),
            local_input: InputState::default(),
            players: players,
            shots: Vec::new(),
            rocks: rocks,
            score: 0,
            assets,
            screen_width: ctx.conf.window_mode.width,
            screen_height: ctx.conf.window_mode.height,
            score_display: score_disp,
            level_display: level_disp,
            start_time: std::time::Instant::now(),
            curr_time: 0.0,
            difficulty_mult: diff_mult,
            play_sounds: PlaySounds::default(),
            connections: 0,
            local_shots_made: Vec::new(),
        };
       
        s.add_player();
        s.restart_game();
        s
    }

    fn get_local_player(&self) -> Option<&Player> {
        if let Some(index) = self.local_player_index {
            if self.players.len() > index {
                Some(&self.players[index])
            } else {
                None
            }
        } else {
            None
        }
    }

    fn is_server(&self) -> bool {
        self.local_player_index == Some(0)
    }

    fn add_player(&mut self) -> usize {
        let mut new_player = Player::create();
        let index = self.players.len();
        new_player.index = index as u32;
        self.players.push(new_player);
        index
    }

    fn spawn_shots(shots_ref: &mut Vec<Actor>, pos: &Vector2) {
        for i in -1..2 {
            let mut shot = Actor::create_shot();
            shot.pos = pos.clone();

            shot.velocity.x = (i as f32) * SHOT_SPEED / 3.0;
            shot.velocity.y = SHOT_SPEED;
            shots_ref.push(shot);
        }
    }

    fn fire_player_shot(shots_ref: &mut Vec<Actor>, player: &Player) {
        MainState::spawn_shots(shots_ref, &player.actor.pos);
    }

    fn clear_dead_stuff(&mut self) {
        self.shots.retain(|s| !s.kill);
        self.rocks.retain(|r| !r.kill);
    }

    fn update_time(&mut self) {
        let now = std::time::Instant::now();
        self.curr_time = now.duration_since(self.start_time).as_micros() as f32 / 1000000.0;
    }

    fn reset_time(&mut self) {
        self.start_time = std::time::Instant::now();
    }

    fn restart_game(&mut self) {
        println!("GAME OVER: Time: {:?} | Score: {:?} | On Difficulty: {:?}", self.curr_time, self.score, self.difficulty_mult);

        self.local_input = InputState::default();
        for p in &mut self.players {
            p.last_shot_at = 0.0;
            p.input = InputState::default();
        }
        self.reset_time();
        self.score = 0;
        for shot in &mut self.shots {
            shot.kill = true;
        }
        for rock in &mut self.rocks {
            rock.kill = true;
        }
    }

    fn handle_collisions(&mut self, _ctx: &ggez::Context) {
        let mut should_restart = false;
        for rock in &mut self.rocks {

            for player_obj in &self.players {
                let player = &player_obj.actor;
                let pdistance = rock.pos - player.pos;
                if pdistance.norm() < (player.bbox_size + rock.bbox_size) {
                    should_restart = true;
                }
            }
            
            for shot in &mut self.shots {
                let distance = shot.pos - rock.pos;
                if distance.norm() < (shot.bbox_size + rock.bbox_size) {
                    shot.kill = true;
                    rock.kill = true;
                    self.score += 1;
                    self.play_sounds.play_hit = true;
                }
            }
        }
        if should_restart {
            self.restart_game();
            self.play_sounds.play_hit = true;
        }
    }
    
    fn client_handle_sounds(&mut self, _ctx: &ggez::Context) {
        for rock in &mut self.rocks {
            for shot in &mut self.shots {
                let distance = shot.pos - rock.pos;
                if distance.norm() < (shot.bbox_size + rock.bbox_size) {
                    self.play_sounds.play_hit = true;
                    return
                }
            }
        }
    }

    fn spawn_rocks(&mut self, delta: f32) {
        let loops = (delta / 0.004).round() as i32;

        let time_mult = self.curr_time * self.difficulty_mult;

        let spawnpercent =  time_mult / 1600.0 + 0.01;
        let speed_mod = f32::powf(time_mult * 4.0, 0.85) + 100.0;
        let mut max_angle = time_mult / 240.0;

        if max_angle > 0.5 {
            max_angle = 0.5;
        }

        for _ in 0..loops {
            if rand::random::<f32>() < spawnpercent {
                let mut rock = Actor::create_rock();

                let mut angle = rand::random::<f32>() * max_angle;
                if rand::random::<bool>() {
                    angle = -angle;
                }
                let x_pos = (rand::random::<f32>() * self.screen_width as f32) - self.screen_width as f32 / 2.0;
                let y_pos = (self.screen_height as f32) / 2.0 - 15.0;

                let speed = rand::random::<f32>() * speed_mod + speed_mod / 2.0;
                
                rock.pos = Vector2::new(x_pos, y_pos);
                rock.velocity = vec_from_angle(std::f32::consts::PI + angle) * (speed);
                
                self.rocks.push(rock);
            }
        }
        
    }

    fn update_ui(&mut self, ctx: &mut Context) {
        let str = match self.local_player_index {
                Some(0) => { 
                    format!("Server | Players: {} | Specators: {}", self.players.len(), self.connections + 1 - (self.players.len()  as u32)) 
                }
                Some(x) => {
                    format!("Client | Player Id: {}", x)
                }
                None => {
                    format!("Specator")
                }
            };
                

        let score_str = format!("Score: {}  {}", self.score, str);
        let score_text = graphics::Text::new(ctx, &score_str, &self.assets.font).unwrap();


        let level_str = format!("Time: {}", self.curr_time);
        let level_text = graphics::Text::new(ctx, &level_str, &self.assets.font).unwrap();

        self.score_display = score_text;
        self.level_display = level_text;
    }

    fn play_sounds(&mut self) {
        if self.play_sounds.play_hit && !self.assets.hit_sound.playing() {
            let _ = self.assets.hit_sound.play();
        }
        if self.play_sounds.play_shot && !self.assets.shot_sound.playing() {
            let _ = self.assets.shot_sound.play();
        }
        self.clear_sounds();
    }

    fn clear_sounds(&mut self) {
        self.play_sounds = PlaySounds::default();
    }

    fn tick_physics(&mut self, seconds: f32) {
        // Tick shots
        for shot in &mut self.shots {
            shot.tick_physics(seconds);

            if shot.is_out_of_bounds(self.screen_width as f32, self.screen_height as f32) {
                shot.kill = true;
            }
        }

        for shot in &mut self.local_shots_made {
            shot.tick_physics(seconds);
        }

        // Tick rocks
        for rock in &mut self.rocks {
            rock.tick_physics(seconds);

            if rock.is_out_of_bounds(self.screen_width as f32, self.screen_height as f32) {
                rock.kill = true;
            }
        }
    }

    fn update_player_inputs(&mut self, seconds: f32) {
        if let Some(index) = self.local_player_index {
            if self.players.len() > index as usize {
                self.players[index as usize].input = self.local_input.clone();
            }
        }

        for player in &mut self.players {
            player.tick_input(seconds);
            player.actor.wrap_position(self.screen_width as f32, self.screen_height as f32);
        }
    
        for player in &mut self.players {

            if player.input.fire && player.last_shot_at <= self.curr_time - PLAYER_SHOT_TIME {
                player.last_shot_at = self.curr_time;

                match self.local_player_index {
                    Some(0) => {
                        if player.index == 0 {
                            MainState::fire_player_shot(&mut self.shots, player);
                        }
                    }
                    None => {
                        MainState::fire_player_shot(&mut self.shots, player);
                    }
                    Some(x) => {
                        if x == player.index as usize {
                            let mut new_shots = Vec::new();
                            MainState::fire_player_shot(&mut new_shots, player);
                            self.local_shots_made.append(&mut new_shots.clone());
                            self.shots.append(&mut new_shots);
                        }
                        else {
                            MainState::fire_player_shot(&mut self.shots, player);
                        }
                    }
                }
                
                self.play_sounds.play_shot = true;
            }
        }
    }

    fn real_update_server(&mut self, ctx: &mut Context, seconds: f32) -> GameResult<()> {
        self.update_player_inputs(seconds);
        self.tick_physics(seconds);
        self.handle_collisions(ctx);
        self.clear_dead_stuff();

        self.spawn_rocks(seconds);
        self.update_ui(ctx);
        Ok(())
    }

    /// Perform interpolation & "prediction"
    fn real_update_client(&mut self, ctx: &mut Context, seconds: f32) -> GameResult<()> {
        self.update_player_inputs(seconds);

        self.tick_physics(seconds);
        self.client_handle_sounds(ctx);
        self.update_ui(ctx);
        Ok(())
    }

    fn s_draw(&mut self, ctx: &mut Context) -> GameResult<()> {

        // Loop over all objects drawing them...
        {
            let assets = &mut self.assets;
            let coords = (self.screen_width, self.screen_height);
            
            for p_obj in &self.players {
                draw_actor(assets, ctx, &p_obj.actor, coords)?;
            }
            
            for s in &self.shots {
                draw_actor(assets, ctx, s, coords)?;
            }

            for r in &self.rocks {
                draw_actor(assets, ctx, r, coords)?;
            }
        }

        // And draw the GUI elements in the right places.
        let level_dest = graphics::Point2::new(10.0, 10.0);
        let score_dest = graphics::Point2::new(200.0, 10.0);
        graphics::draw(ctx, &self.level_display, level_dest, 0.0)?;
        graphics::draw(ctx, &self.score_display, score_dest, 0.0)?;


        // Play our sound queue
        self.play_sounds();

        Ok(())
    }

    // Handle key events.  These just map keyboard events
    // and alter our input state appropriately.
    fn s_key_down_event(&mut self, ctx: &mut Context, keycode: Keycode, _keymod: Mod, _repeat: bool) {
        let input_ref = &mut self.local_input;
        match keycode {
            Keycode::Up => {
                input_ref.up = true;
            }
            Keycode::Down => {
                input_ref.down = true;
            }
            Keycode::Left => {
                input_ref.left = true;
            }
            Keycode::Right => {
                input_ref.right = true;
            }
            Keycode::Space => {
                input_ref.fire = true;
            }
            Keycode::Escape => ctx.quit().unwrap(),
            _ => (), // Do nothing
        }
    }

    fn s_key_up_event(&mut self, _ctx: &mut Context, keycode: Keycode, _keymod: Mod, _repeat: bool) {
        let input_ref = &mut self.local_input;
        match keycode {
            Keycode::Up => {
                input_ref.up = false;
            }
            Keycode::Down => {
                input_ref.down = false;
            }
            Keycode::Left => {
                input_ref.left = false;
            }
            Keycode::Right => {
                input_ref.right = false;
            }
            Keycode::Space => {
                input_ref.fire = false;
            }
            _ => (), // Do nothing
        }
    }

}

fn print_instructions() {
    println!();
    println!("Welcome to Rust-Blaster");
    println!();
}

fn draw_actor(
    assets: &mut Assets,
    ctx: &mut Context,
    actor: &Actor,
    world_coords: (u32, u32),
) -> GameResult<()> {
    let (screen_w, screen_h) = world_coords;
    let pos = world_to_screen_coords(screen_w, screen_h, Point2::new(actor.pos.x, actor.pos.y));
    let image = assets.actor_image(actor);
    let drawparams = graphics::DrawParam {
        dest: pos,
        rotation: actor.facing as f32,
        offset: graphics::Point2::new(0.5, 0.5),
        ..Default::default()
    };
    graphics::draw_ex(ctx, image, drawparams)
}

impl EventHandler for StatePtr {
    fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
        graphics::clear(ctx);
        let r = self.state.lock().unwrap().s_draw(ctx);
        graphics::present(ctx);

        std::thread::sleep(Duration::from_micros(500));
        r
    }

    fn update(&mut self, ctx: &mut Context) -> GameResult<()> {

        const DESIRED_FPS: u32 = 144;
        
        while timer::check_update_time(ctx, DESIRED_FPS) {
            let seconds = 1.0 / (DESIRED_FPS as f32);

            let mut locked_state = self.state.lock().unwrap();          
            
            if locked_state.is_server() {
                locked_state.update_time();
                locked_state.real_update_server(ctx, seconds)?;
            }
            else {
                locked_state.curr_time += seconds;
                locked_state.real_update_client(ctx, seconds)?;
            }
        }

        Ok(())
    }

    fn key_down_event(&mut self, ctx: &mut Context, keycode: Keycode, _keymod: Mod, _repeat: bool) {
        self.state.lock().unwrap().s_key_down_event(ctx, keycode, _keymod, _repeat)
    }

    fn key_up_event(&mut self, _ctx: &mut Context, keycode: Keycode, _keymod: Mod, _repeat: bool) {
        self.state.lock().unwrap().s_key_up_event(_ctx, keycode, _keymod, _repeat)
    }
}

pub fn main() {
    let mut cb = ContextBuilder::new("rust-blaster", "katagis")
        .window_setup(conf::WindowSetup::default().title("Rust Blaster!"))
        .window_mode(conf::WindowMode::default().dimensions(1080, 1080));

    cb = cb.add_resource_path(path::PathBuf::from("resources"));

    let ctx = &mut cb.build().unwrap();
    
    let mut game_ptr = StatePtr::new(ctx);

    let mut net_ptr = game_ptr.get_ref();
    std::thread::spawn(move || {
        networking::network_main(&mut net_ptr);
    });

    let result = event::run(ctx, &mut game_ptr);

    if let Err(e) = result {
        println!("Error encountered running game: {}", e);
    } else {
        println!("Game exited cleanly.");
    }
}
