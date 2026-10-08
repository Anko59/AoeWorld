#[derive(Clone, Copy)]
enum LexState {
    Code,
    String,
    Character,
    ByteString,
    ByteCharacter,
    Raw(usize),
    LineComment,
    BlockComment(usize),
}

#[derive(Clone, Copy)]
enum LineKind {
    Comment,
    Code,
    Blank,
}

#[derive(Default)]
struct Scan {
    lines: u64,
    comments: u64,
    tests: u64,
    code: bool,
    comment: bool,
    prefix: [u8; 7],
    prefix_len: usize,
    prefix_candidate: bool,
}

impl Scan {
    fn finish_line(&mut self) {
        let _kind = if self.comment {
            LineKind::Comment
        } else if self.code {
            LineKind::Code
        } else {
            LineKind::Blank
        };
        self.lines += 1;
        self.comments += u64::from(self.comment);
        if self.prefix_candidate
            && self.prefix_len == 7
            && (&self.prefix[..] == b"#[test]" || &self.prefix[..] == b"#[test(")
        {
            self.tests += 1;
        }
        self.code = false;
        self.comment = false;
        self.prefix = [0; 7];
        self.prefix_len = 0;
        self.prefix_candidate = true;
    }

    fn push_code(&mut self, byte: u8) {
        if !byte.is_ascii_whitespace() {
            self.code = true;
        }
        if !self.prefix_candidate || byte.is_ascii_whitespace() {
            return;
        }
        if self.prefix_len < self.prefix.len() {
            self.prefix[self.prefix_len] = byte;
            self.prefix_len += 1;
            let part = &self.prefix[..self.prefix_len];
            if !b"#[test(".starts_with(part) && !b"#[test]".starts_with(part) {
                self.prefix_candidate = false;
            }
        } else {
            self.prefix_candidate = false;
        }
    }
}

/// A single lexer pass counts source lines, comment lines, and code-state
/// `#[test]` attributes. Macro-generated tokens and embedded languages are not
/// parsed as Rust syntax.
pub(super) fn scan_rust(source: &str) -> (u64, u64, u64) {
    let bytes = source.as_bytes();
    let mut state = LexState::Code;
    let mut scan = Scan {
        prefix_candidate: true,
        ..Scan::default()
    };
    let mut i = 0;
    while i < bytes.len() {
        match state {
            LexState::Code => match bytes[i] {
                b'\n' => {
                    scan.finish_line();
                    i += 1;
                }
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    state = LexState::LineComment;
                    scan.comment = true;
                    i += 2;
                }
                b'/' if bytes.get(i + 1) == Some(&b'*') => {
                    state = LexState::BlockComment(1);
                    scan.comment = true;
                    i += 2;
                }
                b'r' | b'b' => {
                    if let Some((hashes, end)) = raw_string_start(bytes, i) {
                        scan.code = true;
                        scan.prefix_candidate = false;
                        state = LexState::Raw(hashes);
                        i = end;
                    } else if bytes[i] == b'b' && bytes.get(i + 1) == Some(&b'"') {
                        scan.code = true;
                        scan.prefix_candidate = false;
                        state = LexState::ByteString;
                        i += 2;
                    } else if bytes[i] == b'b' && bytes.get(i + 1) == Some(&b'\'') {
                        scan.code = true;
                        scan.prefix_candidate = false;
                        state = LexState::ByteCharacter;
                        i += 2;
                    } else {
                        scan.push_code(bytes[i]);
                        i += 1;
                    }
                }
                b'"' => {
                    scan.code = true;
                    scan.prefix_candidate = false;
                    state = LexState::String;
                    i += 1;
                }
                b'\'' if char_end(bytes, i).is_some() => {
                    scan.code = true;
                    scan.prefix_candidate = false;
                    state = LexState::Character;
                    i += 1;
                }
                _ => {
                    scan.push_code(bytes[i]);
                    i += 1;
                }
            },
            LexState::String
            | LexState::Character
            | LexState::ByteString
            | LexState::ByteCharacter => {
                let delimiter = if matches!(state, LexState::Character | LexState::ByteCharacter) {
                    b'\''
                } else {
                    b'"'
                };
                if bytes[i] == b'\\' && bytes.get(i + 1) == Some(&b'\n') {
                    scan.finish_line();
                    scan.code = true;
                    scan.prefix_candidate = false;
                    i += 2;
                } else if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                } else if bytes[i] == delimiter {
                    state = LexState::Code;
                    i += 1;
                } else if bytes[i] == b'\n' {
                    scan.finish_line();
                    scan.code = true;
                    scan.prefix_candidate = false;
                    i += 1;
                } else {
                    i += 1;
                }
            }
            LexState::Raw(hashes) => {
                if bytes[i] == b'"' {
                    let quote = i;
                    i += 1;
                    while bytes.get(i) == Some(&b'#') {
                        i += 1;
                    }
                    if i - quote > hashes {
                        state = LexState::Code;
                        i = quote + hashes + 1;
                    }
                } else {
                    if bytes[i] == b'\n' {
                        scan.finish_line();
                        scan.code = true;
                        scan.prefix_candidate = false;
                    }
                    i += 1;
                }
            }
            LexState::LineComment => {
                if bytes[i] == b'\n' {
                    scan.finish_line();
                    state = LexState::Code;
                }
                i += 1;
            }
            LexState::BlockComment(depth) => {
                scan.comment = true;
                match (bytes[i], bytes.get(i + 1)) {
                    (b'/', Some(b'*')) => {
                        state = LexState::BlockComment(depth + 1);
                        i += 2;
                    }
                    (b'*', Some(b'/')) => {
                        state = if depth == 1 {
                            LexState::Code
                        } else {
                            LexState::BlockComment(depth - 1)
                        };
                        i += 2;
                    }
                    (b'\n', _) => {
                        scan.finish_line();
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
        }
    }
    if i > 0 && bytes.last() != Some(&b'\n') {
        scan.finish_line();
    }
    (scan.lines, scan.comments, scan.tests)
}

#[cfg(test)]
pub(super) fn comment_lines(source: &str) -> u64 {
    scan_rust(source).1
}

fn raw_string_start(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut i = start;
    if bytes.get(i) == Some(&b'b') {
        i += 1;
    }
    if bytes.get(i) != Some(&b'r') {
        return None;
    }
    i += 1;
    let hash_start = i;
    while bytes.get(i) == Some(&b'#') {
        i += 1;
    }
    (bytes.get(i) == Some(&b'"')).then_some((i - hash_start, i + 1))
}

fn char_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    if i >= bytes.len() || bytes[i] == b'\n' {
        return None;
    }
    if bytes[i] == b'\\' {
        i += 1;
        match bytes.get(i).copied()? {
            b'x' => i += 3,
            b'u' if bytes.get(i + 1) == Some(&b'{') => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'}' && bytes[i] != b'\n' {
                    i += 1;
                }
                if bytes.get(i) != Some(&b'}') {
                    return None;
                }
                i += 1;
            }
            b'\n' => return None,
            _ => i += 1,
        }
    } else if bytes[i].is_ascii() {
        i += 1;
    } else {
        i += std::str::from_utf8(&bytes[i..])
            .ok()?
            .chars()
            .next()?
            .len_utf8();
    }
    (bytes.get(i) == Some(&b'\'')).then_some(i + 1)
}
