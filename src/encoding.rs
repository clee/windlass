/// Message decoding error
#[derive(thiserror::Error, Debug, Clone)]
pub enum MessageDecodeError {
    /// More data was expected but none is available
    #[error("eof unexpected")]
    UnexpectedEof,
    /// A received string could not be decoded as UTF8
    #[error("invalid utf8 string")]
    Utf8Error(#[from] std::str::Utf8Error),
}

fn encode_vlq_int(output: &mut Vec<u8>, v: u32) {
    let sv = v as i32;
    if !(-(1 << 26)..(3 << 26)).contains(&sv) {
        output.push(((sv >> 28) & 0x7F) as u8 | 0x80);
    }
    if !(-(1 << 19)..(3 << 19)).contains(&sv) {
        output.push(((sv >> 21) & 0x7F) as u8 | 0x80);
    }
    if !(-(1 << 12)..(3 << 12)).contains(&sv) {
        output.push(((sv >> 14) & 0x7F) as u8 | 0x80);
    }
    if !(-(1 << 5)..(3 << 5)).contains(&sv) {
        output.push(((sv >> 7) & 0x7F) as u8 | 0x80);
    }
    output.push((sv & 0x7F) as u8);
}

pub(crate) fn next_byte(data: &mut &[u8]) -> Result<u8, MessageDecodeError> {
    if data.is_empty() {
        Err(MessageDecodeError::UnexpectedEof)
    } else {
        let v = data[0];
        *data = &data[1..];
        Ok(v)
    }
}

fn parse_vlq_int(data: &mut &[u8]) -> Result<u32, MessageDecodeError> {
    let mut c = next_byte(data)? as u32;
    let mut v = c & 0x7F;
    if (c & 0x60) == 0x60 {
        v |= (-0x20i32) as u32;
    }
    while c & 0x80 != 0 {
        c = next_byte(data)? as u32;
        v = (v << 7) | (c & 0x7F);
    }

    Ok(v)
}

pub trait Readable<'de>: Sized {
    fn read(data: &mut &'de [u8]) -> Result<Self, MessageDecodeError>;

    fn skip(data: &mut &[u8]) -> Result<(), MessageDecodeError>;
}

pub trait Writable: Sized {
    fn write(&self, output: &mut Vec<u8>);
}

pub trait Ownable: Sized {
    type Owned;
    fn to_owned(&self) -> Self::Owned;
}

pub trait ToFieldType: Sized {
    fn as_field_type() -> FieldType;
}

macro_rules! int_readwrite {
    ( $type:tt, $field_type:expr ) => {
        impl Readable<'_> for $type {
            fn read(data: &mut &[u8]) -> Result<Self, MessageDecodeError> {
                parse_vlq_int(data).map(|v| v as $type)
            }

            fn skip(data: &mut &[u8]) -> Result<(), MessageDecodeError> {
                parse_vlq_int(data).map(|_| ())
            }
        }

        impl Writable for $type {
            fn write(&self, output: &mut Vec<u8>) {
                encode_vlq_int(output, *self as u32)
            }
        }

        impl Ownable for $type {
            type Owned = Self;
            fn to_owned(&self) -> Self::Owned {
                *self
            }
        }

        impl ToFieldType for $type {
            fn as_field_type() -> FieldType {
                $field_type
            }
        }
    };
}

int_readwrite!(u32, FieldType::U32);
int_readwrite!(i32, FieldType::I32);
int_readwrite!(u16, FieldType::U16);
int_readwrite!(i16, FieldType::I16);
int_readwrite!(u8, FieldType::U8);

impl Readable<'_> for bool {
    fn read(data: &mut &[u8]) -> Result<Self, MessageDecodeError> {
        parse_vlq_int(data).map(|v| v != 0)
    }

    fn skip(data: &mut &[u8]) -> Result<(), MessageDecodeError> {
        parse_vlq_int(data).map(|_| ())
    }
}

impl Writable for bool {
    fn write(&self, output: &mut Vec<u8>) {
        encode_vlq_int(output, u32::from(*self))
    }
}

impl Ownable for bool {
    type Owned = Self;
    fn to_owned(&self) -> Self::Owned {
        *self
    }
}

