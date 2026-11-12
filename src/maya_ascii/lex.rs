use super::{AsciiError, MAX_BYTES, Work, malformed, resource, unsupported};
use crate::native_scene::admission::Budget;
use std::{fmt, ops::Range, sync::Arc};

const MAX_STATEMENTS: usize = 1_000_000;
const MAX_TOKENS: usize = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Word,
    Quoted,
    LeftParen,
    RightParen,
    Plus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) span: Range<usize>,
}

impl Token {
    pub(crate) fn text<'a>(
        &self,
        source: &'a [u8],
        work: &mut Work,
    ) -> Result<&'a str, AsciiError> {
        let bytes = source
            .get(self.span.clone())
            .ok_or_else(|| malformed(self.span.start))?;
        work.charge(bytes.len())?;
        std::str::from_utf8(bytes).map_err(|_| malformed(self.span.start))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Statement {
    pub(crate) retained: Range<usize>,
    pub(crate) command: Range<usize>,
    pub(crate) tokens: Range<usize>,
}

pub(crate) struct Records {
    pub(crate) source: Arc<[u8]>,
    pub(crate) statements: Vec<Statement>,
    pub(crate) tokens: Vec<Token>,
    pub(crate) trailing: Range<usize>,
    pub(crate) budget: Budget,
    pub(crate) work: Work,
}

impl fmt::Debug for Records {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsciiRecords")
            .field("source_bytes", &self.source.len())
            .field("statements", &self.statements.len())
            .field("tokens", &self.tokens.len())
            .finish()
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Counts {
    statements: usize,
    tokens: usize,
}

enum Event {
    Token(Token),
    Statement(Statement),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Concat {
    NeedString,
    AfterString,
}

struct Scan<'a> {
    source: &'a [u8],
    cursor: usize,
    work: &'a mut Work,
    counts: Counts,
    retained_start: usize,
    command_start: Option<usize>,
    first_token: usize,
    previous_end: Option<usize>,
    concat: Option<Concat>,
}

impl Scan<'_> {
    fn advance(&mut self, bytes: usize) -> Result<(), AsciiError> {
        self.work.charge(bytes)?;
        self.cursor = self
            .cursor
            .checked_add(bytes)
            .filter(|end| *end <= self.source.len())
            .ok_or_else(|| malformed(self.cursor))?;
        Ok(())
    }

