//! Expansion boundaries and bodies use the same quote/escape handlers as words.
use super::{ARITHMETIC, EXPANSION, Lexer, Parsed, Word, plain_parameter};

impl Lexer<'_> {
    pub(super) fn dollar(&mut self, word: &mut Word, in_double: bool) -> Result<(), String> {
        self.at += 1;
        match self.peek(0) {
            Some('(') if self.peek(1) == Some('(') => {
                self.at += 2;
                // Bash expands arithmetic like double-quoted text even when
                // the surrounding word is unquoted; apostrophes are data.
                self.expansion_body(')', true)?;
                if self.peek(0) != Some(')') {
                    return Err("unbalanced arithmetic expansion".into());
                }
                self.at += 1;
                self.parsed.opaque = Some(ARITHMETIC);
                word.computed = true;
            }
            Some('[') => {
                self.parsed.opaque = Some(ARITHMETIC);
                word.text.push('$');
            }
            Some('(') => {
                self.at += 1;
                let inner = self.command_body()?;
                self.parsed.substitutions.push(inner);
                word.computed = true;
            }
            Some('{') => {
                self.at += 1;
                let inner = self.expansion_body('}', in_double)?;
                if !plain_parameter(&inner) {
                    self.parsed.opaque = Some(EXPANSION);
                }
                word.computed = true;
            }
            Some('\'') if !in_double => {
                self.single_quoted(word, true)?;
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

    /// Lex a command substitution with fresh command quote/comment context.
    /// Keep its original source so policy judges its commands in their own cwd.
    pub(super) fn command_body(&mut self) -> Result<String, String> {
        let source: String = self.chars[self.at..].iter().collect();
        let mut parsed = Parsed::default();
        let mut nested = Lexer::new(&source, &mut parsed);
        nested.tokens_until(true)?;
        let consumed = nested.at;
        let inner = self.chars[self.at..self.at + consumed - 1].iter().collect();
        self.at += consumed;
        self.parsed.continued_line |= parsed.continued_line;
        Ok(inner)
    }

    /// An expansion body is text, not command syntax: whitespace, `#` and
    /// operators do not start tokens/comments. Quotes and substitutions still
    /// use the word lexer. In a double-quoted expansion apostrophes are data.
    fn expansion_body(&mut self, close: char, in_double: bool) -> Result<String, String> {
        let source: String = self.chars[self.at..].iter().collect();
        let mut parsed = Parsed::default();
        let mut nested = Lexer::new(&source, &mut parsed);
        let mut word = Word::default();
        let mut depth = 0usize;
        let open = if close == ')' { '(' } else { '{' };
        loop {
            match nested.peek(0) {
                None => return Err(format!("unbalanced `{open}`")),
                Some(c) if c == close && depth == 0 => break,
                _ if nested.fragment(&mut word, in_double)? => continue,
                Some(c) => {
                    // Bare braces are text in `${...}`; only `${` nests,
                    // through dollar(). Arithmetic balances bare parentheses.
                    if c == open && close == ')' {
                        depth += 1;
                    } else if c == close {
                        depth -= 1;
                    }
                    nested.at += 1;
                }
            }
        }
        let inner = self.chars[self.at..self.at + nested.at].iter().collect();
        self.at += nested.at + 1;
        self.parsed.continued_line |= parsed.continued_line;
        self.parsed.substitutions.append(&mut parsed.substitutions);
        if parsed.opaque.is_some() {
            self.parsed.opaque = parsed.opaque;
        }
        Ok(inner)
    }

    pub(super) fn backtick(&mut self, word: &mut Word) -> Result<(), String> {
        self.at += 1;
        let mut inner = String::new();
        loop {
            match (self.peek(0), self.peek(1)) {
                (None, _) => return Err("unterminated backtick".into()),
                (Some('`'), _) => break,
                (Some('\\'), Some('\n')) => {
                    self.parsed.continued_line = true;
                    self.at += 2;
                    continue;
                }
                (Some('\\'), Some(next)) if "$`\\".contains(next) => {
                    inner.push(next);
                    self.at += 2;
                    continue;
                }
                (Some(c), _) => inner.push(c),
            }
            self.at += 1;
        }
        self.at += 1;
        self.parsed.substitutions.push(inner);
        word.computed = true;
        Ok(())
    }
}
