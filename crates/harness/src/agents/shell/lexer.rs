//! The character-level lexer behind [`super::parse`]: quotes, escapes,
//! substitutions, redirections and here-documents.
use super::{Join, Parsed, Word, valid_name};

pub(super) enum Token {
    Word(Word),
    /// `<<DELIMITER`, whose body was consumed at the next newline.
    Heredoc,
    Redirect {
        op: String,
    },
    Join(Join),
    Open,
    Close,
}

struct Heredoc {
    delimiter: String,
    strip_tabs: bool,
    quoted: bool,
}

pub(super) struct Lexer<'a> {
    chars: Vec<char>,
    at: usize,
    parsed: &'a mut Parsed,
    heredocs: Vec<Heredoc>,
}

const OPERATOR: &str = ";&|()<>\n";
const ARITHMETIC: &str = "arithmetic `(( ))`, `$(( ))` or `$[ ]` can assign shell variables";
const EXPANSION: &str = "agents expand parameters only as `$NAME`, `${NAME}`, `$1`…`$9`, `$?`, `$$`, `$#`, `$@` or `$*` (other forms can assign, run commands or hide them)";
const HEREDOC: &str =
    "agents do not use here-documents or here-strings; write files with the editor tools";
const CONTINUATION: &str = "agents do not continue lines with a backslash-newline";

/// The parameter forms agents may expand: a name, `1`…`9` or `?$#@*`.
fn plain_parameter(name: &str) -> bool {
    valid_name(name) || (name.len() == 1 && "123456789?$#@*".contains(name))
}

