//! Conservative shell flattening for the Claude Code policy. It splits a Bash
//! line into simple commands, redirections and nested substitutions. It is not a
//! shell: anything it cannot follow is marked computed or opaque so the policy
//! can refuse it for agents.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Word {
    pub(crate) text: String,
    /// Part of the word is only known at run time (`$x`, `$(..)`, braces).
    pub(crate) computed: bool,
    /// An unquoted glob character the shell may expand.
    pub(crate) glob: bool,
    /// Prefix before an unquoted `=` when the word is a valid assignment.
    assignment: Option<usize>,
}

impl Word {
    pub(crate) fn literal(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            ..Self::default()
        }
    }
    /// Another text with the same run-time markers (part of a split word).
    pub(crate) fn with_text(&self, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            computed: self.computed,
            glob: self.glob,
            assignment: None,
        }
    }
    /// `NAME` and the value of an assignment-shaped word (`NAME=value`).
    pub(crate) fn split_assignment(&self) -> Option<(&str, Word)> {
        let split = self.assignment?;
        Some((&self.text[..split], self.with_text(&self.text[split + 1..])))
    }
    pub(crate) fn plain(&self) -> bool {
        !self.computed && !self.glob
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Join {
    End,
    Sequence,
    And,
    Or,
    Pipe,
    Background,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Redirect {
    pub(crate) target: Word,
    pub(crate) write: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Simple {
    pub(crate) assignments: Vec<(String, Word)>,
    pub(crate) words: Vec<Word>,
    pub(crate) redirects: Vec<Redirect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Item {
    Command(Simple, Join),
    Open,
    Close,
}

#[derive(Debug, Default)]
pub(crate) struct Parsed {
    pub(crate) items: Vec<Item>,
    /// Command and process substitutions, judged as lines of their own.
    pub(crate) substitutions: Vec<String>,
    /// Why part of the line cannot be judged at all.
    pub(crate) opaque: Option<&'static str>,
}

mod lexer;
use lexer::{Lexer, Token};

pub(crate) fn parse(source: &str) -> Result<Parsed, String> {
    let mut parsed = Parsed::default();
    let tokens = Lexer::new(source, &mut parsed).tokens()?;
    let mut current = Simple::default();
    let mut tokens = tokens.into_iter().peekable();
    let finish = |current: &mut Simple, join, items: &mut Vec<Item>| {
        if *current != Simple::default() {
            items.push(Item::Command(std::mem::take(current), join));
        }
    };
    let mut items = Vec::new();
    while let Some(token) = tokens.next() {
        match token {
            Token::Word(word) => match word.assignment {
                Some(split) if current.words.is_empty() => {
                    let name = word.text[..split].to_owned();
                    let value = Word {
                        text: word.text[split + 1..].to_owned(),
                        assignment: None,
                        ..word
                    };
                    current.assignments.push((name, value));
                }
                _ => current.words.push(word),
            },
            Token::Redirect { op } => {
                let Some(Token::Word(target)) = tokens.next() else {
                    return Err(format!("redirection `{op}` has no target"));
                };
                let duplicate = op.ends_with('&')
                    && (target.text == "-" || target.text.chars().all(|c| c.is_ascii_digit()));
                let write = !duplicate
                    && matches!(op.as_str(), ">" | ">>" | ">|" | "&>" | "&>>" | "<>" | ">&");
                current.redirects.push(Redirect { target, write });
            }
            Token::Heredoc => {}
            Token::Join(join) => finish(&mut current, join, &mut items),
            Token::Open => {
                finish(&mut current, Join::Sequence, &mut items);
                items.push(Item::Open);
            }
            Token::Close => {
                finish(&mut current, Join::Sequence, &mut items);
                items.push(Item::Close);
            }
        }
    }
    finish(&mut current, Join::End, &mut items);
    parsed.items = items;
    Ok(parsed)
}

pub(crate) fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
