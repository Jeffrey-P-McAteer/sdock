
#![allow(dead_code, unused_variables, unused_imports)]

use std::{fs::File, os::unix::io::AsFd};

use wayland_client::{
    delegate_noop,
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool,
        wl_surface,
    },
    Connection, Dispatch, QueueHandle, WEnum,
};

use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

// Our modules
mod err;
mod util;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::thread::spawn(do_special_wm_configs);
    std::thread::sleep(std::time::Duration::from_millis(20)); // Tiny delay to allow bg thread a chance to win race conditions

    let conn = Connection::connect_to_env().unwrap();

    let mut event_queue = conn.new_event_queue();
    let qhandle = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let mut state = State::default();

    let mut haruhi_shot_init_retries = 9;
    while haruhi_shot_init_retries > 0 {
        if let Err(ref e) = state.haruhi_shot {
            haruhi_shot_init_retries -= 1;
            eprintln!("WARNING: Failed to init HaruhiShotState, {} retries remaining: {:?}", haruhi_shot_init_retries, e);
            std::thread::sleep(std::time::Duration::from_millis(800));
            state.haruhi_shot = libharuhishot::HaruhiShotState::init();
        }
        else {
            break; // we got it, yay!
        }
    }

    println!("Starting the example window app, press <ESC> to quit.");

    while state.running {
        event_queue.blocking_dispatch(&mut state).map_err(err::eloc!())?;
        // TODO determine based on window positions if drawing is appropriate; this loop runs _ALL_THE_TIME_
        state.take_screenshot(); // Queue error; libharuhi is also maintaining a connection; can we send ours to it so they can share?
        state.draw_from_stolen();
    }

    println!("Done goodbye!");

    Ok(())
}

fn do_special_wm_configs() {
    // Force sway to make the window float
    //std::thread::sleep(std::time::Duration::from_millis(300));
    let _s = std::process::Command::new("swaymsg")
        // float, move resize to 100% by 12%, move to x=0, y=80% // sticky enable
        .args(&["for_window [app_id=\"sdock\"] floating enable, for_window [app_id=\"sdock\"] resize set width 100ppt height 12ppt, for_window [app_id=\"sdock\"] move position 0 89ppt, for_window [app_id=\"sdock\"] sticky enable"])
        .status();
}

struct State {
    pub running: bool,
    pub base_surface: Option<wl_surface::WlSurface>,
    pub buffer: Option<wl_buffer::WlBuffer>,
    pub wm_base: Option<xdg_wm_base::XdgWmBase>,
    pub xdg_surface: Option<(xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel)>,
    pub configured: bool,

    pub stolen_registry: Option<wl_registry::WlRegistry>,
    pub stolen_qh: Option<QueueHandle<State>>,

    pub redraw_necessary: bool,

    // INVARIANT: width and height must ALWAYS be > 0
    pub configured_w: i32,
    pub configured_h: i32,

    pub haruhi_shot: Result<libharuhishot::HaruhiShotState, libharuhishot::haruhierror::HaruhiError>,
    // Pixel values in format [b, g, r, a]
    pub last_screenshot_px: Vec::<[u8; 4]>,
}

