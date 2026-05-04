use std::fmt;

use crate::{NumKind, ValueKind, parser::ParseError};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Operator {
    Add, Sub, Mul, Div, Mod,                                        // +, -, *, /, %
    AddAssign, SubAssign, MulAssign, DivAssign, ModAssign,          // +=, -=, *=, /=, %=
    BWAnd, BWOr, BWXor, BWNot, BWShiftL, BWShiftR,                  // &, |, ^, ~, <<, >>
    AndAssign, OrAssign, XorAssign, ShiftLAssign, ShiftRAssign,     // &=, |=, ^=, <<=, >>=
    Inc, Dec, Len,                                                  // ++, --, len
    Equal, NEqual, LT, GT, LTE, GTE,                                // ==, !=, <, >, <=, >=
    Not, And, Or,                                                   // !, &&, ||
    Range, RangeLT,                                                 // ..., ..<
    At, Hash, Dollar,                                               // @, #, $
    Dot, QDot, Colon, DColon, QDColon, Question, DQuestion, QCall,  //., ?., :, ::, ?::, ?, ??, ?(
    Assign, Attach, Detach, Arrow, FatArrow,                        // =, :=, =:, ->, =>
    IsShape, NIsShape                                               // =~, !~
}

pub const UNARY_OPERATORS: &[Operator] = &[ 
    Operator::BWNot, Operator::Not, 
    //Operator::Dec, Operator::Inc, 
    Operator::Sub, Operator::Len 
    ];
pub const BINARY_OPERATORS: &[Operator] = &[
    Operator::Add, Operator::Sub, Operator::Mul, Operator::Div, Operator::Mod,
    Operator::Equal, Operator::NEqual, Operator::GT, Operator::LT, Operator::GTE, Operator::LTE,
    Operator::BWAnd, Operator::BWOr, Operator::BWXor, Operator::BWShiftL, Operator::BWShiftR,
    Operator::And, Operator::Or, Operator::IsShape, Operator::NIsShape,
    Operator::Range, Operator::RangeLT,
    Operator::DQuestion,
];

const OP_CHARS: &[char] = &[
    '+', '-', '*', '/', '%',
    '&', '|', '^', '~',
    '=', '!', '<', '>',
    '.', '?', ':',
    '@', '#', '$'
];

