//! 90-byte `razer_report` codec.
//!
//! Layout and CRC are ported (as protocol facts, not code) from OpenRazer
//! (GPL-2.0). See per-item `// source:` citations below.

/// Total wire size of a `razer_report`.
///
// source: OpenRazer driver/razercommon.h — `static_assert(sizeof(struct razer_report) == 90)`
pub const REPORT_SIZE: usize = 90;

/// Maximum payload length that fits in the arguments field.
pub const MAX_ARGS: usize = 80;

/// Response status byte values.
///
// source: OpenRazer driver/razercommon.h status codes.
pub mod status {
    /// Request accepted, no response yet / send value.
    pub const NEW_COMMAND: u8 = 0x00;
    /// Device busy — retryable.
    pub const BUSY: u8 = 0x01;
    /// Command completed successfully.
    pub const SUCCESSFUL: u8 = 0x02;
    /// Command failed.
    pub const FAILURE: u8 = 0x03;
    /// Command timed out.
    pub const TIMEOUT: u8 = 0x04;
    /// Command not supported.
    pub const NOT_SUPPORTED: u8 = 0x05;
}

/// An in-memory representation of the Razer USB HID report.
///
/// Field offsets, sizes and semantics are taken verbatim from OpenRazer.
///
// source: OpenRazer driver/razercommon.h:124-136 (`struct razer_report`):
//   uint8_t  status;            // @0
//   uint8_t  transaction_id;    // @1  (union in OpenRazer; we keep the raw byte)
//   uint16_t remaining_packets; // @2  BIG-ENDIAN on the wire
//   uint8_t  protocol_type;     // @4
//   uint8_t  data_size;         // @5
//   uint8_t  command_class;     // @6
//   uint8_t  command_id;        // @7  (union; bit7 = direction, 0=set 1=get)
//   uint8_t  arguments[80];     // @8
//   uint8_t  crc;               // @88
//   uint8_t  reserved;          // @89
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RazerReport {
    /// Status byte (@0).
    pub status: u8,
    /// Per-device opaque transaction id (@1), e.g. 0xFF / 0x3F / 0x1F.
    pub transaction_id: u8,
    /// Remaining packets (@2..=3). Kept as raw bytes because the wire format is
    /// **big-endian**, not native-endian. Normally `[0, 0]`.
    // source: OpenRazer driver/razercommon.h — u16 stored big-endian; razer-ctl
    // stored it native-endian, which is the bug we deliberately avoid here.
    pub remaining_packets: [u8; 2],
    /// Protocol type (@4), always 0x00.
    pub protocol_type: u8,
    /// Payload length (@5), <= 80.
    pub data_size: u8,
    /// Command class (@6).
    pub command_class: u8,
    /// Command id (@7); bit 7 selects direction (0 = set, 1 = get).
    pub command_id: u8,
    /// Argument payload (@8..=87), 80 bytes.
    pub arguments: [u8; MAX_ARGS],
    /// CRC (@88): XOR of bytes 2..=87.
    pub crc: u8,
    /// Reserved (@89), always 0x00.
    pub reserved: u8,
}

impl Default for RazerReport {
    fn default() -> Self {
        Self {
            status: status::NEW_COMMAND,
            transaction_id: 0xFF,
            remaining_packets: [0, 0],
            protocol_type: 0x00,
            data_size: 0,
            command_class: 0,
            command_id: 0,
            arguments: [0u8; MAX_ARGS],
            crc: 0,
            reserved: 0,
        }
    }
}

impl RazerReport {
    /// Build a fresh request report and compute its CRC.
    ///
    /// `data_size` is set to `args.len()` and the first `args.len()` bytes of the
    /// arguments field are populated. `status`, `protocol_type` and
    /// `remaining_packets` are left at their send-time defaults (0 / 0 / [0,0]).
    ///
    /// # Panics
    /// Panics only on a programming error: `args.len() > MAX_ARGS`. This is not a
    /// device-I/O path — callers construct commands from compile-time-known sizes.
    pub fn new(transaction_id: u8, command_class: u8, command_id: u8, args: &[u8]) -> Self {
        assert!(
            args.len() <= MAX_ARGS,
            "argument payload {} exceeds {MAX_ARGS} bytes",
            args.len()
        );
        let mut arguments = [0u8; MAX_ARGS];
        arguments[..args.len()].copy_from_slice(args);

        let mut report = Self {
            status: status::NEW_COMMAND,
            transaction_id,
            remaining_packets: [0, 0],
            protocol_type: 0x00,
            data_size: args.len() as u8,
            command_class,
            command_id,
            arguments,
            crc: 0,
            reserved: 0,
        };
        report.crc = report.compute_crc();
        report
    }

