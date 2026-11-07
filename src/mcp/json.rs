// ─────────────────────────────────────────────────────────────────────────────
//  Zero-Dependency Byte-Slice JSON Parser, Serializer & Base64 Encoder
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Clone, Debug, PartialEq)]
pub enum JsonVal {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<JsonVal>),
    Obj(Vec<(String, JsonVal)>),
}

impl JsonVal {
    pub fn get(&self, key: &str) -> Option<&JsonVal> {
        match self {
            Self::Obj(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Num(n) if *n >= 0.0 => Some(*n as u64),
            _ => None,
        }
    }

    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(128);
        self.write_json(&mut out);
        out
    }

    fn write_json(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    out.push_str(&format!("{:.0}", n));
                } else {
                    out.push_str(&n.to_string());
                }
            }
            Self::Str(s) => write_escaped_str(s, out),
            Self::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_json(out);
                }
                out.push(']');
            }
            Self::Obj(entries) => {
                out.push('{');
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_escaped_str(k, out);
                    out.push(':');
                    v.write_json(out);
                }
                out.push('}');
            }
        }
    }

    /// Parses JSON directly over `&[u8]` without allocating a `Vec<char>`
    pub fn parse(input: &str) -> Result<Self, String> {
        let bytes = input.as_bytes();
        let mut pos = 0;
        let val = parse_value(bytes, &mut pos)?;
        skip_ws(bytes, &mut pos);
        Ok(val)
    }
}

fn write_escaped_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn skip_ws(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && bytes[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
}

fn parse_value(bytes: &[u8], pos: &mut usize) -> Result<JsonVal, String> {
    skip_ws(bytes, pos);
    let b = *bytes
        .get(*pos)
        .ok_or_else(|| "Unexpected EOF".to_string())?;
    match b {
        b'"' => parse_string(bytes, pos).map(JsonVal::Str),
        b'{' => parse_object(bytes, pos),
        b'[' => parse_array(bytes, pos),
        b't' if bytes[*pos..].starts_with(b"true") => {
            *pos += 4;
            Ok(JsonVal::Bool(true))
        }
        b'f' if bytes[*pos..].starts_with(b"false") => {
            *pos += 5;
            Ok(JsonVal::Bool(false))
        }
        b'n' if bytes[*pos..].starts_with(b"null") => {
            *pos += 4;
            Ok(JsonVal::Null)
        }
        c if c == b'-' || c.is_ascii_digit() => parse_number(bytes, pos),
        other => Err(format!("Unexpected byte '{}'", other as char)),
    }
}

fn parse_string(bytes: &[u8], pos: &mut usize) -> Result<String, String> {
    *pos += 1;
    let mut out = String::new();
    while *pos < bytes.len() {
        let start = *pos;
        while *pos < bytes.len() && bytes[*pos] != b'"' && bytes[*pos] != b'\\' {
            *pos += 1;
        }
        if *pos > start {
            let chunk = std::str::from_utf8(&bytes[start..*pos])
                .map_err(|_| "Invalid UTF-8 in JSON string".to_string())?;
            out.push_str(chunk);
        }
        let b = *bytes
            .get(*pos)
            .ok_or_else(|| "Unterminated string".to_string())?;
        *pos += 1;
        match b {
            b'"' => return Ok(out),
            b'\\' => {
                let esc = *bytes
                    .get(*pos)
                    .ok_or_else(|| "Unterminated escape".to_string())?;
                *pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let slice = bytes
                            .get(*pos..*pos + 4)
                            .ok_or_else(|| "Invalid \\u escape".to_string())?;
                        let hex = std::str::from_utf8(slice)
                            .map_err(|_| "Invalid UTF-8 in \\u escape".to_string())?;
                        *pos += 4;
                        let code = u32::from_str_radix(hex, 16)
                            .map_err(|_| "Invalid hex in \\u escape".to_string())?;
                        if let Some(ch) = char::from_u32(code) {
                            out.push(ch);
                        }
                    }
                    other => out.push(other as char),
                }
            }
            _ => unreachable!(),
        }
    }
    Err("Unterminated string".to_string())
}

fn parse_number(bytes: &[u8], pos: &mut usize) -> Result<JsonVal, String> {
    let start = *pos;
    if bytes[*pos] == b'-' {
        *pos += 1;
    }
    while *pos < bytes.len()
        && (bytes[*pos].is_ascii_digit() || matches!(bytes[*pos], b'.' | b'e' | b'E' | b'+' | b'-'))
    {
        *pos += 1;
    }
    let num_str = std::str::from_utf8(&bytes[start..*pos])
        .map_err(|_| "Invalid UTF-8 in number".to_string())?;
    let n = num_str
        .parse::<f64>()
        .map_err(|_| format!("Invalid number '{}'", num_str))?;
    Ok(JsonVal::Num(n))
}

fn parse_array(bytes: &[u8], pos: &mut usize) -> Result<JsonVal, String> {
    *pos += 1;
    let mut items = Vec::new();
    loop {
        skip_ws(bytes, pos);
        let b = *bytes
            .get(*pos)
            .ok_or_else(|| "Unterminated array".to_string())?;
        if b == b']' {
            *pos += 1;
            return Ok(JsonVal::Arr(items));
        }
        items.push(parse_value(bytes, pos)?);
        skip_ws(bytes, pos);
        if bytes.get(*pos) == Some(&b',') {
            *pos += 1;
        }
    }
}

fn parse_object(bytes: &[u8], pos: &mut usize) -> Result<JsonVal, String> {
    *pos += 1;
    let mut entries = Vec::new();
    loop {
        skip_ws(bytes, pos);
        let b = *bytes
            .get(*pos)
            .ok_or_else(|| "Unterminated object".to_string())?;
        if b == b'}' {
            *pos += 1;
            return Ok(JsonVal::Obj(entries));
        }
        if b != b'"' {
            return Err("Expected string key in object".to_string());
        }
        let key = parse_string(bytes, pos)?;
        skip_ws(bytes, pos);
        if bytes.get(*pos) != Some(&b':') {
            return Err("Expected ':' after object key".to_string());
        }
        *pos += 1;
        let val = parse_value(bytes, pos)?;
        entries.push((key, val));
        skip_ws(bytes, pos);
        if bytes.get(*pos) == Some(&b',') {
            *pos += 1;
        }
    }
}

pub fn j_str(s: impl Into<String>) -> JsonVal {
    JsonVal::Str(s.into())
}

pub fn j_bool(b: bool) -> JsonVal {
    JsonVal::Bool(b)
}

pub fn j_num(n: impl Into<f64>) -> JsonVal {
    JsonVal::Num(n.into())
}

pub fn j_arr(items: impl Into<Vec<JsonVal>>) -> JsonVal {
    JsonVal::Arr(items.into())
}

pub fn j_obj(entries: impl Into<Vec<(&'static str, JsonVal)>>) -> JsonVal {
    JsonVal::Obj(
        entries
            .into()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(B64_TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64_TABLE[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_TABLE[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_TABLE[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}
