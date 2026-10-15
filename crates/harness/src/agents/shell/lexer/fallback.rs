//! Span recovery when the more precise nested lexer cannot follow a body.
use super::Lexer;

impl Lexer<'_> {
    /// The integration lexer's balanced boundary rule. Do not propagate a
    /// nested syntax error if this rule can still locate the span: main-session
    /// parse errors allow the entire line, including later forbidden commands.
    /// Callers queue the returned source as a separate line to be judged.
    pub(super) fn fallback_span(&mut self, close: char) -> Result<String, String> {
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

    /// Backticks used to remove every backslash before judging the body.
    /// Keep that verdict floor as well as the Bash-decoded line: parsing may
    /// succeed here but fail later inside a command wrapper's script argument.
    pub(super) fn fallback_backtick(&self, start: usize) -> String {
        let mut chars = self.chars[start..self.at].iter();
        let mut inner = String::new();
        while let Some(c) = chars.next() {
            if *c == '\\' {
                if let Some(next) = chars.next() {
                    inner.push(*next);
                }
            } else {
                inner.push(*c);
            }
        }
        inner
    }
}