impl Default for State {
     fn default() -> State {
        State {
            running: true,
            base_surface: None,
            buffer: None,
            wm_base: None,
            xdg_surface: None,
            configured: false,
            stolen_registry: None,
            stolen_qh: None,
            redraw_necessary: true,
            configured_h: 1,
            configured_w: 1,
            haruhi_shot: libharuhishot::HaruhiShotState::init(),
            last_screenshot_px: Vec::with_capacity(1920 * ((1200*80)/100) * 2), // Guess at a monitor size, take last 20% of space + double estimate. Yay heuristics for performance!
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        // Allows other methods to push things to the registry
        state.stolen_registry = Some(registry.clone());
        state.stolen_qh = Some(qh.clone());

        if let wl_registry::Event::Global { name, interface, .. } = event {
            match &interface[..] {
                "wl_compositor" => {
                    eprintln!("{}:{} got event name={} wl_compositor ", file!(), line!(), &name);
                    let compositor =
                        registry.bind::<wl_compositor::WlCompositor, _, _>(name, 1, qh, ());
                    let surface = compositor.create_surface(qh, ());
                    state.base_surface = Some(surface);

                    if state.wm_base.is_some() && state.xdg_surface.is_none() {
                        state.init_xdg_surface(qh);
                    }
                }
                "wl_shm" => {
                    eprintln!("{}:{} got event name={} wl_shm ", file!(), line!(), &name);
                    state.draw(name, registry, qh);
                }
                "wl_seat" => {
                    eprintln!("{}:{} got event name={} wl_seat ", file!(), line!(), &name);
                    registry.bind::<wl_seat::WlSeat, _, _>(name, 1, qh, ());
                }
                "xdg_wm_base" => {
                    eprintln!("{}:{} got event name={} xdg_wm_base ", file!(), line!(), &name);
                    let wm_base = registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 1, qh, ());
                    state.wm_base = Some(wm_base);

                    if state.base_surface.is_some() && state.xdg_surface.is_none() {
                        state.init_xdg_surface(qh);
                    }
                }
                "wl_output" => {
                    eprintln!("{}:{} got event name={} wl_output ", file!(), line!(), &name);

                    state.draw(1, registry, qh); // this doesn't violate protocol, but we shouldn't hard-code protocol numbers;;;

                }
                unk_name => {
                    eprintln!("{}:{} got event name={} unk_name={} ", file!(), line!(), &name, &unk_name);
                }
            }
        }
    }
}

// Ignore events from these object types in this example.
delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);

impl State {
    fn init_xdg_surface(&mut self, qh: &QueueHandle<State>) {
        match self.wm_base.as_ref() {
            Some(wm_base) => {
                match self.base_surface.as_ref() {
                    Some(base_surface) => {
                        let xdg_surface = wm_base.get_xdg_surface(base_surface, qh, ());
                        let toplevel = xdg_surface.get_toplevel(qh, ());
                        // https://smithay.github.io/wayland-rs/wayland_protocols/xdg/shell/client/xdg_toplevel/struct.XdgToplevel.html#method.set_title
                        toplevel.set_title("sdock".into());
                        toplevel.set_app_id("sdock".into());

                        base_surface.commit();

                        self.xdg_surface = Some((xdg_surface, toplevel));
                    }
                    None => {
                        eprintln!("{}:{} self.base_surface.as_ref() returned None!", file!(), line!());
                    }
                }
            }
            None => {
                eprintln!("{}:{} self.wm_base.as_ref() returned None!", file!(), line!());
            }
        }
    }
    fn draw(&mut self, name: u32, registry: &wl_registry::WlRegistry, qh: &QueueHandle<State>) {
        let shm = registry.bind::<wl_shm::WlShm, _, _>(name, 1, qh, ());

        match util::create_shm_fd() {
            Ok(owned_file) => {
                let mut file = std::fs::File::from(owned_file);
                eprintln!("Drawing to memory at {:?}", file);

                let uw = self.configured_w as u32;
                let uh = self.configured_h as u32;

                if let Err(e) = static_draw(&self.last_screenshot_px, &mut file, (uw, uh)) {
                    eprintln!("{:?}", e);
                }

                let pool = shm.create_pool(file.as_fd(), self.configured_w * self.configured_h * 4, qh, ()); // create_pool CANNOT take in a 0 value, invalid protocol use!

                let buffer = pool.create_buffer(
                    0,
                    self.configured_w,
                    self.configured_h,
                    self.configured_w * 4,
                    wl_shm::Format::Argb8888,
                    qh,
                    (),
                );
                self.buffer = Some(buffer.clone());

                if self.configured {
                    match self.base_surface.as_ref() {
                        Some(surface) => {
                            surface.attach(Some(&buffer), 0, 0);
                            surface.damage(0, 0, self.configured_w, self.configured_h);
                            surface.commit();
                        }
                        None => {
                            eprintln!("{}:{} self.base_surface.as_ref() is None", file!(), line!());
                        }
                    }
                }
                else {
                    eprintln!("{}:{} We are not yet configured!", file!(), line!());
                }
            }
            Err(e) => {
                eprintln!("{}:{} {:?}", file!(), line!(), e);
            }
        }

    }