impl<'a> Lexer<'a> {
    pub(super) fn new(source: &str, parsed: &'a mut Parsed) -> Self {
        Self {
            chars: source.chars().collect(),
            at: 0,
            parsed,
            heredocs: Vec::new(),
        }
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.at + offset).copied()
    }

    pub(super) fn tokens(mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        if self.chars.windows(2).any(|pair| pair == ['\\', '\n']) {
            self.parsed.opaque = Some(CONTINUATION);
        }
        while let Some(c) = self.peek(0) {
            match c {
                ' ' | '\t' | '\r' => self.at += 1,
                '\\' if self.peek(1) == Some('\n') => self.at += 2,
                '#' => {
                    while self.peek(0).is_some_and(|c| c != '\n') {
                        self.at += 1;
                    }
                }
                '\n' => {
                    self.at += 1;
                    self.heredoc_bodies()?;
                    tokens.push(Token::Join(Join::Sequence));
                }
                ';' => {
                    self.at += if self.peek(1) == Some(';') { 2 } else { 1 };
                    tokens.push(Token::Join(Join::Sequence));
                }
                '&' => tokens.push(self.ampersand()),
                '|' => {
                    let join = match self.peek(1) {
                        Some('|') => Join::Or,
                        _ => Join::Pipe,
                    };
                    self.at += if matches!(self.peek(1), Some('|' | '&')) {
                        2
                    } else {
                        1
                    };
                    tokens.push(Token::Join(join));
                }
                '(' => {
                    if self.peek(1) == Some('(') {
                        self.parsed.opaque = Some(ARITHMETIC);
                    }
                    self.at += 1;
                    tokens.push(Token::Open);
                }
                ')' => {
                    self.at += 1;
                    tokens.push(Token::Close);
                }
                '<' | '>' if self.peek(1) == Some('(') => {
                    self.at += 2;
                    let inner = self.balanced(')')?;
                    self.parsed.substitutions.push(inner);
                    tokens.push(Token::Word(Word {
                        text: "/dev/fd/process-substitution".into(),
                        computed: true,
                        ..Word::default()
                    }));
                }
                '<' | '>' => tokens.push(self.redirect()?),
                c if c.is_ascii_digit() && self.fd_redirect() => {
                    while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                        self.at += 1;
                    }
                    tokens.push(self.redirect()?);
                }
                _ => {
                    let word = self.word()?;
                    let text = &word.text;
                    if text.starts_with('{')
                        && text.ends_with('}')
                        && matches!(self.peek(0), Some('<' | '>'))
                    {
                        self.parsed.opaque =
                            Some("a `{name}` redirection assigns a shell variable");
                    }
                    tokens.push(Token::Word(word));
                }
            }
        }
        if !self.heredocs.is_empty() {
            return Err("unterminated here-document".into());
        }
        Ok(tokens)
    }

    fn ampersand(&mut self) -> Token {
        match self.peek(1) {
            Some('&') => {
                self.at += 2;
                Token::Join(Join::And)
            }
            Some('>') => {
                let append = self.peek(2) == Some('>');
                self.at += if append { 3 } else { 2 };
                Token::Redirect {
                    op: if append { "&>>" } else { "&>" }.into(),
                }
            }
            _ => {
                self.at += 1;
                Token::Join(Join::Background)
            }
        }
    }

    fn fd_redirect(&self) -> bool {
        let mut offset = 0;
        while self.peek(offset).is_some_and(|c| c.is_ascii_digit()) {
            offset += 1;
        }
        matches!(self.peek(offset), Some('<' | '>'))
    }

    fn redirect(&mut self) -> Result<Token, String> {
        let rest: String = self.chars[self.at..].iter().take(3).collect();
        let op = ["<<<", "<<-", ">>", ">|", ">&", "<<", "<&", "<>", "<", ">"]
            .into_iter()
            .find(|op| rest.starts_with(op))
            .ok_or("invalid redirection")?;
        self.at += op.chars().count();
        if op.starts_with("<<") {
            self.parsed.opaque = Some(HEREDOC);
        }
        if op == "<<" || op == "<<-" {
            while matches!(self.peek(0), Some(' ' | '\t')) {
                self.at += 1;
            }
            let start = self.at;
            let word = self.word()?;
            let raw: String = self.chars[start..self.at].iter().collect();
            self.heredocs.push(Heredoc {
                delimiter: word.text.clone(),
                strip_tabs: op == "<<-",
                quoted: raw.contains(['\'', '"', '\\']),
            });
            return Ok(Token::Heredoc);
        }
        Ok(Token::Redirect { op: op.into() })
    }

    fn heredoc_bodies(&mut self) -> Result<(), String> {
        for heredoc in std::mem::take(&mut self.heredocs) {
            loop {
                if self.at >= self.chars.len() {
                    return Err(format!("here-document `{}` never ends", heredoc.delimiter));
                }
                let start = self.at;
                while self.peek(0).is_some_and(|c| c != '\n') {
                    self.at += 1;
                }
                let line: String = self.chars[start..self.at].iter().collect();
                self.at += 1;
                let line = if heredoc.strip_tabs {
                    line.trim_start_matches('\t')
                } else {
                    &line
                };
                if line == heredoc.delimiter {
                    break;
                }
                if !heredoc.quoted && (line.contains("$(") || line.contains('`')) {
                    self.parsed.opaque = Some("a here-document expands a command substitution");
                }
            }
        }
        Ok(())
    }

    fn word(&mut self) -> Result<Word, String> {
        let mut word = Word::default();
        let mut quoted = false;
        let mut brace = false;
        while let Some(c) = self.peek(0) {
            if c.is_whitespace() || OPERATOR.contains(c) {
                break;
            }
            match c {
                '\\' => {
                    if let Some(next) = self.peek(1) {
                        if next != '\n' {
                            word.text.push(next);
                        }
                        quoted = true;
                    }
                    self.at += 2;
                }
                '\'' => {
                    self.at += 1;
                    quoted = true;
                    loop {
                        match self.peek(0) {
                            None => return Err("unterminated single quote".into()),
                            Some('\'') => break,
                            Some(c) => word.text.push(c),
                        }
                        self.at += 1;
                    }
                    self.at += 1;
                }
                '"' => {
                    self.at += 1;
                    quoted = true;
                    self.double_quoted(&mut word)?;
                }
                '$' => self.dollar(&mut word)?,
                '`' => self.backtick(&mut word)?,
                '=' if word.assignment.is_none() && !quoted && !word.computed => {
                    if valid_name(&word.text) {
                        word.assignment = Some(word.text.len());
                    }
                    word.text.push('=');
                    self.at += 1;
                }
                _ => {
                    if matches!(c, '*' | '?' | '[') {
                        word.glob = true;
                    }
                    brace |= c == '{';
                    word.text.push(c);
                    self.at += 1;
                }
            }
        }
        if brace && (word.text.contains(',') || word.text.contains("..")) {
            word.computed = true;
        }
        Ok(word)
    }

    fn double_quoted(&mut self, word: &mut Word) -> Result<(), String> {
        loop {
            match self.peek(0) {
                None => return Err("unterminated double quote".into()),
                Some('"') => {
                    self.at += 1;
                    return Ok(());
                }
                Some('\\') => {
                    match self.peek(1) {
                        Some(c @ ('$' | '`' | '"' | '\\')) => word.text.push(c),
                        Some('\n') => {}
                        Some(c) => {
                            word.text.push('\\');
                            word.text.push(c);
                        }
                        None => return Err("unterminated double quote".into()),
                    }
                    self.at += 2;
                }
                Some('$') => self.dollar(word)?,
                Some('`') => self.backtick(word)?,
                Some(c) => {
                    word.text.push(c);
                    self.at += 1;
                }
            }
        }
    }

    fn dollar(&mut self, word: &mut Word) -> Result<(), String> {
        self.at += 1;
        match self.peek(0) {
            Some('(') if self.peek(1) == Some('(') => {
                self.at += 2;
                self.balanced(')')?;
                if self.peek(0) == Some(')') {
                    self.at += 1;
                }
                self.parsed.opaque = Some(ARITHMETIC);
                word.computed = true;
            }
            Some('[') => {
                self.parsed.opaque = Some(ARITHMETIC);
                word.text.push('$');
            }
            Some('(') => {
                self.at += 1;
                let inner = self.balanced(')')?;
                self.parsed.substitutions.push(inner);
                word.computed = true;
            }
            Some('{') => {
                self.at += 1;
                if !plain_parameter(&self.balanced('}')?) {
                    self.parsed.opaque = Some(EXPANSION);
                }
                word.computed = true;
            }
            Some('\'') => {
                self.at += 1;
                while let Some(c) = self.peek(0) {
                    self.at += 1;
                    match c {
                        '\\' => self.at += 1,
                        '\'' => break,
                        c => word.text.push(c),
                    }
                }
                word.computed = true;
            }
            Some(c) if c.is_ascii_alphanumeric() || "_@*#?$!-".contains(c) => {
                let special = !(c.is_ascii_alphabetic() || c == '_');
                if special && !plain_parameter(&c.to_string()) {
                    self.parsed.opaque = Some(EXPANSION);
                }
                self.at += 1;
                while !special
                    && self
                        .peek(0)
                        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    self.at += 1;
                }
                word.computed = true;
            }
            _ => word.text.push('$'),
        }
        Ok(())
    }

    fn backtick(&mut self, word: &mut Word) -> Result<(), String> {
        self.at += 1;
        let mut inner = String::new();
        loop {
            match self.peek(0) {
                None => return Err("unterminated backtick".into()),
                Some('`') => break,
                Some('\\') => {
                    if let Some(next) = self.peek(1) {
                        inner.push(next);
                    }
                    self.at += 2;
                    continue;
                }
                Some(c) => inner.push(c),
            }
            self.at += 1;
        }
        self.at += 1;
        self.parsed.substitutions.push(inner);
        word.computed = true;
        Ok(())
    }

    /// Text up to the matching `close`, honouring quotes and nesting.
    fn balanced(&mut self, close: char) -> Result<String, String> {
        let open = if close == ')' { '(' } else { '{' };
        let start = self.at;
        let mut depth = 0usize;
        while let Some(c) = self.peek(0) {
            match c {
                '\\' => self.at += 1,
                '\'' => {
                    self.at += 1;
                    while self.peek(0).is_some_and(|c| c != '\'') {
                        self.at += 1;
                    }
                }
                '"' => {
                    self.at += 1;
                    while self.peek(0).is_some_and(|c| c != '"') {
                        if self.peek(0) == Some('\\') {
                            self.at += 1;
                        }
                        self.at += 1;
                    }
                }
                c if c == open => depth += 1,
                c if c == close && depth == 0 => {
                    let inner = self.chars[start..self.at].iter().collect();
                    self.at += 1;
                    return Ok(inner);
                }
                c if c == close => depth -= 1,
                _ => {}
            }
            self.at += 1;
        }
        Err(format!("unbalanced `{open}`"))
    }
}
