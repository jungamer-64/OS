use x86_64::instructions::port::Port;

pub struct Keyboard {
    data_port: Port<u8>,
    status_port: Port<u8>,
}

impl Keyboard {
    pub const fn new() -> Self {
        Self {
            data_port: Port::new(0x60),
            status_port: Port::new(0x64),
        }
    }

    pub fn read_scancode(&mut self) -> Option<u8> {
        let status = unsafe { self.status_port.read() };
        // Bit 0: Output Buffer Full
        if status & 1 != 0 {
            Some(unsafe { self.data_port.read() })
        } else {
            None
        }
    }
}