    pub fn draw_from_stolen(&mut self) {
        if let Some(registry) = self.stolen_registry.clone() { // ugh .clones
            if let Some(qh) = self.stolen_qh.clone() {
                self.draw(1, &registry, &qh);
            }
        }
    }

    pub fn take_screenshot(&mut self) {
        eprintln!("Begin take_screenshot");
        if self.configured_w < 4 || self.configured_h < 4 {
            return; // invalid to take screenshot 0x0 in size
        }
        let dock_w = self.configured_w / 2;
        let dock_lr_margin = (self.configured_w - dock_w) / 2;
        let begin_x = dock_lr_margin;
        let end_x = self.configured_w - dock_lr_margin;

        let screenshot_y_above_dock_dist = self.configured_h; // We capture 2x the dock's height; no need for entire screen!

        //eprintln!("size = {:?}", (dock_w as i32, (self.configured_h + screenshot_y_above_dock_dist) as i32));

        let mut screenshot_px = Vec::<[u8; 4]>::with_capacity((self.configured_w * self.configured_h) as usize); // Screenshot turns into array of [b as u8, g as u8, r as u8, a as u8] values
        if let Ok(ref mut haruhi_shot) = self.haruhi_shot {
            match haruhi_shot.capture_output_frame(
                &haruhi_shot.displays[0].clone(),
                (dock_w as i32, (self.configured_h + screenshot_y_above_dock_dist) as i32), // output w,h
                haruhi_shot.display_transform[0],
                Some((
                    begin_x as i32, haruhi_shot.display_logic_size[0].1 - (self.configured_h + screenshot_y_above_dock_dist) as i32, // x,y
                    dock_w as i32, (self.configured_h + screenshot_y_above_dock_dist) as i32 // w,h
                ))
            ) {
                Ok(Some(frame_buff_info)) => {
                    // Map it and draw into screenshot_px
                    let frame_px =  frame_buff_info.frame_mmap;
                    if frame_buff_info.frameformat.format == libharuhishot::reexport::Format::Xbgr8888 {
                        for y in 0..frame_buff_info.frameformat.height {
                            for x in 0..frame_buff_info.frameformat.width {
                                let frame_px_i = (((y * frame_buff_info.frameformat.width) + x) * 4) as usize;

                                screenshot_px.push([
                                    unsafe { *frame_px.get_unchecked(frame_px_i+2) },
                                    unsafe { *frame_px.get_unchecked(frame_px_i+1) },
                                    unsafe { *frame_px.get_unchecked(frame_px_i+0) },
                                    0xFF as u8
                                ]);
                            }
                        }
                    }
                    else {
                        eprintln!("UNK VALUE: frame_buff_info.realheight = {} frame_buff_info.realwidth = {}", frame_buff_info.realheight, frame_buff_info.realwidth);
                        eprintln!("UNK VALUE: frame_buff_info.frameformat.format = {:?} frame_buff_info.frameformat.width = {} frame_buff_info.frameformat.height = {}",
                                frame_buff_info.frameformat.format, frame_buff_info.frameformat.width, frame_buff_info.frameformat.height);
                        eprintln!("Expected Xbgr8888 formatted pixels");
                    }
                }
                Ok(None) => {
                    eprintln!("{}:{} success but no frame data returned to us!", file!(), line!());
                }
                Err(e) => {
                    eprintln!("{}:{} {:?}", file!(), line!(), e);
                }
            }
        }
        eprintln!("screenshot_px.len() = {}", screenshot_px.len());
        if screenshot_px.len() > 0 {
            self.last_screenshot_px.clear();
            self.last_screenshot_px.append(&mut screenshot_px);
            // screenshot_px is now empty
        }
    }

}