    /// Compute the report CRC: XOR of bytes 2..=87 of the serialized form.
    ///
    // source: OpenRazer driver/razercommon.c `razer_calculate_crc()` —
    //   crc ^= report_bytes[i] for i in 2..=87 (i.e. from remaining_packets
    //   through the last argument byte, excluding status/transaction_id and the
    //   crc/reserved trailer).
    pub fn compute_crc(&self) -> u8 {
        let bytes = self.to_bytes();
        let mut crc = 0u8;
        for &b in &bytes[2..=87] {
            crc ^= b;
        }
        crc
    }

    /// Serialize to the 90-byte wire representation.
    pub fn to_bytes(&self) -> [u8; REPORT_SIZE] {
        let mut buf = [0u8; REPORT_SIZE];
        buf[0] = self.status;
        buf[1] = self.transaction_id;
        buf[2] = self.remaining_packets[0];
        buf[3] = self.remaining_packets[1];
        buf[4] = self.protocol_type;
        buf[5] = self.data_size;
        buf[6] = self.command_class;
        buf[7] = self.command_id;
        buf[8..88].copy_from_slice(&self.arguments);
        buf[88] = self.crc;
        buf[89] = self.reserved;
        buf
    }

    /// Parse from the 90-byte wire representation.
    pub fn from_bytes(buf: &[u8; REPORT_SIZE]) -> Self {
        let mut arguments = [0u8; MAX_ARGS];
        arguments.copy_from_slice(&buf[8..88]);
        Self {
            status: buf[0],
            transaction_id: buf[1],
            remaining_packets: [buf[2], buf[3]],
            protocol_type: buf[4],
            data_size: buf[5],
            command_class: buf[6],
            command_id: buf[7],
            arguments,
            crc: buf[88],
            reserved: buf[89],
        }
    }

    /// True when the stored CRC matches a freshly computed one.
    pub fn crc_valid(&self) -> bool {
        self.crc == self.compute_crc()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_round_trip() {
        let report = RazerReport::new(0x3F, 0x04, 0x05, &[0x01, 0x02, 0x03, 0x04, 0x05]);
        let bytes = report.to_bytes();
        assert_eq!(bytes.len(), REPORT_SIZE);
        let parsed = RazerReport::from_bytes(&bytes);
        assert_eq!(report, parsed);
    }

    #[test]
    fn constructor_sets_data_size() {
        let args = [0xAA, 0xBB, 0xCC];
        let report = RazerReport::new(0xFF, 0x00, 0x82, &args);
        assert_eq!(report.data_size, 3);
        assert_eq!(&report.arguments[..3], &args);
        // Remaining arg bytes must be zero-filled.
        assert!(report.arguments[3..].iter().all(|&b| b == 0));
    }

    #[test]
    fn crc_matches_independent_xor() {
        // Known-good hand computation. Build a report with fixed fields and
        // XOR bytes 2..=87 independently.
        let tx = 0x1F;
        let class = 0x0F;
        let cmd = 0x02;
        let args = [0x01, 0x00, 0x01, 0x03]; // varstore, led, effect=spectrum
        let report = RazerReport::new(tx, class, cmd, &args);

        // Independent CRC: XOR of remaining_packets(2) + protocol(1) + data_size(1)
        // + class + cmd + all 80 arg bytes. remaining_packets and protocol are 0,
        // so they contribute nothing.
        let mut expected = 0u8;
        expected ^= 0x00; // remaining_packets[0]
        expected ^= 0x00; // remaining_packets[1]
        expected ^= 0x00; // protocol_type
        expected ^= args.len() as u8; // data_size = 4
        expected ^= class;
        expected ^= cmd;
        for &b in &args {
            expected ^= b;
        }
        // trailing 76 zero arg bytes contribute nothing.
        assert_eq!(report.crc, expected);
        assert!(report.crc_valid());

        // Spell out the arithmetic: 0x04 ^ 0x0F ^ 0x02 ^ 0x01 ^ 0x00 ^ 0x01 ^ 0x03
        let manual = 0x04u8 ^ 0x0F ^ 0x02 ^ 0x01 ^ 0x00 ^ 0x01 ^ 0x03;
        assert_eq!(report.crc, manual);
    }

    #[test]
    fn crc_placed_at_byte_88() {
        let report = RazerReport::new(0xFF, 0x00, 0x81, &[]);
        let bytes = report.to_bytes();
        assert_eq!(bytes[88], report.crc);
        assert_eq!(bytes[89], 0x00); // reserved
    }

    #[test]
    fn remaining_packets_is_big_endian_bytes() {
        let mut report = RazerReport::new(0xFF, 0x00, 0x00, &[]);
        report.remaining_packets = [0x12, 0x34]; // BE u16 = 0x1234
        let bytes = report.to_bytes();
        assert_eq!(bytes[2], 0x12);
        assert_eq!(bytes[3], 0x34);
    }
}
