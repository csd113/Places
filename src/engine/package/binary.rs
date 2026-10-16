//! Explicit little-endian binary primitives for compiled world records.
//!
//! These helpers define the byte order, alignment and bounded-length rules the
//! package specification states. Every read is bounds-checked and returns a
//! named error instead of panicking; every string and byte run carries an
//! explicit length that the caller bounds before the read.

/// A growable little-endian writer for one compiled record.
#[derive(Default)]
pub struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    /// An empty writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty writer with a capacity hint.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }

    /// Number of bytes written so far.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    /// True when nothing has been written.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The written bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    /// Consumes the writer and returns its bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// Appends one byte.
    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    /// Appends one little-endian `u16`.
    pub fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Appends one little-endian `u32`.
    pub fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Appends one little-endian `u64`.
    pub fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Appends one little-endian `i32`.
    pub fn i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Appends one little-endian IEEE-754 `f32`.
    pub fn f32(&mut self, value: f32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Appends a boolean as one byte (`0` or `1`).
    pub fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    /// Appends a raw byte run with no length prefix.
    pub fn bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    /// Appends a length-prefixed (`u32`) byte run.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn blob(&mut self, value: &[u8]) -> Result<(), String> {
        let length = u32::try_from(value.len())
            .map_err(|error| format!("byte run is too long to encode: {error}"))?;
        self.u32(length);
        self.bytes(value);
        Ok(())
    }

    /// Appends a length-prefixed (`u32`) UTF-8 string.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn str(&mut self, value: &str) -> Result<(), String> {
        self.blob(value.as_bytes())
    }

    /// Appends an `[f32; 3]`.
    pub fn f32_3(&mut self, value: [f32; 3]) {
        for component in value {
            self.f32(component);
        }
    }

    /// Appends an `[f32; 4]`.
    pub fn f32_4(&mut self, value: [f32; 4]) {
        for component in value {
            self.f32(component);
        }
    }

    /// Appends a `u16` index run with a `u32` element count.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn u16s(&mut self, values: &[u16]) -> Result<(), String> {
        let length = u32::try_from(values.len())
            .map_err(|error| format!("index run is too long to encode: {error}"))?;
        self.u32(length);
        for value in values {
            self.u16(*value);
        }
        Ok(())
    }

    /// Appends a `u32`-counted `f32` run.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn blob_f32s(&mut self, values: &[f32]) -> Result<(), String> {
        let length = u32::try_from(values.len())
            .map_err(|error| format!("float run is too long to encode: {error}"))?;
        self.u32(length);
        for value in values {
            self.f32(*value);
        }
        Ok(())
    }

    /// Appends a `u32`-counted `u32` run.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn blob_u32s(&mut self, values: &[u32]) -> Result<(), String> {
        let length = u32::try_from(values.len())
            .map_err(|error| format!("integer run is too long to encode: {error}"))?;
        self.u32(length);
        for value in values {
            self.u32(*value);
        }
        Ok(())
    }
}

