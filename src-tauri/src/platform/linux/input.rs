use xcb::x;

pub fn is_primary_mouse_button_down() -> bool {
    let Ok((conn, screen_num)) = xcb::Connection::connect(None) else {
        return false;
    };

    let setup = conn.get_setup();
    let Some(screen) = setup.roots().nth(screen_num as usize) else {
        return false;
    };

    let cookie = conn.send_request(&x::QueryPointer {
        window: screen.root(),
    });

    match conn.wait_for_reply(cookie) {
        Ok(reply) => {
            let mask = reply.mask().bits();
            // Button1Mask = 0x100 (button 1 pressed)
            (mask & 0x100) != 0
        }
        Err(_) => false,
    }
}
