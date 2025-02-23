use std::{fs::File, os::unix::io::AsFd};

use wayland_client::{
    delegate_noop,
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_shm, wl_shm_pool,
        wl_surface,
    },
    Connection, Dispatch, QueueHandle, WEnum,
};

use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

// Our modules
mod err;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::thread::spawn(do_special_wm_configs);
    std::thread::sleep(std::time::Duration::from_millis(20)); // Tiny delay to allow bg thread a chance to win race conditions

    let conn = Connection::connect_to_env().unwrap();

    let mut event_queue = conn.new_event_queue();
    let qhandle = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let mut state = State::default();

    println!("Starting the example window app, press <ESC> to quit.");

    while state.running {
        event_queue.blocking_dispatch(&mut state).map_err(err::eloc!())?;
    }

    println!("Done goodbye!");

    Ok(())
}

fn do_special_wm_configs() {
    // Force sway to make the window float
    //std::thread::sleep(std::time::Duration::from_millis(300));
    let _s = std::process::Command::new("swaymsg")
        // float, move resize to 100% by 12%, move to x=0, y=80%
        .args(&["for_window [app_id=\"sdock\"] floating enable, for_window [app_id=\"sdock\"] resize set width 100ppt height 12ppt, for_window [app_id=\"sdock\"] move position 0 88ppt"])
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

    pub redraw_necessary: bool,

    // INVARIANT: width and height must ALWAYS be > 0
    pub configured_w: i32,
    pub configured_h: i32,
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
            redraw_necessary: true,
            configured_h: 1,
            configured_w: 1,
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

fn static_draw(tmp: &mut File, (buf_x, buf_y): (u32, u32)) -> Result<(), Box<dyn std::error::Error>> {
    use std::{cmp::min, io::Write};
    let mut buf = std::io::BufWriter::new(tmp);
    for y in 0..buf_y {
        for x in 0..buf_x {
            let a = 0xFF;
            let r = min(((buf_x - x) * 0xFF) / buf_x, ((buf_y - y) * 0xFF) / buf_y);
            let g = min((x * 0xFF) / buf_x, ((buf_y - y) * 0xFF) / buf_y);
            let b = min(((buf_x - x) * 0xFF) / buf_x, (y * 0xFF) / buf_y);
            buf.write_all(&[b as u8, g as u8, r as u8, a as u8]).map_err(err::eloc!())?;
        }
    }
    buf.flush().map_err(err::eloc!())?;
    Ok(())
}

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

        match tempfile::tempfile() {
            Ok(mut file) => {
                eprintln!("Drawing to memory at {:?}", file);

                let uw = self.configured_w as u32;
                let uh = self.configured_h as u32;

                if let Err(e) = static_draw(&mut file, (uw, uh)) {
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
            let surface = state.base_surface.as_ref().unwrap();
            if let Some(ref buffer) = state.buffer {
                surface.attach(Some(buffer), 0, 0);
                surface.damage(0, 0, 1, 1);
                surface.commit();
            }
            if let Some(registry) = state.stolen_registry.clone() {
                state.draw(1, &registry, qh);
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
                //seat.get_pointer(qh, ());
            }
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
    }
}