/// A bounds-checked little-endian reader for one compiled record.
pub struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    /// A reader over `bytes`.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    /// Bytes not yet consumed.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.cursor)
    }

    /// True when every byte has been consumed.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Reads exactly `length` bytes.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn bytes(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or_else(|| "record length overflow".to_string())?;
        let slice = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| "record is truncated".to_string())?;
        self.cursor = end;
        Ok(slice)
    }

    /// Reads one byte.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn u8(&mut self) -> Result<u8, String> {
        let bytes = self.bytes(1)?;
        bytes
            .first()
            .copied()
            .ok_or_else(|| "record is truncated".to_string())
    }

    /// Reads one little-endian `u16`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn u16(&mut self) -> Result<u16, String> {
        let bytes: [u8; 2] = self
            .bytes(2)?
            .try_into()
            .map_err(|error| format!("record is truncated: {error}"))?;
        Ok(u16::from_le_bytes(bytes))
    }

    /// Reads one little-endian `u32`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn u32(&mut self) -> Result<u32, String> {
        let bytes: [u8; 4] = self
            .bytes(4)?
            .try_into()
            .map_err(|error| format!("record is truncated: {error}"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Reads one little-endian `u64`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn u64(&mut self) -> Result<u64, String> {
        let bytes: [u8; 8] = self
            .bytes(8)?
            .try_into()
            .map_err(|error| format!("record is truncated: {error}"))?;
        Ok(u64::from_le_bytes(bytes))
    }

    /// Reads one little-endian `i32`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn i32(&mut self) -> Result<i32, String> {
        let bytes: [u8; 4] = self
            .bytes(4)?
            .try_into()
            .map_err(|error| format!("record is truncated: {error}"))?;
        Ok(i32::from_le_bytes(bytes))
    }

    /// Reads one little-endian IEEE-754 `f32`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn f32(&mut self) -> Result<f32, String> {
        let bytes: [u8; 4] = self
            .bytes(4)?
            .try_into()
            .map_err(|error| format!("record is truncated: {error}"))?;
        Ok(f32::from_le_bytes(bytes))
    }

    /// Reads one boolean stored as `0` or `1`; any other byte is an error.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn bool(&mut self) -> Result<bool, String> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(format!("invalid boolean byte {other}")),
        }
    }

    /// Reads a length-prefixed byte run, bounded by `max_length`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn blob(&mut self, max_length: u64) -> Result<Vec<u8>, String> {
        let length = u64::from(self.u32()?);
        if length > max_length {
            return Err(format!(
                "byte run declares {length} bytes (limit {max_length})"
            ));
        }
        let byte_length =
            usize::try_from(length).map_err(|error| format!("byte run is too long: {error}"))?;
        Ok(self.bytes(byte_length)?.to_vec())
    }

    /// Reads a length-prefixed UTF-8 string, bounded by `max_length`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn str(&mut self, max_length: u64) -> Result<String, String> {
        let bytes = self.blob(max_length)?;
        String::from_utf8(bytes).map_err(|error| format!("string is not valid UTF-8: {error}"))
    }

    /// Reads an `[f32; 3]`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn f32_3(&mut self) -> Result<[f32; 3], String> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    /// Reads an `[f32; 4]`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn f32_4(&mut self) -> Result<[f32; 4], String> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }

    /// Reads a `u32`-counted `u16` run, bounded by `max_count`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn u16s(&mut self, max_count: u64) -> Result<Vec<u16>, String> {
        let count = u64::from(self.u32()?);
        if count > max_count {
            return Err(format!(
                "index run declares {count} entries (limit {max_count})"
            ));
        }
        let index_count =
            usize::try_from(count).map_err(|error| format!("index run is too long: {error}"))?;
        let byte_length = index_count
            .checked_mul(2)
            .ok_or_else(|| "index run length overflow".to_string())?;
        let bytes = self.bytes(byte_length)?;
        let mut values = Vec::with_capacity(index_count);
        for pair in bytes.as_chunks::<2>().0 {
            values.push(u16::from_le_bytes(*pair));
        }
        Ok(values)
    }

    /// Reads a `u32`-counted `f32` run, bounded by `max_count`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn f32s(&mut self, max_count: u64) -> Result<Vec<f32>, String> {
        let count = u64::from(self.u32()?);
        if count > max_count {
            return Err(format!(
                "float run declares {count} entries (limit {max_count})"
            ));
        }
        let float_count =
            usize::try_from(count).map_err(|error| format!("float run is too long: {error}"))?;
        let mut values = Vec::with_capacity(float_count.min(16384));
        for _ in 0..float_count {
            values.push(self.f32()?);
        }
        Ok(values)
    }

    /// Reads a `u32`-counted `u32` run, bounded by `max_count`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn u32s(&mut self, max_count: u64) -> Result<Vec<u32>, String> {
        let count = u64::from(self.u32()?);
        if count > max_count {
            return Err(format!(
                "integer run declares {count} entries (limit {max_count})"
            ));
        }
        let integer_count =
            usize::try_from(count).map_err(|error| format!("integer run is too long: {error}"))?;
        let mut values = Vec::with_capacity(integer_count.min(16384));
        for _ in 0..integer_count {
            values.push(self.u32()?);
        }
        Ok(values)
    }

    /// A `u32` element count, bounded by `max_count`.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn count(&mut self, max_count: u64, what: &str) -> Result<usize, String> {
        let count = u64::from(self.u32()?);
        if count > max_count {
            return Err(format!(
                "{what} declares {count} entries (limit {max_count})"
            ));
        }
        usize::try_from(count).map_err(|error| format!("{what} count is too large: {error}"))
    }
}

/// True when every component of `value` is finite.
#[must_use]
pub fn finite3(value: [f32; 3]) -> bool {
    value.iter().all(|component| component.is_finite())
}

/// True when every component of `value` is finite.
#[must_use]
pub fn finite4(value: [f32; 4]) -> bool {
    value.iter().all(|component| component.is_finite())
}
