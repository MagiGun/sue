//! A pen under Windows Ink, packet by packet.
//!
//! winit hands each packet of a Windows Ink pen to Sue as a touch event
//! at the packet's own place, but with the pressure the pen has at the
//! moment the message is read, the same for every packet of the message,
//! and without the packet's time. When Sue was busy with a frame, one
//! message stands for several packets, so their pressures would be one.
//! [`InkHistory`] pairs each touch event with its packet in the message's
//! history (`sue_winpen::pen_history`), in order and by place, and gives
//! back that packet's own pressure, lean, twist and time.

use sue_winpen::PenPacket;

use crate::stylus::DeviceClock;

/// How far a touch event may lie from its packet, in physical pixels: the
/// event's place is worked out from the pen's finer units, the packet's is
/// the screen pixel.
const NEAR: f64 = 2.0;

/// What a packet of the history says about a touch event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Packet {
    /// The pressure, 0 to 1.
    pub(crate) pressure: Option<f32>,
    /// The lean.
    pub(crate) tilt: Option<[f32; 2]>,
    /// The twist.
    pub(crate) twist: Option<f32>,
    /// The eraser end is in use.
    pub(crate) eraser: bool,
    /// When, on the stream's clock.
    pub(crate) time: f64,
}

/// The history of the pointer message being handled, and how far into
/// it the touch events have come.
#[derive(Debug, Default)]
pub(crate) struct InkHistory {
    /// The pointer, and the newest packet's frame and time: which message.
    batch: Option<(u64, u32, u32)>,
    packets: Vec<PenPacket>,
    next: usize,
    clock: DeviceClock,
}

impl InkHistory {
    /// The packet of `history` (the message's packets, oldest first, of
    /// pointer `id`) that a touch event at `client` (physical pixels in
    /// the window, whose top left is at `origin` on the screen) stands
    /// for, read at `now` on the stream's clock. `None` when none fits:
    /// the event then keeps what winit said.
    pub(crate) fn take(
        &mut self,
        id: u64,
        client: [f64; 2],
        origin: [i32; 2],
        history: Vec<PenPacket>,
        now: f64,
    ) -> Option<Packet> {
        let newest = history.last()?;
        let key = (id, newest.frame, newest.time);
        if self.batch != Some(key) {
            self.clock.sync(newest.time, now);
            self.batch = Some(key);
            self.packets = history;
            self.next = 0;
        }
        let near = |packet: &PenPacket| {
            let x = f64::from(packet.screen[0] - origin[0]);
            let y = f64::from(packet.screen[1] - origin[1]);
            (x - client[0].floor()).abs() <= NEAR && (y - client[1].floor()).abs() <= NEAR
        };
        let at = self.packets[self.next..].iter().position(near)? + self.next;
        self.next = at + 1;
        let packet = self.packets[at];
        Some(Packet {
            pressure: packet.pressure,
            tilt: packet.tilt,
            twist: packet.twist,
            eraser: packet.eraser,
            time: self.clock.seconds(packet.time),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(frame: u32, x: i32, pressure: f32, time: u32) -> PenPacket {
        PenPacket {
            frame,
            screen: [x, 500],
            pressure: Some(pressure),
            time,
            ..PenPacket::default()
        }
    }

    #[test]
    fn each_touch_event_of_a_message_gets_its_own_packet() {
        let mut ink = InkHistory::default();
        let origin = [100, 400];
        // One message for three packets, pressing harder each time, read
        // at 10 s.
        let history = vec![
            packet(1, 110, 0.2, 1_000),
            packet(2, 113, 0.4, 1_005),
            packet(3, 116, 0.6, 1_010),
        ];
        let mut seen = Vec::new();
        for x in [10.3, 13.7, 16.1] {
            let packet = ink
                .take(7, [x, 100.5], origin, history.clone(), 10.0)
                .unwrap();
            seen.push((packet.pressure.unwrap(), packet.time));
        }
        assert_eq!(
            seen.iter().map(|s| s.0).collect::<Vec<_>>(),
            [0.2, 0.4, 0.6]
        );
        assert!((seen[2].1 - 10.0).abs() < 1e-9, "the newest is now");
        assert!((seen[0].1 - 9.99).abs() < 1e-9, "10 ms before it");
        // The next message: its own packets, the clock kept.
        let next = vec![packet(4, 119, 0.7, 1_015)];
        let packet = ink.take(7, [19.0, 100.0], origin, next, 10.004).unwrap();
        assert_eq!(packet.pressure, Some(0.7));
        assert!((packet.time - 10.005).abs() < 1e-9);
    }

    #[test]
    fn an_event_no_packet_fits_keeps_what_winit_said() {
        let mut ink = InkHistory::default();
        let history = vec![packet(1, 110, 0.2, 1_000)];
        assert!(
            ink.take(1, [60.0, 100.0], [100, 400], history.clone(), 1.0)
                .is_none()
        );
        assert!(
            ink.take(1, [60.0, 100.0], [100, 400], Vec::new(), 1.0)
                .is_none()
        );
        // A packet is not given twice.
        assert!(
            ink.take(1, [10.0, 100.0], [100, 400], history.clone(), 1.0)
                .is_some()
        );
        assert!(
            ink.take(1, [10.0, 100.0], [100, 400], history, 1.0)
                .is_none()
        );
    }
}
