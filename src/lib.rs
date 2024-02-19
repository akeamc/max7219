//! A platform agnostic driver to interface with the MAX7219 (LED matrix display driver)
//!
//! This driver was built using [`embedded-hal-async`] traits.
//!
//! [`embedded-hal-async`]: https://docs.rs/embedded-hal-async/~1.0

#![deny(unsafe_code)]
#![deny(warnings)]
#![no_std]

use embedded_hal_async::spi::SpiDevice;

/// Maximum number of displays connected in series supported by this lib.
pub const MAX_DISPLAYS: usize = 8;

/// Digits per display
pub const NUM_DIGITS: usize = 8;

/// Possible command register values on the display chip.
#[derive(Clone, Copy)]
pub enum Register {
    Noop = 0x00,
    Digit0 = 0x01,
    Digit1 = 0x02,
    Digit2 = 0x03,
    Digit3 = 0x04,
    Digit4 = 0x05,
    Digit5 = 0x06,
    Digit6 = 0x07,
    Digit7 = 0x08,
    DecodeMode = 0x09,
    Intensity = 0x0A,
    ScanLimit = 0x0B,
    Power = 0x0C,
    DisplayTest = 0x0F,
}

impl From<Register> for u8 {
    fn from(command: Register) -> u8 {
        command as u8
    }
}

/// Decode modes for BCD encoded input.
#[derive(Copy, Clone)]
pub enum DecodeMode {
    NoDecode = 0x00,
    CodeBDigit0 = 0x01,
    CodeBDigits3_0 = 0x0F,
    CodeBDigits7_0 = 0xFF,
}

pub struct Max7219<SPI> {
    pub spi: SPI,
}

impl<SPI> Max7219<SPI>
where
    SPI: SpiDevice,
{
    pub async fn write_reg(&mut self, register: impl Into<u8>, data: u8) -> Result<(), SPI::Error> {
        self.spi.write(&[register.into(), data]).await
    }

    /// Power on
    pub async fn power_on(&mut self) -> Result<(), SPI::Error> {
        self.write_reg(Register::Power, 0x01).await
    }

    /// Powers off all connected displays
    pub async fn power_off(&mut self) -> Result<(), SPI::Error> {
        self.write_reg(Register::Power, 0x00).await
    }

    /// Clears display by settings all digits to empty
    pub async fn clear_display(&mut self) -> Result<(), SPI::Error> {
        self.write_raw(&[0; NUM_DIGITS]).await
    }

    /// Sets intensity level on the display
    ///
    /// # Arguments
    ///
    /// * `intensity` - intensity value to set to `0x00` to 0x0F`
    pub async fn set_intensity(&mut self, intensity: u8) -> Result<(), SPI::Error> {
        self.write_reg(Register::Intensity, intensity).await
    }

    /// Sets decode mode to be used on input sent to the display chip.
    ///
    /// # Arguments
    ///
    /// * `mode` - the decode mode to set
    pub async fn set_decode_mode(&mut self, mode: DecodeMode) -> Result<(), SPI::Error> {
        self.write_reg(Register::DecodeMode, mode as u8).await
    }

    /// Writes byte string to the display
    ///
    /// # Arguments
    ///
    /// * `string` - the byte string to send 8 bytes long. Unknown characters result in question mark.
    /// * `dots` - u8 bit array specifying where to put dots in the string (1 = dot, 0 = not)
    pub async fn write_str(
        &mut self,
        string: &[u8; NUM_DIGITS],
        dots: u8,
    ) -> Result<(), SPI::Error> {
        for (i, b) in string.iter().enumerate() {
            let reg = NUM_DIGITS as u8 - i as u8; // reverse order
            self.write_reg(reg, ssb_byte(*b, (dots & (1 << i)) != 0))
                .await?;
        }

        Ok(())
    }

    /// Writes a right justified integer with sign
    ///
    /// # Arguments
    ///
    /// * `val` - an integer i32
    pub async fn write_integer(&mut self, value: i32) -> Result<(), SPI::Error> {
        let mut buf = [0u8; 8];
        let j = base_10_bytes(value, &mut buf);
        buf = pad_left(j);
        self.write_str(&buf, 0b00000000).await
    }

    /// Writes a right justified hex formatted integer with sign
    ///
    /// # Arguments
    ///
    /// * `val` - an integer i32
    pub async fn write_hex(&mut self, value: u32) -> Result<(), SPI::Error> {
        let mut buf = [0u8; 8];
        let j = hex_bytes(value, &mut buf);
        buf = pad_left(j);
        self.write_str(&buf, 0b00000000).await
    }

    /// Writes a raw value to the display
    ///
    /// # Arguments
    ///
    /// * `raw` - an array of raw bytes to write. Each bit represents a pixel on the display
    pub async fn write_raw(&mut self, raw: &[u8; NUM_DIGITS]) -> Result<(), SPI::Error> {
        for (n, b) in raw.iter().enumerate() {
            self.write_reg(n as u8 + 1, *b).await?;
        }
        Ok(())
    }

    /// Set test mode on/off
    ///
    /// # Arguments
    ///
    /// * `is_on` - whether to turn test mode on or off
    pub async fn set_test(&mut self, is_on: bool) -> Result<(), SPI::Error> {
        self.write_reg(Register::DisplayTest, if is_on { 0x01 } else { 0x00 })
            .await
    }

    pub async fn new(spi: SPI) -> Result<Self, SPI::Error> {
        let mut max7219 = Max7219 { spi };

        max7219.init().await?;
        Ok(max7219)
    }

    async fn init(&mut self) -> Result<(), SPI::Error> {
        self.set_test(false).await?; // turn testmode off
        self.write_reg(Register::ScanLimit, 0x07).await?; // set scanlimit
        self.set_decode_mode(DecodeMode::NoDecode).await?; // direct decode
        self.clear_display().await?; // clear all digits
        self.power_off().await?; // power off

        Ok(())
    }
}