fn shadow_falloff_f(dist_to_edge: f32) -> u8 {
    return ((dist_to_edge as f32 / SHADOW_W_PX as f32) * 255.0) as u8;
}

fn shadow_falloff_i(dist_to_edge: i32) -> u8 {
    return ((dist_to_edge as f32 / SHADOW_W_PX as f32) * 255.0) as u8;
    //return (( (dist_to_edge as f32 / 3.46).powf(2.0) / SHADOW_W_PX as f32) * 255.0) as u8;
    //return (( (dist_to_edge as f32 / 5.23).powf(3.0) / SHADOW_W_PX as f32) * 255.0) as u8;
}

const SHADOW_W_PX: i32 = 24;
const METAL_TEXTURE_OVLY: [u8; 16] = [
    8,  12, 16, 12,
    4,  8,  12,  8,
    8,  4,  8,   4,
    12, 8,  12,  8,
];

fn static_draw(screenshot_px: &Vec::<[u8; 4]>, tmp: &mut File, (buf_x, buf_y): (u32, u32)) -> Result<(), Box<dyn std::error::Error>> {
    use std::{cmp::min, io::Write};

    if buf_x < 12 || buf_y < 12 {
        return Ok(());
    }

    let mut buf = std::io::BufWriter::new(tmp);

    let dock_w = buf_x / 2;
    let dock_lr_margin = (buf_x - dock_w) / 2;
    let begin_x = dock_lr_margin;
    let end_x = buf_x - dock_lr_margin;

    let screenshot_y_above_dock_dist = buf_y; // We capture 2x the dock's height; no need for entire screen!

    // Compute dock detailed geometry

    let dock_lip_h = 6;
    let dock_angle_deg = 30;

    // Used with: griffin-reader 'file_int_ex(45, "/tmp/a", lambda x: x-1)' 'file_int_ex(45, "/tmp/a", lambda x: x+1)'
    //let contents = std::fs::read_to_string("/tmp/a")?;
    //let dock_angle_deg = contents.parse::<i32>()?;

    let dock_top_x_inset = f32::sin(dock_angle_deg as f32 * (180.0 as f32 / std::f32::consts::PI)) * buf_y as f32;
    let dock_top_x_inset = dock_top_x_inset.abs();
    // let dock_height = f32::sin(dock_angle_deg as f32 * (180.0 as f32 / std::f32::consts::PI)) as u32;

    //eprintln!("dock_top_x_inset = {:?}", dock_top_x_inset);

    let mut dock_x_insets = vec![];
    for y in 0..buf_y {
        let ratio = (buf_y-y) as f32 / buf_y as f32;
        dock_x_insets.push(
            (dock_top_x_inset * ratio) as i32
        );
    }

    let mut px_buf: Vec<[u8; 4]> = Vec::with_capacity((buf_x * buf_y) as usize);
    for y in 0..buf_y {
        for x in 0..buf_x {
            px_buf.push([0 as u8, 0 as u8, 0 as u8, 0 as u8]);
        }
    }

    for y in 0..buf_y {
        let dist_to_y_edge = SHADOW_W_PX - y as i32;
        for x in 0..begin_x {
            let buf_i = ((y * buf_x) + x) as usize;
            //buf.write_all(&[0 as u8, 0 as u8, 0 as u8, 0 as u8]).map_err(err::eloc!())?;
            px_buf[buf_i] = [0 as u8, 0 as u8, 0 as u8, 0 as u8];
        }
        for x in begin_x..end_x {
            let buf_i = ((y * buf_x) + x) as usize;
            if x > dock_x_insets[y as usize] as u32 + begin_x as u32 && x < end_x - dock_x_insets[y as usize] as u32 {
                // We are within the "dock" area - but we use the first interior SHADOW_W_PX as an alpha ramp-up from transparent to the actual edge.
                let dist_to_left_edge = (x as i32 - dock_x_insets[y as usize]) - dock_lr_margin as i32;
                let dist_to_right_edge = (end_x as i32 - dock_x_insets[y as usize] as i32) - x as i32;
                if dist_to_y_edge > 0 && dist_to_y_edge <= SHADOW_W_PX {
                    // Make a linear shadow, skipping the first + last SHADOW_W_PX of X space
                    if dist_to_left_edge < SHADOW_W_PX || dist_to_right_edge < SHADOW_W_PX {
                        // Circular fall-off or some such shadow nonsense
                        let dist_to_x_corner = SHADOW_W_PX - std::cmp::min(dist_to_left_edge, dist_to_right_edge);
                        let dist_to_y_corner = dist_to_y_edge;
                        //let dist_to_corner = ((dist_to_x_corner*dist_to_x_corner) as f32 + (dist_to_y_corner*dist_to_y_corner) as f32).sqrt() as i32;
                        let dist_to_corner = ((dist_to_x_corner*dist_to_x_corner) as f32 + (dist_to_y_corner*dist_to_y_corner) as f32).sqrt() as i32;
                        //let shadow_amnt = 255.0 - ((dist_to_corner as f32 / SHADOW_W_PX as f32) * 255.0);
                        let shadow_amnt = 255 - shadow_falloff_i(dist_to_corner);
                        //buf.write_all(&[0x00 as u8, 0x00 as u8, 0x00 as u8, shadow_amnt]).map_err(err::eloc!())?;
                        px_buf[buf_i] = [0x00 as u8, 0x00 as u8, 0x00 as u8, shadow_amnt];
                    }
                    else {
                        let linear_shadow_a = 255 - shadow_falloff_i(dist_to_y_edge); //((1.0 - (dist_to_y_edge as f32 / SHADOW_W_PX as f32)) * 255.0) as u8;
                        //buf.write_all(&[0x00 as u8, 0x00 as u8, 0x00 as u8, linear_shadow_a]).map_err(err::eloc!())?;
                        px_buf[buf_i] = [0x00 as u8, 0x00 as u8, 0x00 as u8, linear_shadow_a];
                    }
                }
                else if dist_to_left_edge < SHADOW_W_PX {
                    let linear_shadow_a = shadow_falloff_i(dist_to_left_edge);
                    //buf.write_all(&[0x00 as u8, 0x00 as u8, 0x00 as u8, linear_shadow_a]).map_err(err::eloc!())?;
                    px_buf[buf_i] = [0x00 as u8, 0x00 as u8, 0x00 as u8, linear_shadow_a];
                }
                else if dist_to_left_edge == SHADOW_W_PX {
                    //buf.write_all(&[0x00 as u8, 0x00 as u8, 0x00 as u8, 0xFF as u8]).map_err(err::eloc!())?;
                    px_buf[buf_i] = [0x00 as u8, 0x00 as u8, 0x00 as u8, 0xFF as u8];
                }
                else if dist_to_right_edge < SHADOW_W_PX {
                    let linear_shadow_a = shadow_falloff_i(dist_to_right_edge);
//                    buf.write_all(&[0x00 as u8, 0x00 as u8, 0x00 as u8, linear_shadow_a]).map_err(err::eloc!())?;
                    px_buf[buf_i] = [0x00 as u8, 0x00 as u8, 0x00 as u8, linear_shadow_a];
                }
                else if dist_to_right_edge == SHADOW_W_PX {
                    //buf.write_all(&[0x00 as u8, 0x00 as u8, 0x00 as u8, 0xFF as u8]).map_err(err::eloc!())?;
                    px_buf[buf_i] = [0x00 as u8, 0x00 as u8, 0x00 as u8, 0xFF as u8];
                }
                else {
                    let screenshot_reflected_y = (screenshot_y_above_dock_dist - y) + SHADOW_W_PX as u32; // todo more magic here
                    let x_correction_amount = (dock_w / 2) + 6; // Ok genius where are we being offset by w/2 and six pixels?!/???
                    let screenshot_px_i = ((screenshot_reflected_y * dock_w) + x + x_correction_amount) as usize;

                    //let metal_overlay_val = METAL_TEXTURE_OVLY[(((y % 4) * 4) + (x % 4)) as usize];
                    let metal_overlay_val = 0;

                    if screenshot_px_i > 0 && screenshot_px_i < screenshot_px.len() {
                        let mut b = screenshot_px[screenshot_px_i][0];
                        let mut g = screenshot_px[screenshot_px_i][1];
                        let mut r = screenshot_px[screenshot_px_i][2];

                        if b > metal_overlay_val {
                            b -= metal_overlay_val;
                        }
                        if g > metal_overlay_val {
                            g -= metal_overlay_val;
                        }
                        if r > metal_overlay_val {
                            r -= metal_overlay_val;
                        }

                        // buf.write_all(&[b, g, r, 0xFF as u8]).map_err(err::eloc!())?;
                        px_buf[buf_i] = [b, g, r, 0xFF as u8];

                    }
                    else {
                        let a = 0xE0;
                        let r = min(((buf_x - x) * 0xFF) / buf_x, ((buf_y - y) * 0xFF) / buf_y);
                        let g = min((x * 0xFF) / buf_x, ((buf_y - y) * 0xFF) / buf_y);
                        let b = min(((buf_x - x) * 0xFF) / buf_x, (y * 0xFF) / buf_y);
                        //buf.write_all(&[b as u8, g as u8, r as u8, a as u8]).map_err(err::eloc!())?;
                        px_buf[buf_i] = [b as u8, g as u8, r as u8, a as u8];
                    }
                }
            }
            else {
                //buf.write_all(&[0 as u8, 0 as u8, 0 as u8, 0 as u8]).map_err(err::eloc!())?;
                px_buf[buf_i] = [0 as u8, 0 as u8, 0 as u8, 0 as u8];
            }
        }
        for x in end_x..buf_x {
            let buf_i = ((y * buf_x) + x) as usize;
            //buf.write_all(&[0 as u8, 0 as u8, 0 as u8, 0 as u8]).map_err(err::eloc!())?;
            px_buf[buf_i] = [0 as u8, 0 as u8, 0 as u8, 0 as u8];
        }
    }

    // Blur the shadows by re-processing & avreaging a 2x2 grid
    for y in 0..buf_y {
        let dist_to_y_edge = SHADOW_W_PX - y as i32;
        for x in begin_x..end_x {
            let buf_i = ((y * buf_x) + x) as usize;

            let buf_i_north = ((std::cmp::max(0, y as i32 - 1) * buf_x as i32) + x as i32) as usize;
            let buf_i_south = ((std::cmp::min(buf_y-1, y+1) * buf_x) + x) as usize;
            let buf_i_west = std::cmp::max(0, ((y * buf_x) + x) as i32 - 1 as i32) as usize;
            let buf_i_east = ((y * buf_x) + x + 1) as usize;

            //println!("{x} {y} to {buf_y} {buf_x} ; {buf_i_north} {buf_i_south} {buf_i_west} {buf_i_east}");
            assert!(buf_i_north < px_buf.len());
            assert!(buf_i_south < px_buf.len());
            assert!(buf_i_west < px_buf.len());
            assert!(buf_i_east < px_buf.len());

            if x > dock_x_insets[y as usize] as u32 + begin_x as u32 && x < end_x - dock_x_insets[y as usize] as u32 {
                // We are within the "dock" area - but we use the first interior SHADOW_W_PX as an alpha ramp-up from transparent to the actual edge.
                let dist_to_left_edge = (x as i32 - dock_x_insets[y as usize]) - dock_lr_margin as i32;
                let dist_to_right_edge = (end_x as i32 - dock_x_insets[y as usize] as i32) - x as i32;
                if dist_to_y_edge > 0 && dist_to_y_edge <= SHADOW_W_PX {
                    // Make a linear shadow, skipping the first + last SHADOW_W_PX of X space
                    if dist_to_left_edge < SHADOW_W_PX || dist_to_right_edge < SHADOW_W_PX {
                        px_buf[buf_i][3] = ((px_buf[buf_i_north][3] as i32 + px_buf[buf_i_south][3] as i32 + px_buf[buf_i_west][3] as i32 + px_buf[buf_i_east][3] as i32) / 4) as u8;
                    }
                    else {
                        px_buf[buf_i][3] = ((px_buf[buf_i_north][3] as i32 + px_buf[buf_i_south][3] as i32 + px_buf[buf_i_west][3] as i32 + px_buf[buf_i_east][3] as i32) / 4) as u8;
                    }
                }
                else if dist_to_left_edge < SHADOW_W_PX {
                    px_buf[buf_i][3] = ((px_buf[buf_i_north][3] as i32 + px_buf[buf_i_south][3] as i32 + px_buf[buf_i_west][3] as i32 + px_buf[buf_i_east][3] as i32) / 4) as u8;
                }
                else if dist_to_left_edge == SHADOW_W_PX {
                    //px_buf[buf_i] = [0x00 as u8, 0x00 as u8, 0x00 as u8, 0xFF as u8];
                }
                else if dist_to_right_edge < SHADOW_W_PX {
                    px_buf[buf_i][3] = ((px_buf[buf_i_north][3] as i32 + px_buf[buf_i_south][3] as i32 + px_buf[buf_i_west][3] as i32 + px_buf[buf_i_east][3] as i32) / 4) as u8;
                }
                else if dist_to_right_edge == SHADOW_W_PX {
                    //px_buf[buf_i] = [0x00 as u8, 0x00 as u8, 0x00 as u8, 0xFF as u8];
                }
            }
        }
    }



    // Final write to shared-memory buffer
    for i in 0..px_buf.len() {
        buf.write_all(&px_buf[i]).map_err(err::eloc!())?;
    }

    buf.flush().map_err(err::eloc!())?;
    Ok(())
}



impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
        eprintln!("Got Dispatch<xdg_wm_base::XdgWmBase, ()> {:?}", event);
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(
        state: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial, .. } = event {
            xdg_surface.ack_configure(serial);
            state.configured = true;
            if let Some(registry) = state.stolen_registry.clone() {
                state.draw(1, &registry, qh);
            }
            else {
                let surface = state.base_surface.as_ref().unwrap();
                if let Some(ref buffer) = state.buffer {
                    surface.attach(Some(buffer), 0, 0);
                    surface.damage(0, 0, 1, 1);
                    surface.commit();
                }
            }
        }
        else {
            eprintln!("Ignoring Dispatch<xdg_surface::XdgSurface, ()> for State event {:?}", event);
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for State {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Close {} = event {
            state.running = false;
        }
        if let xdg_toplevel::Event::Configure { width, height, states: _ } = event {
            if width != state.configured_w || height != state.configured_h {
                state.redraw_necessary = true;
            }
            if width > 0 {
                state.configured_w = width;
            }
            if height > 0 {
                state.configured_h = height;
            }
            eprintln!("Got xdg_toplevel::Event::Configure {:?}", event);
        }
        else {
            eprintln!("Ignoring Dispatch<xdg_toplevel::XdgToplevel, ()> for State event {:?}", event);
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(capabilities) } = event {
            if capabilities.contains(wl_seat::Capability::Keyboard) {
                seat.get_keyboard(qh, ());
            }
            if capabilities.contains(wl_seat::Capability::Pointer) {
                seat.get_pointer(qh, ());
            }
            eprintln!("Got wl_seat::Event::Capabilities {:?}", event);
        }
        else {
            eprintln!("Ignoring Dispatch<wl_seat::WlSeat, ()> for State event {:?}", event);
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key { key, .. } = event {
            if key == 1 {
                // ESC key
                state.running = false;
            }
        }
        eprintln!("Got Dispatch<wl_keyboard::WlKeyboard, ()> {:?}", event);
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Motion { .. } = event {
            let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis();
            if now_ms % 50 == 0 {
                eprintln!("Got Dispatch<wl_pointer::WlPointer, ()> {:?}", event);
            }
        }
        else {
            eprintln!("Got Dispatch<wl_pointer::WlPointer, ()> {:?}", event);
        }
    }
}
