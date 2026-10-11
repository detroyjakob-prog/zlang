//! Lexer for the Z Programming Language.
//! Tokenizes Z source code into a stream of tokens.

use crate::error::{ZError, ZResult};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Int(i64),
    Float(f64),
    String(String),
    FString(String),
    Ident(String),

    // Keywords
    Abl,
    And,
    As,
    Assert,
    Async,
    Await,
    Break,
    Call,
    Callclass,
    Callerror,
    Case,
    Const,
    Continue,
    Del,
    Dir,
    Elif,
    Else,
    Enum,
    Except,
    False,
    Finally,
    Fn,
    For,
    From,
    Global,
    If,
    Import,
    In,
    Is,
    Match,
    None,
    Not,
    Or,
    Outer,
    Pass,
    Return,
    Self_,
    Static,
    Super,
    True,
    Try,
    While,
    With,
    Yield,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    FloorDiv,
    Percent,
    Pow,
    Amp,
    Pipe,
    Caret,
    Tilde,
    ShiftLeft,
    ShiftRight,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Assign,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    FloorDivEq,
    PercentEq,
    PowEq,

    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    Arrow,

    // Structural
    Newline,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    tokens: Vec<Token>,
    source: String,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            tokens: Vec::new(),
            source: source.to_string(),
        }
    }

    pub fn tokenize(&mut self) -> ZResult<Vec<Token>> {
        while self.pos < self.chars.len() {
            let c = self.peek();
            match c {
                ' ' | '\t' | '\r' => {
                    self.advance();
                }
                '\n' => {
                    self.push(TokenKind::Newline);
                    self.line += 1;
                    self.col = 1;
                    self.advance();
                }
                '/' if self.lookahead(1) == '*' => {
                    self.block_comment()?;
                }
                '/' if self.lookahead(1) == '/' && self.comment_starts_here() => {
                    self.line_comment();
                }
                '"' | '\'' => {
                    self.string(c)?;
                }
                'r' if self.lookahead(1) == '"' || self.lookahead(1) == '\'' => {
                    self.raw_string()?;
                }
                'f' if self.lookahead(1) == '"' || self.lookahead(1) == '\'' => {
                    self.fstring()?;
                }
                c if c.is_ascii_digit() => {
                    self.number()?;
                }
                c if c.is_alphabetic() || c == '_' => {
                    self.identifier();
                }
                '(' => {
                    self.push(TokenKind::LParen);
                    self.advance();
                }
                ')' => {
                    self.push(TokenKind::RParen);
                    self.advance();
                }
                '{' => {
                    self.push(TokenKind::LBrace);
                    self.advance();
                }
                '}' => {
                    self.push(TokenKind::RBrace);
                    self.advance();
                }
                '[' => {
                    self.push(TokenKind::LBracket);
                    self.advance();
                }
                ']' => {
                    self.push(TokenKind::RBracket);
                    self.advance();
                }
                ',' => {
                    self.push(TokenKind::Comma);
                    self.advance();
                }
                ':' => {
                    self.push(TokenKind::Colon);
                    self.advance();
                }
                '.' => {
                    if self.lookahead(1).is_ascii_digit() {
                        self.number()?;
                    } else {
                        self.push(TokenKind::Dot);
                        self.advance();
                    }
                }
                '+' => {
                    if self.lookahead(1) == '=' {
                        self.push(TokenKind::PlusEq);
                        self.advance();
                    } else {
                        self.push(TokenKind::Plus);
                    }
                    self.advance();
                }
                '-' => {
                    if self.lookahead(1) == '=' {
                        self.push(TokenKind::MinusEq);
                        self.advance();
                    } else if self.lookahead(1) == '>' {
                        self.push(TokenKind::Arrow);
                        self.advance();
                    } else {
                        self.push(TokenKind::Minus);
                    }
                    self.advance();
                }
                '*' => {
                    let next = self.lookahead(1);
                    if next == '*' {
                        if self.lookahead(2) == '=' {
                            self.push(TokenKind::PowEq);
                            self.advance();
                        } else {
                            self.push(TokenKind::Pow);
                        }
                        self.advance();
                    } else if next == '=' {
                        self.push(TokenKind::StarEq);
                        self.advance();
                    } else {
                        self.push(TokenKind::Star);
                    }
                    self.advance();
                }
                '/' => {
                    let next = self.lookahead(1);
                    if next == '/' {
                        if self.lookahead(2) == '=' {
                            self.push(TokenKind::FloorDivEq);
                            self.advance();
                        } else {
                            self.push(TokenKind::FloorDiv);
                        }
                        self.advance();
                    } else if next == '=' {
                        self.push(TokenKind::SlashEq);
                        self.advance();
                    } else {
                        self.push(TokenKind::Slash);
                    }
                    self.advance();
                }
                '%' => {
                    if self.lookahead(1) == '=' {
                        self.push(TokenKind::PercentEq);
                        self.advance();
                    } else {
                        self.push(TokenKind::Percent);
                    }
                    self.advance();
                }
                '&' => {
                    self.push(TokenKind::Amp);
                    self.advance();
                }
                '|' => {
                    self.push(TokenKind::Pipe);
                    self.advance();
                }
                '^' => {
                    self.push(TokenKind::Caret);
                    self.advance();
                }
                '~' => {
                    self.push(TokenKind::Tilde);
                    self.advance();
                }
                '<' => {
                    let next = self.lookahead(1);
                    if next == '<' {
                        self.push(TokenKind::ShiftLeft);
                        self.advance();
                    } else if next == '=' {
                        self.push(TokenKind::LtEq);
                        self.advance();
                    } else {
                        self.push(TokenKind::Lt);
                    }
                    self.advance();
                }
                '>' => {
                    let next = self.lookahead(1);
                    if next == '>' {
                        self.push(TokenKind::ShiftRight);
                        self.advance();
                    } else if next == '=' {
                        self.push(TokenKind::GtEq);
                        self.advance();
                    } else {
                        self.push(TokenKind::Gt);
                    }
                    self.advance();
                }
                '=' => {
                    let next = self.lookahead(1);
                    if next == '=' {
                        self.push(TokenKind::Eq);
                        self.advance();
                    } else if next == '>' {
                        // `=>` introduces a lambda body.
                        self.push(TokenKind::Arrow);
                        self.advance();
                    } else {
                        self.push(TokenKind::Assign);
                    }
                    self.advance();
                }
                '!' => {
                    if self.lookahead(1) == '=' {
                        self.push(TokenKind::NotEq);
                        self.advance();
                    } else {
                        return Err(self.error("'!' not recognized — use 'not' for logical negation"));
                    }
                    self.advance();
                }
                _ => {
                    return Err(self.error(&format!("unexpected character '{}'", c)));
                }
            }
        }
        self.push(TokenKind::Eof);
        Ok(self.tokens.clone())
    }

    fn peek(&self) -> char {
        self.peek_at(0)
    }

    fn peek_at(&self, offset: usize) -> char {
        self.chars.get(self.pos + offset).copied().unwrap_or('\0')
    }

    fn lookahead(&self, offset: usize) -> char {
        self.peek_at(offset)
    }

    /// Decide whether a `//` at the current position starts a line comment.
    ///
    /// `//` is both a line comment and floor division in Z, so it is a comment
    /// only when the previous meaningful token cannot end an expression. That
    /// makes `10 // 3` floor division while a comment on its own line — or
    /// after a `}` or `,` — stays a comment.
    fn comment_starts_here(&self) -> bool {
        let left = match self.tokens.last().map(|t| &t.kind) {
            None => return true,
            Some(kind) => kind,
        };
        // Division needs a value on the left.
        let left_ends_expression = matches!(
            left,
            TokenKind::Int(_)
                | TokenKind::Float(_)
                | TokenKind::Ident(_)
                | TokenKind::String(_)
                | TokenKind::FString(_)
                | TokenKind::RParen
                | TokenKind::RBracket
                | TokenKind::RBrace
                | TokenKind::True
                | TokenKind::False
                | TokenKind::None
                | TokenKind::Self_
        );
        if !left_ends_expression {
            return true;
        }
        // ... and a value on the right. A word after `//` is prose, not code.
        // `peek_at(1)` is the second `/`, so look two ahead.
        let next = self.peek_at(2);
        matches!(
            next,
            ' ' | '\t' | '\n' | '\r' | '\0'
        ) || next.is_ascii_digit()
            || next == '_'
            || next == '('
    }

    fn push(&mut self, kind: TokenKind) {
        self.tokens.push(Token {
            kind,
            line: self.line,
            col: self.col,
        });
    }

    fn advance(&mut self) {
        if self.pos < self.chars.len() {
            self.col += 1;
            self.pos += 1;
        }
    }

    fn error(&self, msg: &str) -> ZError {
        ZError::Lexer(format!("line {}: {}", self.line, msg))
    }

    fn line_comment(&mut self) {
        while self.pos < self.chars.len() && self.peek() != '\n' {
            self.advance();
        }
    }

    fn block_comment(&mut self) -> ZResult<()> {
        self.advance(); // '/'
        self.advance(); // '*'
        let mut depth = 1;
        while self.pos < self.chars.len() && depth > 0 {
            if self.peek() == '*' && self.lookahead(1) == '/' {
                self.advance();
                self.advance();
                depth -= 1;
            } else if self.peek() == '/' && self.lookahead(1) == '*' {
                self.advance();
                self.advance();
                depth += 1;
            } else {
                self.advance();
            }
        }
        if depth > 0 {
            return Err(self.error("unterminated block comment"));
        }
        Ok(())
    }

    fn string(&mut self, quote: char) -> ZResult<()> {
        self.advance();
        if self.peek() == quote && self.lookahead(1) == quote {
            self.triple_string(quote)
        } else {
            self.normal_string(quote)
        }
    }

    fn normal_string(&mut self, quote: char) -> ZResult<()> {
        let mut s = String::new();
        loop {
            if self.pos >= self.chars.len() {
                return Err(self.error("unterminated string"));
            }
            let c = self.peek();
            if c == quote {
                self.advance();
                break;
            }
            if c == '\\' {
                self.advance();
                s.push(self.escape_sequence()?);
            } else if c == '\n' {
                return Err(self.error("newline in string"));
            } else {
                s.push(c);
                self.advance();
            }
        }
        self.push(TokenKind::String(s));
        Ok(())
    }

    fn triple_string(&mut self, quote: char) -> ZResult<()> {
        self.advance();
        self.advance();
        let mut s = String::new();
        loop {
            if self.pos >= self.chars.len() {
                return Err(self.error("unterminated triple-quoted string"));
            }
            let c = self.peek();
            if c == quote && self.lookahead(1) == quote && self.lookahead(2) == quote {
                self.advance();
                self.advance();
                self.advance();
                break;
            }
            if c == '\\' {
                self.advance();
                s.push(self.escape_sequence()?);
            } else {
                s.push(c);
                self.advance();
            }
        }
        self.push(TokenKind::String(s));
        Ok(())
    }

    fn raw_string(&mut self) -> ZResult<()> {
        self.advance();
        let quote = self.peek();
        self.advance();
        let mut s = String::new();
        loop {
            if self.pos >= self.chars.len() {
                return Err(self.error("unterminated raw string"));
            }
            let c = self.peek();
            if c == quote {
                self.advance();
                break;
            }
            s.push(c);
            self.advance();
        }
        self.push(TokenKind::String(s));
        Ok(())
    }

    fn fstring(&mut self) -> ZResult<()> {
        self.advance(); // 'f'
        let quote = self.peek();
        self.advance();
        let mut s = String::new();
        loop {
            if self.pos >= self.chars.len() {
                return Err(self.error("unterminated f-string"));
            }
            let c = self.peek();
            if c == quote {
                self.advance();
                break;
            }
            if c == '\\' {
                self.advance();
                s.push(self.escape_sequence()?);
            } else if c == '{' {
                s.push('{');
                self.advance();
                let mut depth = 1;
                while self.pos < self.chars.len() && depth > 0 {
                    let c = self.peek();
                    if c == '{' {
                        depth += 1;
                        s.push(c);
                    } else if c == '}' {
                        depth -= 1;
                        s.push(c);
                    } else {
                        s.push(c);
                    }
                    self.advance();
                }
            } else {
                s.push(c);
                self.advance();
            }
        }
        self.push(TokenKind::FString(s));
        Ok(())
    }

    fn escape_sequence(&mut self) -> ZResult<char> {
        let c = self.peek();
        self.advance();
        match c {
            'n' => Ok('\n'),
            't' => Ok('\t'),
            '\\' => Ok('\\'),
            '"' => Ok('"'),
            '\'' => Ok('\''),
            '0' => Ok('\0'),
            'r' => Ok('\r'),
            _ => Err(self.error(&format!("invalid escape sequence \\{}", c))),
        }
    }

    fn number(&mut self) -> ZResult<()> {
        let mut s = String::new();
        let start = self.pos;

        // Hex, binary, octal
        if self.peek() == '0' && matches!(self.lookahead(1), 'x' | 'X') {
            self.advance();
            self.advance();
            while self.peek().is_ascii_hexdigit() {
                s.push(self.peek());
                self.advance();
            }
            let val = i64::from_str_radix(&s, 16)
                .map_err(|_| self.error("invalid hexadecimal number"))?;
            self.push(TokenKind::Int(val));
            return Ok(());
        }
        if self.peek() == '0' && matches!(self.lookahead(1), 'b' | 'B') {
            self.advance();
            self.advance();
            while self.peek().is_ascii_digit() {
                s.push(self.peek());
                self.advance();
            }
            let val = i64::from_str_radix(&s, 2)
                .map_err(|_| self.error("invalid binary number"))?;
            self.push(TokenKind::Int(val));
            return Ok(());
        }
        if self.peek() == '0' && matches!(self.lookahead(1), 'o' | 'O') {
            self.advance();
            self.advance();
            while self.peek() >= '0' && self.peek() <= '7' {
                s.push(self.peek());
                self.advance();
            }
            let val = i64::from_str_radix(&s, 8)
                .map_err(|_| self.error("invalid octal number"))?;
            self.push(TokenKind::Int(val));
            return Ok(());
        }

        let mut is_float = false;
        while self.peek().is_ascii_digit() || self.peek() == '_' {
            if self.peek() != '_' {
                s.push(self.peek());
            }
            self.advance();
        }

        if self.peek() == '.' && self.lookahead(1).is_ascii_digit() {
            is_float = true;
            s.push('.');
            self.advance();
            while self.peek().is_ascii_digit() {
                s.push(self.peek());
                self.advance();
            }
        }

        if matches!(self.peek(), 'e' | 'E') {
            let mut look = 1;
            if matches!(self.lookahead(1), '+' | '-') {
                look = 2;
            }
            if self.lookahead(look).is_ascii_digit() {
                is_float = true;
                s.push('e');
                self.advance();
                if matches!(self.peek(), '+' | '-') {
                    s.push(self.peek());
                    self.advance();
                }
                while self.peek().is_ascii_digit() {
                    s.push(self.peek());
                    self.advance();
                }
            }
        }

        // If an identifier char follows a number, produce a nicer error
        if self.peek().is_alphabetic() || self.peek() == '_' {
            let raw: String = self.chars[start..self.pos].iter().collect();
            let bad: String = self.chars[self.pos..]
                .iter()
                .take_while(|c| c.is_alphanumeric() || **c == '_')
                .collect();
            return Err(self.error(&format!(
                "invalid number '{}' — identifiers cannot start with a digit",
                format!("{}{}", raw, bad)
            )));
        }

        if is_float {
            let val: f64 = s.parse().map_err(|_| self.error("invalid float number"))?;
            self.push(TokenKind::Float(val));
        } else {
            let val: i64 = s
                .parse()
                .map_err(|_| self.error("integer literal out of range"))?;
            self.push(TokenKind::Int(val));
        }
        Ok(())
    }

    fn identifier(&mut self) {
        let mut s = String::new();
        while (self.peek().is_alphanumeric() || self.peek() == '_') && self.pos < self.chars.len() {
            s.push(self.peek());
            self.advance();
        }

        let kind = match s.as_str() {
            "abl" => TokenKind::Abl,
            "and" => TokenKind::And,
            "as" => TokenKind::As,
            "assert" => TokenKind::Assert,
            "async" => TokenKind::Async,
            "await" => TokenKind::Await,
            "break" => TokenKind::Break,
            "call" => TokenKind::Call,
            "callclass" => TokenKind::Callclass,
            "callerror" => TokenKind::Callerror,
            "case" => TokenKind::Case,
            "const" => TokenKind::Const,
            "continue" => TokenKind::Continue,
            "del" => TokenKind::Del,
            "dir" => TokenKind::Dir,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "enum" => TokenKind::Enum,
            "except" => TokenKind::Except,
            "False" => TokenKind::False,
            "finally" => TokenKind::Finally,
            "fn" => TokenKind::Fn,
            "for" => TokenKind::For,
            "from" => TokenKind::From,
            "global" => TokenKind::Global,
            "if" => TokenKind::If,
            "import" => TokenKind::Import,
            "in" => TokenKind::In,
            "is" => TokenKind::Is,
            "match" => TokenKind::Match,
            "None" => TokenKind::None,
            "not" => TokenKind::Not,
            "or" => TokenKind::Or,
            "outer" => TokenKind::Outer,
            "pass" => TokenKind::Pass,
            "return" => TokenKind::Return,
            "self" => TokenKind::Self_,
            "static" => TokenKind::Static,
            "super" => TokenKind::Super,
            "True" => TokenKind::True,
            "try" => TokenKind::Try,
            "while" => TokenKind::While,
            "with" => TokenKind::With,
            "yield" => TokenKind::Yield,
            _ => TokenKind::Ident(s.clone()),
        };
        self.push(kind);
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}