    fn trivia(&mut self) -> Result<bool, AsciiError> {
        let Some(&byte) = self.source.get(self.cursor) else {
            return Ok(false);
        };
        if byte.is_ascii_whitespace() {
            self.advance(1)?;
            return Ok(true);
        }
        if self.source.get(self.cursor..self.cursor.saturating_add(2)) == Some(b"//") {
            self.advance(2)?;
            while self.source.get(self.cursor).is_some_and(|b| *b != b'\n') {
                self.advance(1)?;
            }
            return Ok(true);
        }
        if self.source.get(self.cursor..self.cursor.saturating_add(2)) == Some(b"/*") {
            let start = self.cursor;
            self.advance(2)?;
            loop {
                if self.source.get(self.cursor..self.cursor.saturating_add(2)) == Some(b"*/") {
                    self.advance(2)?;
                    break;
                }
                if self.cursor == self.source.len() {
                    return Err(malformed(start));
                }
                self.advance(1)?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn quoted(&mut self) -> Result<(), AsciiError> {
        let start = self.cursor;
        self.advance(1)?;
        while let Some(&byte) = self.source.get(self.cursor) {
            match byte {
                b'"' => {
                    self.advance(1)?;
                    return Ok(());
                }
                b'\\' => {
                    let escaped = self
                        .source
                        .get(self.cursor + 1)
                        .ok_or_else(|| malformed(start))?;
                    if !matches!(*escaped, b'"' | b'\\' | b'n' | b't') {
                        return Err(unsupported(self.cursor));
                    }
                    self.advance(2)?;
                }
                0..=8 | 11..=31 | 127 => return Err(unsupported(self.cursor)),
                _ => self.advance(1)?,
            }
        }
        Err(malformed(start))
    }

    fn word(&mut self) -> Result<(), AsciiError> {
        if self.source.get(self.cursor) == Some(&b'+') {
            self.advance(1)?;
        }
        let start = self.cursor;
        while let Some(&byte) = self.source.get(self.cursor) {
            if byte.is_ascii_alphanumeric()
                || matches!(byte, b'_' | b'.' | b':' | b'[' | b']' | b'-')
                || (byte == b'+'
                    && self.cursor > start
                    && matches!(self.source.get(self.cursor - 1), Some(b'e' | b'E')))
            {
                self.advance(1)?;
            } else {
                break;
            }
        }
        if self.cursor == start {
            return Err(unsupported(start));
        }
        Ok(())
    }

    fn token(&mut self) -> Result<Token, AsciiError> {
        let start = self.cursor;
        let byte = *self.source.get(start).ok_or_else(|| malformed(start))?;
        let kind = match byte {
            b'"' => {
                if self.concat == Some(Concat::AfterString) {
                    return Err(malformed(start));
                }
                if self.previous_end == Some(start) && self.concat.is_none() {
                    return Err(malformed(start));
                }
                self.quoted()?;
                if self.concat.is_some() {
                    self.concat = Some(Concat::AfterString);
                }
                TokenKind::Quoted
            }
            b'(' => {
                if self.concat.is_some() {
                    return Err(unsupported(start));
                }
                self.concat = Some(Concat::NeedString);
                self.advance(1)?;
                TokenKind::LeftParen
            }
            b')' => {
                if self.concat != Some(Concat::AfterString) {
                    return Err(malformed(start));
                }
                self.concat = None;
                self.advance(1)?;
                TokenKind::RightParen
            }
            b'+' if self.concat == Some(Concat::AfterString) => {
                self.concat = Some(Concat::NeedString);
                self.advance(1)?;
                TokenKind::Plus
            }
            _ => {
                if self.concat.is_some() {
                    return Err(unsupported(start));
                }
                if self.previous_end == Some(start) {
                    return Err(unsupported(start));
                }
                if byte == b'+'
                    && !self
                        .source
                        .get(start + 1)
                        .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
                {
                    return Err(unsupported(start));
                }
                self.word()?;
                TokenKind::Word
            }
        };
        if self.command_start.is_none() && kind != TokenKind::Word {
            return Err(malformed(start));
        }
        self.command_start.get_or_insert(start);
        self.previous_end = Some(self.cursor);
        self.counts.tokens = self
            .counts
            .tokens
            .checked_add(1)
            .filter(|n| *n <= MAX_TOKENS)
            .ok_or_else(resource)?;
        Ok(Token {
            kind,
            span: start..self.cursor,
        })
    }

    fn run(
        mut self,
        mut emit: impl FnMut(Event) -> Result<(), AsciiError>,
    ) -> Result<(Counts, Range<usize>), AsciiError> {
        while self.cursor < self.source.len() {
            if self.trivia()? {
                continue;
            }
            if self.source.get(self.cursor) == Some(&b';') {
                let start = self.command_start.ok_or_else(|| malformed(self.cursor))?;
                if self.concat.is_some() {
                    return Err(malformed(self.cursor));
                }
                self.advance(1)?;
                self.counts.statements = self
                    .counts
                    .statements
                    .checked_add(1)
                    .filter(|n| *n <= MAX_STATEMENTS)
                    .ok_or_else(resource)?;
                emit(Event::Statement(Statement {
                    retained: self.retained_start..self.cursor,
                    command: start..self.cursor,
                    tokens: self.first_token..self.counts.tokens,
                }))?;
                self.retained_start = self.cursor;
                self.command_start = None;
                self.first_token = self.counts.tokens;
                self.previous_end = None;
            } else {
                emit(Event::Token(self.token()?))?;
            }
        }
        if self.command_start.is_some() || self.concat.is_some() {
            return Err(malformed(self.cursor));
        }
        Ok((self.counts, self.retained_start..self.source.len()))
    }
}

fn scanner<'a>(source: &'a [u8], work: &'a mut Work) -> Scan<'a> {
    Scan {
        source,
        cursor: 0,
        work,
        counts: Counts::default(),
        retained_start: 0,
        command_start: None,
        first_token: 0,
        previous_end: None,
        concat: None,
    }
}

fn header(source: &[u8], work: &mut Work) -> Result<(), AsciiError> {
    let end = source
        .iter()
        .position(|b| *b == b'\n')
        .unwrap_or(source.len());
    work.charge(end)?;
    let line = source.get(..end).ok_or_else(|| malformed(0))?;
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let version = line
        .strip_prefix(b"//Maya ASCII ")
        .and_then(|s| s.strip_suffix(b" scene"))
        .ok_or_else(|| malformed(0))?;
    work.charge(version.len())?;
    if !version
        .split(|b| *b == b'.')
        .all(|part| !part.is_empty() && part.iter().all(u8::is_ascii_digit))
    {
        return Err(malformed(0));
    }
    Ok(())
}

pub(crate) fn inspect(source: Arc<[u8]>) -> Result<Records, AsciiError> {
    if source.len() > MAX_BYTES {
        return Err(resource());
    }
    let mut budget = Budget::new(source.len()).map_err(|_| resource())?;
    let mut work = Work::new();
    work.charge(source.len())?;
    std::str::from_utf8(&source).map_err(|e| malformed(e.valid_up_to()))?;
    header(&source, &mut work)?;
    let (counts, _) = scanner(&source, &mut work).run(|_| Ok(()))?;
    let mut statements = budget
        .vector::<Statement>(counts.statements)
        .map_err(|_| resource())?;
    let mut tokens = budget
        .vector::<Token>(counts.tokens)
        .map_err(|_| resource())?;
    let (_, trailing) = scanner(&source, &mut work).run(|event| {
        match event {
            Event::Token(token) => {
                if tokens.len() == tokens.capacity() {
                    return Err(resource());
                }
                tokens.push(token);
            }
            Event::Statement(statement) => {
                if statements.len() == statements.capacity() {
                    return Err(resource());
                }
                statements.push(statement);
            }
        }
        Ok(())
    })?;
    Ok(Records {
        source,
        statements,
        tokens,
        trailing,
        budget,
        work,
    })
}

#[cfg(test)]
mod tests;