///
/// Translate alphanumeric ASCII bytes into segment set bytes
///
fn ssb_byte(b: u8, dot: bool) -> u8 {
    let mut result = match b as char {
        ' ' => 0b0000_0000, // "blank"
        '.' => 0b1000_0000,
        '-' => 0b0000_0001, // -
        '_' => 0b0000_1000, // _
        '0' => 0b0111_1110,
        '1' => 0b0011_0000,
        '2' => 0b0110_1101,
        '3' => 0b0111_1001,
        '4' => 0b0011_0011,
        '5' => 0b0101_1011,
        '6' => 0b0101_1111,
        '7' => 0b0111_0000,
        '8' => 0b0111_1111,
        '9' => 0b0111_1011,
        'a' | 'A' => 0b0111_0111,
        'b' => 0b0001_1111,
        'c' | 'C' => 0b0100_1110,
        'd' => 0b0011_1101,
        'e' | 'E' => 0b0100_1111,
        'f' | 'F' => 0b0100_0111,
        'g' | 'G' => 0b0101_1110,
        'h' | 'H' => 0b0011_0111,
        'i' | 'I' => 0b0011_0000,
        'j' | 'J' => 0b0011_1100,
        // K undoable
        'l' | 'L' => 0b0000_1110,
        // M undoable
        'n' | 'N' => 0b0001_0101,
        'o' | 'O' => 0b0111_1110,
        'p' | 'P' => 0b0110_0111,
        'q' => 0b0111_0011,
        'r' | 'R' => 0b0000_0101,
        's' | 'S' => 0b0101_1011,
        // T undoable
        'u' | 'U' => 0b0011_1110,
        // V undoable
        // W undoable
        // X undoable
        // Y undoable
        // Z undoable
        _ => 0b1110_0101, // ?
    };

    if dot {
        result |= 0b1000_0000; // turn "." on
    }

    result
}

/// Convert the integer into an integer byte Sequence
fn base_10_bytes(mut n: i32, buf: &mut [u8]) -> &[u8] {
    let mut sign: bool = false;
    if n == 0 {
        return b"0";
    }
    //don't overflow the display
    if !(-9999999..=99999999).contains(&n) {
        return b"Err";
    }
    if n < 0 {
        n = -n;
        sign = true;
    }
    let mut i = 0;
    while n > 0 {
        buf[i] = (n % 10) as u8 + b'0';
        n /= 10;
        i += 1;
    }
    if sign {
        buf[i] = b'-';
        i += 1;
    }
    let slice = &mut buf[..i];
    slice.reverse();
    &*slice
}

/// Convert the integer into a hexidecimal byte sequence
fn hex_bytes(mut n: u32, buf: &mut [u8]) -> &[u8] {
    if n == 0 {
        return b"0";
    }
    let mut i = 0;
    while n > 0 {
        let digit = (n % 16) as u8;
        buf[i] = match digit {
            0 => b'0',
            1 => b'1',
            2 => b'2',
            3 => b'3',
            4 => b'4',
            5 => b'5',
            6 => b'6',
            7 => b'7',
            8 => b'8',
            9 => b'9',
            10 => b'a',
            11 => b'b',
            12 => b'c',
            13 => b'd',
            14 => b'e',
            15 => b'f',
            _ => b'?',
        };
        n /= 16;
        i += 1;
    }
    let slice = &mut buf[..i];
    slice.reverse();
    &*slice
}

/// Take a byte slice and pad the left hand side
fn pad_left(val: &[u8]) -> [u8; 8] {
    assert!(val.len() <= 8);
    let size: usize = 8;
    let pos: usize = val.len();
    let mut cur: usize = 1;
    let mut out: [u8; 8] = *b"        ";
    while cur <= pos {
        out[size - cur] = val[pos - cur];
        cur += 1;
    }
    out
}