impl Operator {
    pub fn kind(&self, operand: ValueKind) -> ValueKind {
        match self {
            Operator::Mul | Operator::Div | Operator::Sub | Operator::Mod |
            Operator::BWAnd | Operator::BWOr | Operator::BWXor | Operator::BWNot | Operator::BWShiftL | Operator::BWShiftR |
            Operator::Inc | Operator::Dec | Operator::Len
                => ValueKind::Number(NumKind::Any),

            Operator::Add => operand,
                
            Operator::Equal | Operator::NEqual | Operator::And | Operator::Or | Operator::Not |
            Operator::LT | Operator::GT | Operator::LTE | Operator::GTE | 
            Operator::IsShape | Operator::NIsShape 
                => ValueKind::Bool,
                
            Operator::Range | Operator::RangeLT
                => ValueKind::Array(Box::new(ValueKind::Number(NumKind::Int32))),

            Operator::DQuestion => match operand {
                ValueKind::Option(kind) => *kind,
                kind => kind
            }

            _ => ValueKind::None
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Keyword {
    // Atlas
    Namespace, Using, Alias,
    Chart, Dependencies,
    Lazy, Hidden, Trailhead,

    // Definitions
    Shape, Let, Enum, Extension, New,
    Static, Seal, Locked, Const, Abstract, Intrinsic, Private,
    Before, After, Next, 
    PSelf, Default, Underscore,
    Attach, Detach,

    // Statements
    If, Else, Switch,
    Try, Catch,
    While, Loop, For, In,
    Break, Return, Continue, Throw,
    Print,
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    pub file: String,
}

impl Span {
    pub fn new(line: usize, column: usize, file: String) -> Self {
        Span{line, column, file}
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-[{}:{}]", self.file, self.line, self.column)
    }
}

#[derive(Debug, Clone)]
pub enum TokenKind{
    Identifier(String),
    Number(f64, NumKind),
    Bool(bool),
    String(String),
    NoneValue,
    Operator(Operator),
    Keyword(Keyword),
    Type(ValueKind),
    StringFormat(String),
    LeftParen,
    RightParen,
    Comma,
    Semicolon,
    NewLine,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    EOF,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub span: Span,
    pub kind: TokenKind,
}

static WHITESPACE: &[char] = &[' ', '\n', '\t', '\r'];

type PeekableChars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

pub struct Lexer {
    line: usize,
    column: usize,
    file: String,

    interp_stack: Vec<usize>,
    depth: usize,
}

impl Lexer {
    pub fn new(file:String) -> Self {
        Lexer { 
            line: 1, 
            column: 1,
            file,
            interp_stack: Vec::new(),
            depth: 0, 
        }
    }

    pub fn span(&self) -> Span {
        Span {line: self.line, column:self.column, file: self.file.clone()}
    }

    pub fn ignore_whitespace(&mut self, ch:char, chars: &mut PeekableChars) -> bool {
        match ch {
            '\n' => {
                self.line += 1;
                self.column = 0;
                true
            },
            ws if WHITESPACE.contains(&ws) => true,
            '/' if chars.peek() == Some(&'/') => {
                while let Some(ch) = chars.next() {
                    self.column += 1;
                    if ch == '\n' {
                        self.line += 1;
                        self.column = 0;
                        break;
                    }
                }
                true
            }
            '/' if chars.peek() == Some(&'*') => {
                while let Some(ch) = chars.next() {
                    self.column += 1;
                    if ch == '\n' {
                        self.line += 1;
                        self.column = 0;
                    }
                    if ch == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        break;
                    }
                }
                true
            }
            _ => false
        }
    }

    pub fn parse_string(&mut self, chars: &mut PeekableChars) -> Result<Token, ParseError> {
        let span= self.span();
        let mut string = String::new();

        while let Some(ch) = chars.next() {
            self.column += 1;
            match ch {
                '"' => break,
                '\\' => {
                    match chars.peek().unwrap() {
                        // Escape characters
                        'n' => { string.push('\n'); self.column += 1; chars.next(); }
                        't' => { string.push('\t'); self.column += 1; chars.next(); }
                        'r' => { string.push('\r'); self.column += 1; chars.next(); }
                        '"' => { string.push('\"'); self.column += 1; chars.next(); }
                        '0' => { string.push('\0'); self.column += 1; chars.next(); }

                        // String format
                        '{' => {
                            self.column += 1; chars.next();
                            self.interp_stack.push(self.depth);
                            return Ok(Token{span:self.span(), kind:TokenKind::StringFormat(string)})
                        }

                        _ => string.push('\\'),
                    }
                }
                other => string.push(other),
            }
        }
        Ok(Token{span:self.span(), kind:TokenKind::String(string)})
    }

    pub fn parse_token_atlas(&mut self, ch:char, chars: &mut PeekableChars) -> Result<Token, ParseError> {
        match ch {
            '{' => Ok(Token{span:self.span(), kind:TokenKind::LeftBrace}),
            '}' => Ok(Token{span:self.span(), kind:TokenKind::RightBrace}),
            ',' => Ok(Token{span:self.span(), kind:TokenKind::Comma}),

            ':' if matches!(chars.peek(), Some(':')) => {
                chars.next();
                self.column += 1;
                Ok(Token{span:self.span(), kind:TokenKind::Operator(Operator::DColon)})
            }
            '-' if matches!(chars.peek(), Some('>')) => {
                chars.next();
                self.column += 1;
                Ok(Token{span:self.span(), kind:TokenKind::Operator(Operator::Arrow)})
            }

            _ => {
                let span= self.span();
                let mut identifier = String::new();
                identifier.push(ch);

                while let Some(&ch) = chars.peek() {
                    if !WHITESPACE.contains(&ch) {
                        identifier.push(ch);
                        chars.next();
                        self.column += 1;
                    } else {
                        break;
                    }
                }

                Ok(Token{span, kind: match identifier.as_str() {
                    "dependencies" => TokenKind::Keyword(Keyword::Dependencies),
                    "chart" => TokenKind::Keyword(Keyword::Chart),
                    "lazy" => TokenKind::Keyword(Keyword::Lazy),
                    "hidden" => TokenKind::Keyword(Keyword::Hidden),
                    "trailhead" => TokenKind::Keyword(Keyword::Trailhead),

                    _ => TokenKind::Identifier(identifier),
                }})
            }
        }
    }

    pub fn parse_token(&mut self, ch:char, chars: &mut PeekableChars) -> Result<Token, ParseError> {
        match ch {
            '0'..='9' => {
                let span= self.span();
                let mut digits = String::new();
                let mut radix = 10u32;

                // Check for base prefix
                if ch == '0' {
                    match chars.peek() {
                        Some('x') | Some('X') => { radix = 16; chars.next(); self.column += 1; }
                        Some('b') | Some('B') => { radix = 2;  chars.next(); self.column += 1; }
                        Some('o') | Some('O') => { radix = 8;  chars.next(); self.column += 1; }
                        Some('c') | Some('C') => { radix = 10; chars.next(); self.column += 1; }
                        _ => { digits.push(ch); }
                    }
                } else {
                    digits.push(ch);
                }

                // Loop through subsequent digits
                while let Some(&c) = chars.peek() {
                    let valid = match radix {
                        2  => matches!(c, '0'|'1'),             // Binary
                        8  => matches!(c, '0'..='7'),           // Octal
                        10 => c.is_ascii_digit() || c == '.',   // Decimal
                        16 => c.is_ascii_hexdigit(),            // Hex
                        _  => false,
                    };
                    if valid || c == '_' {
                        chars.next(); self.column += 1;
                        if c != '_' { digits.push(c); } // Push to number
                    } else {
                        break;
                    }
                }

                // Check type suffix
                let kind = match chars.peek() {
                    Some('i') => {
                        chars.next();
                        match chars.next() {
                            Some('8') => NumKind::Int8,
                            Some('1') if self.next_is('6', chars)? => NumKind::Int16,
                            Some('3') if self.next_is('2', chars)? => NumKind::Int32,
                            Some('6') if self.next_is('4', chars)? => NumKind::Int64,
                            Some(other) => return Err(ParseError::InvalidToken { span, token: format!("{}i{}", digits, other) }),
                            None => return Err(ParseError::EOF(format!("Number suffix")))
                        }
                    }
                    Some('u') => {
                        chars.next();
                        match chars.next() {
                            Some('8') => NumKind::UInt8,
                            Some('1') if self.next_is('6', chars)? => NumKind::UInt16,
                            Some('3') if self.next_is('2', chars)? => NumKind::UInt32,
                            Some('6') if self.next_is('4', chars)? => NumKind::UInt64,
                            Some(other) => return Err(ParseError::InvalidToken { span, token: format!("{}u{}", digits, other) }),
                            None => return Err(ParseError::EOF(format!("Number suffix")))
                        }
                    }
                    Some('f') => {
                        chars.next();
                        match chars.next() {
                            Some('3') if self.next_is('2', chars)? => NumKind::Float32,
                            Some('6') if self.next_is('4', chars)? => NumKind::Float64,
                            Some(other) => return Err(ParseError::InvalidToken { span, token: format!("{}f{}", digits, other) }),
                            None => return Err(ParseError::EOF(format!("Number suffix")))
                        }
                    }
                    _ => NumKind::Any
                };

                // Parse based on radix
                let value = if radix == 10 {
                    digits.parse::<f64>().map_err(|_| ParseError::InvalidToken{span:span.clone(), token:digits})?
                } else {
                    i64::from_str_radix(&digits, radix).map_err(|_| ParseError::InvalidToken{span:span.clone(), token:digits})? as f64
                };

                Ok(Token{span, kind:TokenKind::Number(value, kind)})
            },
            '"' => self.parse_string(chars),
            '{' => {
                if !self.interp_stack.is_empty() { self.depth += 1 }
                Ok(Token{span:self.span(), kind:TokenKind::LeftBrace})
            }
            '}' => {
                if !self.interp_stack.is_empty() {
                    if self.depth == *self.interp_stack.last().unwrap() {
                        self.interp_stack.pop();
                        return self.parse_string(chars)

                    } else { self.depth -= 1 }
                }
                Ok(Token{span:self.span(), kind:TokenKind::RightBrace})
            }
            '(' => Ok(Token{span:self.span(), kind:TokenKind::LeftParen}),
            ')' => Ok(Token{span:self.span(), kind:TokenKind::RightParen}),
            '[' => Ok(Token{span:self.span(), kind:TokenKind::LeftBracket}),
            ']' => Ok(Token{span:self.span(), kind:TokenKind::RightBracket}),
            ',' => Ok(Token{span:self.span(), kind:TokenKind::Comma}),
            ';' => Ok(Token{span:self.span(), kind:TokenKind::Semicolon}),
            _ if ch.is_alphanumeric() || ch == '_' => {
                let span= self.span();
                let mut identifier = String::new();
                identifier.push(ch);

                while let Some(&ch) = chars.peek() {
                    if ch.is_alphanumeric() || ch == '_' {
                        identifier.push(ch);
                        chars.next();
                        self.column += 1;
                    } else {
                        break;
                    }
                }

                Ok(Token{span, kind: match identifier.as_str() {
                    "true" => TokenKind::Bool(true),
                    "false" => TokenKind::Bool(false),
                    "null" => TokenKind::NoneValue,

                    "namespace" => TokenKind::Keyword(Keyword::Namespace),
                    "using" => TokenKind::Keyword(Keyword::Using),
                    "alias" => TokenKind::Keyword(Keyword::Alias),
                    "extension" => TokenKind::Keyword(Keyword::Extension),
                    "shape" => TokenKind::Keyword(Keyword::Shape),
                    "enum" => TokenKind::Keyword(Keyword::Enum),
                    "let" => TokenKind::Keyword(Keyword::Let),
                    "new" => TokenKind::Keyword(Keyword::New),
                    "static" => TokenKind::Keyword(Keyword::Static),
                    "attach" => TokenKind::Keyword(Keyword::Attach),
                    "detach" => TokenKind::Keyword(Keyword::Detach),
                    "self" => TokenKind::Keyword(Keyword::PSelf),
                    "seal" => TokenKind::Keyword(Keyword::Seal),
                    "locked" => TokenKind::Keyword(Keyword::Locked),
                    "abstract" => TokenKind::Keyword(Keyword::Abstract),
                    "intrinsic" => TokenKind::Keyword(Keyword::Intrinsic),
                    "private" => TokenKind::Keyword(Keyword::Private),
                    "const" => TokenKind::Keyword(Keyword::Const),
                    "before" => TokenKind::Keyword(Keyword::Before),
                    "after" => TokenKind::Keyword(Keyword::After),
                    "default" => TokenKind::Keyword(Keyword::Default),
                    "_" => TokenKind::Keyword(Keyword::Underscore),
                    "if" => TokenKind::Keyword(Keyword::If),
                    "else" => TokenKind::Keyword(Keyword::Else),
                    "switch" => TokenKind::Keyword(Keyword::Switch),
                    "try" => TokenKind::Keyword(Keyword::Try),
                    "catch" => TokenKind::Keyword(Keyword::Catch),
                    "while" => TokenKind::Keyword(Keyword::While),
                    "loop" => TokenKind::Keyword(Keyword::Loop),
                    "for" => TokenKind::Keyword(Keyword::For),
                    "in" => TokenKind::Keyword(Keyword::In),
                    "break" => TokenKind::Keyword(Keyword::Break),
                    "return" => TokenKind::Keyword(Keyword::Return),
                    "continue" => TokenKind::Keyword(Keyword::Continue),
                    "throw" => TokenKind::Keyword(Keyword::Throw),
                    "print" => TokenKind::Keyword(Keyword::Print),

                    "len" => TokenKind::Operator(Operator::Len),
                    "and" => TokenKind::Operator(Operator::And),
                    "or" => TokenKind::Operator(Operator::Or),

                    "number" => TokenKind::Type(ValueKind::Number(NumKind::Any)),
                    "range" => TokenKind::Type(ValueKind::Range(NumKind::Any)),

                    "int8" => TokenKind::Type(ValueKind::Number(NumKind::Int8)),
                    "byte" => TokenKind::Type(ValueKind::Number(NumKind::Int8)),

                    "int16" => TokenKind::Type(ValueKind::Number(NumKind::Int16)),
                    "short" => TokenKind::Type(ValueKind::Number(NumKind::Int16)),

                    "int32" => TokenKind::Type(ValueKind::Number(NumKind::Int32)),
                    "int" => TokenKind::Type(ValueKind::Number(NumKind::Int32)),

                    "int64" => TokenKind::Type(ValueKind::Number(NumKind::Int64)),
                    "long" => TokenKind::Type(ValueKind::Number(NumKind::Int64)),

                    "uint8" => TokenKind::Type(ValueKind::Number(NumKind::Int32)),
                    "ubyte" => TokenKind::Type(ValueKind::Number(NumKind::UInt8)),

                    "uint16" => TokenKind::Type(ValueKind::Number(NumKind::UInt16)),
                    "ushort" => TokenKind::Type(ValueKind::Number(NumKind::UInt16)),

                    "uint32" => TokenKind::Type(ValueKind::Number(NumKind::UInt32)),
                    "uint" => TokenKind::Type(ValueKind::Number(NumKind::UInt32)),

                    "uint64" => TokenKind::Type(ValueKind::Number(NumKind::UInt64)),
                    "ulong" => TokenKind::Type(ValueKind::Number(NumKind::UInt64)),

                    "float32" => TokenKind::Type(ValueKind::Number(NumKind::Float32)),
                    "float" => TokenKind::Type(ValueKind::Number(NumKind::Float32)),

                    "float64" => TokenKind::Type(ValueKind::Number(NumKind::Float64)),
                    "double" => TokenKind::Type(ValueKind::Number(NumKind::Float64)),
                    
                    "bool" => TokenKind::Type(ValueKind::Bool),
                    "string" => TokenKind::Type(ValueKind::String),
                    "void" => TokenKind::Type(ValueKind::None),
                    "object" => TokenKind::Type(ValueKind::Object(Vec::new())),

                    _ => TokenKind::Identifier(identifier),
                }})
            },
            _ if OP_CHARS.contains(&ch) => {
                let span= self.span();

                let operator = match ch {
                    '+' => if self.next_is('=', chars)? {
                            Operator::AddAssign
                        } else { Operator::Add }
                    '-' => if self.next_is('=', chars)? {
                            Operator::SubAssign
                        } else if self.next_is('>', chars)? {
                            Operator::Arrow
                        } else { Operator::Sub }
                    '*' => if self.next_is('=', chars)? {
                            Operator::MulAssign
                        } else { Operator::Mul }
                    '/' => if self.next_is('=', chars)? {
                            Operator::DivAssign
                        } else { Operator::Div }
                    '%' => if self.next_is('=', chars)? {
                            Operator::ModAssign
                        } else { Operator::Mod }
                    
                    '~' => Operator::BWNot,
                    '^' => if self.next_is('=', chars)? {
                            Operator::XorAssign
                        } else { Operator::BWXor }
                    '&' => if self.next_is('&', chars)? {
                            Operator::And
                        } else if self.next_is('=', chars)? {
                            Operator::AndAssign
                        } else { Operator::BWAnd }
                    '|' => if self.next_is('|', chars)? {
                            Operator::Or
                        } else if self.next_is('=', chars)? {
                            Operator::OrAssign
                        } else { Operator::BWOr }

                    '<' => if self.next_is('<', chars)? {
                            if self.next_is('=', chars)? {
                                Operator::ShiftLAssign
                            } else { Operator::BWShiftL }
                        } else if self.next_is('=', chars)? {
                            Operator::LTE
                        } else { Operator::LT }
                    '>' => if self.next_is('>', chars)? {
                            if self.next_is('=', chars)? {
                                Operator::ShiftRAssign
                            } else { Operator::BWShiftR }
                        } else if self.next_is('=', chars)? {
                            Operator::GTE
                        } else { Operator::GT }

                    '!' => if self.next_is('~', chars)? {
                            Operator::NIsShape
                        } else if self.next_is('=', chars)? {
                            Operator::NEqual
                        } else { Operator::Not }
                        
                    '=' => if self.next_is('=', chars)? {
                            Operator::Equal
                        } else if self.next_is('~', chars)? {
                            Operator::IsShape
                        } else if self.next_is(':', chars)? {
                            Operator::Detach
                        } else { Operator::Assign }
                        
                    '.' => if self.next_is('.', chars)?  {
                            match chars.next().unwrap() {
                                '.' => Operator::Range,
                                '<' => Operator::RangeLT,
                                ch => return Err(ParseError::InvalidToken{span, token:format!("..{}",ch)}),
                            }
                        } else { Operator::Dot }

                    ':' => if self.next_is('=', chars)? {
                            Operator::Attach
                        } else if self.next_is(':', chars)? {
                            Operator::DColon
                        } else { Operator::Colon }
                    
                    '?' => if self.next_is('.', chars)? {
                            Operator::QDot
                        } else if self.next_is('?', chars)? {
                            Operator::DQuestion
                        } else if self.next_is('(', chars)? {
                            Operator::QCall
                        } else if self.next_is(':', chars)? {
                            if self.next_is(':', chars)? {
                                Operator::QDColon
                            } else { return Err(ParseError::InvalidToken{span, token:format!("?:{}",ch)}) }
                        } else { Operator::Question }
                        
                    '@' => Operator::At,
                    '#' => Operator::Hash,
                    '$' => Operator::Dollar,
                    
                    ch => return Err(ParseError::InvalidToken{span, token:format!("{}",ch)})
                };

                Ok(Token{span:span.clone(), kind:TokenKind::Operator(operator)})
            }
            ch => return Err(ParseError::InvalidToken{span:self.span(), token:format!("{}",ch)})
        }
    }

    fn next_is(&mut self, next: char, chars: &mut PeekableChars) -> Result<bool, ParseError> {
        match chars.peek() {
            None => return Err(ParseError::EOF(format!(""))),
            Some(ch) if *ch == next => {
                chars.next();
                self.column += 1;
                Ok(true)
            }
            _ => Ok(false)
        }
    }

    pub fn tokenize(&mut self, input: &str, atlas: bool) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();
        let mut chars = input.chars().peekable();
        
        while let Some(ch) = chars.next() {
            self.column += 1;
            if self.ignore_whitespace(ch, &mut chars) { continue }

            if atlas {
                tokens.push(self.parse_token_atlas(ch, &mut chars)?)
            } else {
                tokens.push(self.parse_token(ch, &mut chars)?)
            }
        }

        tokens.push(Token{span:self.span(), kind:TokenKind::EOF});
        Ok(tokens)
    }
}

pub fn tokenize(input: &str, filename:String, atlas: bool) -> Result<Vec<Token>, ParseError> {
    Lexer::new(filename).tokenize(input, atlas)
}