impl ToFieldType for bool {
    fn as_field_type() -> FieldType {
        FieldType::U8
    }
}

impl<'de> Readable<'de> for &'de [u8] {
    fn read(data: &mut &'de [u8]) -> Result<&'de [u8], MessageDecodeError> {
        let len = parse_vlq_int(data)? as usize;
        if data.len() < len {
            Err(MessageDecodeError::UnexpectedEof)
        } else {
            let ret = &data[..len];
            *data = &data[len..];
            Ok(ret)
        }
    }

    fn skip(data: &mut &[u8]) -> Result<(), MessageDecodeError> {
        let len = parse_vlq_int(data)? as usize;
        if data.len() < len {
            Err(MessageDecodeError::UnexpectedEof)
        } else {
            *data = &data[len..];
            Ok(())
        }
    }
}

impl Writable for &[u8] {
    fn write(&self, output: &mut Vec<u8>) {
        encode_vlq_int(output, self.len() as u32);
        output.extend_from_slice(self);
    }
}

impl Ownable for &[u8] {
    type Owned = Vec<u8>;
    fn to_owned(&self) -> Self::Owned {
        self.to_vec()
    }
}

impl ToFieldType for &[u8] {
    fn as_field_type() -> FieldType {
        FieldType::ByteArray
    }
}

impl<'de> Readable<'de> for &'de str {
    fn read(data: &mut &'de [u8]) -> Result<&'de str, MessageDecodeError> {
        let len = parse_vlq_int(data)? as usize;
        if data.len() < len {
            Err(MessageDecodeError::UnexpectedEof)
        } else {
            let ret = &data[..len];
            *data = &data[len..];
            Ok(std::str::from_utf8(ret)?)
        }
    }

    fn skip(data: &mut &[u8]) -> Result<(), MessageDecodeError> {
        let len = parse_vlq_int(data)? as usize;
        if data.len() < len {
            Err(MessageDecodeError::UnexpectedEof)
        } else {
            *data = &data[len..];
            Ok(())
        }
    }
}

impl Writable for &str {
    fn write(&self, output: &mut Vec<u8>) {
        let bytes = self.as_bytes();
        encode_vlq_int(output, bytes.len() as u32);
        output.extend_from_slice(bytes);
    }
}

impl Ownable for &str {
    type Owned = String;
    fn to_owned(&self) -> Self::Owned {
        self.to_string()
    }
}

impl ToFieldType for &str {
    fn as_field_type() -> FieldType {
        FieldType::String
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FieldType {
    U32,
    I32,
    U16,
    I16,
    U8,
    String,
    ByteArray,
}

impl FieldType {
    pub(crate) fn skip(&self, input: &mut &[u8]) -> Result<(), MessageDecodeError> {
        match self {
            Self::U32 => <u32 as Readable>::skip(input),
            Self::I32 => <i32 as Readable>::skip(input),
            Self::U16 => <u16 as Readable>::skip(input),
            Self::I16 => <i16 as Readable>::skip(input),
            Self::U8 => <u8 as Readable>::skip(input),
            Self::String => <&str as Readable>::skip(input),
            Self::ByteArray => <&[u8] as Readable>::skip(input),
        }
    }

    pub(crate) fn read(&self, input: &mut &[u8]) -> Result<FieldValue, MessageDecodeError> {
        Ok(match self {
            Self::U32 => FieldValue::U32(<u32 as Readable>::read(input)?),
            Self::I32 => FieldValue::I32(<i32 as Readable>::read(input)?),
            Self::U16 => FieldValue::U16(<u16 as Readable>::read(input)?),
            Self::I16 => FieldValue::I16(<i16 as Readable>::read(input)?),
            Self::U8 => FieldValue::U8(<u8 as Readable>::read(input)?),
            Self::String => FieldValue::String(<&str as Readable>::read(input)?.into()),
            Self::ByteArray => FieldValue::ByteArray(<&[u8] as Readable>::read(input)?.into()),
        })
    }
}

#[derive(Debug)]
pub enum FieldValue {
    U32(u32),
    I32(i32),
    U16(u16),
    I16(i16),
    U8(u8),
    String(String),
    ByteArray(Vec<u8>),
}
