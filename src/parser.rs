use std::collections::{HashMap, HashSet};

use crate::analyzer::{CompileError, LocalId};
use crate::executor::{OBJECT_INSTANCE, ShapeInstance};
use crate::intrinsic::IntrinsicOp;
use crate::lexer::{self, Keyword, Operator, Span, Token, TokenKind, UNARY_OPERATORS};
use crate::loader::{AtlasLocation, AtlasMapping, AtlasMappingFlags, NamespaceId};
use crate::{AzimuthFlags, Function, FunctionSignature, Value, ValueKind, intrinsic};

#[derive(Debug, Clone)]
pub enum ParseError {
    WithStatements { statements: Vec<Statement>, error: Box<ParseError> },
    Error { span: Span, message: String },
    InvalidToken { span: Span, token: String },
    UnexpectedToken { span: Span, token: TokenKind, loc: String },
    IncorrectToken { span: Span, token: TokenKind, expected: String, loc: String },
    IllegalModifier { span: Span, illegal: String, conflict: String },
    NotHighInNamespace { span: Span, identifier: String },
    NamespaceAzimuthNonStatic { span: Span, identifier: String },
    EOF(String)
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::WithStatements{ statements, error } => 
                write!(f, "{:?}\n{}", statements, error),

            ParseError::Error { span, message } =>
                write!(f, "{}: {}", span, message),
                
            ParseError::InvalidToken { span, token } =>
                write!(f, "{}: invalid token: {}", span, token),

            ParseError::UnexpectedToken { span, token, loc } => 
                write!(f, "{}: Unexpected token in {}: {:?}", span, loc, token),

            ParseError::IncorrectToken { span, token, expected, loc } => 
                write!(f, "{}: Expected {:?} in {}, got {:?}", span, expected, loc, token),

            ParseError::IllegalModifier { span, illegal, conflict } => 
                write!(f, "{}: Azimuth can not be {} and {}", span, conflict, illegal),
                
            ParseError::NotHighInNamespace{ span, identifier } => 
                write!(f, "{}: Declaration was not in the highest level of namespace: {}", span, identifier),
                
            ParseError::NamespaceAzimuthNonStatic{ span, identifier } => 
                write!(f, "{}: Azimuth in non-shape namespace is not static: {}", span, identifier),
                
            ParseError::EOF(loc) => 
                write!(f, "Unexpected end of file while parsing {}", loc),
        }
    }
}

pub type Identifier = String;

#[derive(Debug, Clone)]
enum Presence {
    Guaranteed,
    Optional,
}

#[derive(Debug, Clone)]
pub enum Expression {
    Value(Span, Value),
    Default(Span),
    StringFormat(Span, Vec<Expression>),
    Array(Span, Vec<Expression>, Option<ValueKind>),
    Set(Span, Vec<Expression>, Option<ValueKind>),
    Dict(Span, Vec<(Expression, Expression)>, Option<ValueKind>, Option<ValueKind>),
    Range(Span, Value, Value),
    Variable(Span, Identifier),
    Option(Span, Box<Option<Expression>>),
    Shape(Span, ShapeExpression),
    UnaryOp {
        span: Span,
        operator: Operator,
        operand: Box<Expression>,
    },
    BinaryOp {
        span: Span,
        left: Box<Expression>,
        operator: Operator,
        right: Box<Expression>,
    },
    Ternary {
        span: Span,
        condition: Box<Expression>,
        true_expr: Box<Expression>,
        else_expr: Box<Expression>,
    },
    MemberAccess {
        span: Span,
        target: Box<Expression>,
        qualifier: Option<NamespaceId>,
        member: Identifier,
        optional: bool,
        chained: bool,
    },
    ArrayAccess {
        span: Span,
        target: Box<Expression>,
        index: Box<Expression>,
        optional: bool,
        chained: bool,
    },
    FunctionCall {
        span: Span,
        caller: Box<Expression>,
        target: Box<Expression>,
        args: Vec<Expression>,
        optional: bool,
        chained: bool,
    },
    Function {
        span: Span,
        has_self: bool,
        input_types: Vec<RawFunctionParam>,
        output_type: ShapeExpression,
        func: Box<Option<FunctionBody>>,
        captures: Vec<Identifier>,
    },
    Reflection(Span, Box<Expression>)
}

#[derive(Debug, Clone)]
pub enum FunctionBody {
    Script(Statement),
    Intrinsic(IntrinsicOp),
}

#[derive(Debug, Clone)]
pub struct RawFunctionParam {
    pub value_type: ShapeExpression,
    pub identifier: Option<Identifier>
}

#[derive(Debug, Clone)]
pub struct RawFunction {
    pub has_self: bool,
    pub input_types: Vec<RawFunctionParam>,
    pub output_type: ShapeExpression,
    pub func: Option<FunctionBody>
}

#[derive(Debug, Clone)]
pub struct RawFunctionSignature {
    pub has_self: bool,
    pub input_types: Vec<ShapeExpression>,
    pub output_type: ShapeExpression,
}

#[derive(Debug, Clone)]
pub enum ShapeExpression {
    Shape(Span, Identifier),
    Primitive(Span, ValueKind),
    Array(Span, Box<ShapeExpression>),
    Set(Span, Box<ShapeExpression>),
    Dict(Span, Box<ShapeExpression>, Box<ShapeExpression>),
    Applied {
        span: Span,
        base: Identifier,
        args: Vec<ShapeExpression>,
    },
    FunctionSignature(Span, Box<RawFunctionSignature>),
    Optional(Span, Box<ShapeExpression>),
}

impl ShapeExpression {
    pub fn get_identifier(&self) -> String {
        use ShapeExpression::*;
        match self {
            Shape(_, identifier) => identifier.clone(),
            Primitive(_, kind) => todo!(),
            Array(_, expr) => todo!(),
            Applied {base, .. } => base.clone(),
            FunctionSignature(_, signature) => todo!(),
            Optional(_, expr) => expr.get_identifier(),
            _ => todo!(),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum MappingKind {
    Before,
    After,
    Strict
}

#[derive(Debug, Clone)]
pub struct RawMapping {
    pub from_slot: Identifier,
    pub to_slot: Identifier,
    pub kind: MappingKind,
    pub shape: ShapeExpression,
}

#[derive(Debug, Clone)]
pub struct RawAzimuth {
    pub name: Identifier,
    pub value_type: ShapeExpression,
    pub set_value: Option<Expression>,
    pub flags: crate::AzimuthFlags
}

#[derive(Debug, Clone)]
pub enum Statement {
    Using { span: Span, package: NamespaceId },
    DeclareAzimuth { 
        span: Span, 
        azimuth: RawAzimuth,
    },
    Namespace {
        span: Span,
        name: Identifier,
        content: Vec<Statement>,
    },
    DeclareShape { 
        span: Span, 
        name: Identifier, 
        slot_ids: Vec<RawAzimuth>,
        parents: Vec<ShapeExpression>,
        mappings: Vec<RawMapping>,
        generics: Vec<(ShapeExpression, Vec<ShapeExpression>)>,
        extension: bool,
    },
    DeclareObject { span: Span, name: Identifier, shape: ShapeExpression }, 
    DeclareLocal { span: Span, name: Identifier, value: Expression }, 
    Detach { span: Span, object: Expression, shape: ShapeExpression },
    AddMapping { span: Span, object: Expression, mapping: RawMapping },
    Attach { span: Span, object: Expression, shape: ShapeExpression, mappings: Vec<RawMapping> },
    Print { span: Span, expr: Expression },
    Expression { span: Span, expr: Expression },
    If {
        span: Span, 
        condition: Expression,
        true_statement: Box<Statement>,
        else_statement: Option<Box<Statement>>,
    },
    Switch {
        span: Span, 
        target: Expression,
        branch_statements: Vec<(Expression, Statement)>,
        else_statement: Option<Box<Statement>>,
    },
    While {
        span: Span, 
        condition: Expression,
        statement: Box<Statement>,
    },
    For {
        span: Span, 
        local: Identifier,
        target: Expression,
        statement: Box<Statement>,
    },
    ForInc {
        span: Span,
        local: Identifier,
        start: Expression,
        cond: Expression,
        inc: Box<Statement>,
        statement: Box<Statement>,
    },
    Try {
        span: Span, 
        try_statement: Box<Statement>,
        catch_statement: Box<Option<Statement>>,
    },
    Assign { span: Span, target: Expression, value: Expression, },
    AssignAugmented { span: Span, target: Expression, value: Expression, operator: Operator },
    Seal { span: Span, target: Expression },
    Block(Vec<Statement>),

    Break{ span: Span }, 
    Continue{ span: Span }, 
    Return{ span: Span, value: Expression },
    Throw{ span: Span, message: Expression },
}

type PeekableTokens = std::iter::Peekable<std::vec::IntoIter<Token>>;

fn infix_bp(op: &Operator) -> (u8, u8) {
    match op {
        Operator::Or                            => (3, 4),
        Operator::And                           => (5, 6),

        Operator::BWOr                          => (7, 8),
        Operator::BWXor                         => (9, 10),
        Operator::BWAnd                         => (11, 12),

        Operator::Equal | Operator::NEqual      => (13, 14),
        Operator::LT | Operator::GT |
        Operator::LTE | Operator::GTE           => (15, 16),

        Operator::BWShiftL | Operator::BWShiftR => (17, 18),

        Operator::Add | Operator::Sub           => (19, 20),
        Operator::Mul | Operator::Div |
        Operator::Mod                           => (21, 22),

        Operator::Range                         => (1, 2), 
        Operator::RangeLT                       => (1, 2), 

        Operator::IsShape                       => (1, 2), 
        Operator::NIsShape                      => (1, 2), 
        Operator::DQuestion                     => (1, 2), 

        _                                       => (0, 0),
    }
}

fn parse_expression(tokens: &mut PeekableTokens, min_bp: u8) -> Result<Expression, ParseError> {
    let token = tokens.next().unwrap();
    let span = token.span.clone();

    let mut expr = match token.kind {
        // Unary preceding operator
        TokenKind::Operator(operator) if UNARY_OPERATORS.contains(&operator) => {
            let expr = parse_expression(tokens, 25)?;
            Expression::UnaryOp{span, operator, operand: Box::new(expr)}
        }
        
        // Reflection
        TokenKind::Operator(Operator::At) => {
            let expr = parse_expression(tokens, 0)?;
            Expression::Reflection(span, Box::new(expr))
        }

        TokenKind::Number(num) => Expression::Value(span, (num as i32).into()),
        TokenKind::Bool(b) => Expression::Value(span, b.into()),
        TokenKind::String(s) => Expression::Value(span, s.into()),
        TokenKind::NoneValue => Expression::Value(span, Value::None),
        TokenKind::Keyword(Keyword::Default) => Expression::Default(span),

        // Lambda
        TokenKind::Operator(Operator::BWOr) => parse_lambda(span, tokens)?,

        // String format
        TokenKind::StringFormat(s) => {
            let mut expressions = Vec::new();
            expressions.push(Expression::Value(span.clone(), s.into()));

            while let Some(token) = tokens.peek() {
                let span = token.span.clone();
                match token.kind.clone() {
                    TokenKind::String(s) => {
                        tokens.next();
                        expressions.push(Expression::Value(span, (s.clone()).into()));
                        break;
                    }
                    TokenKind::StringFormat(s) => {
                        tokens.next();
                        expressions.push(Expression::Value(span, (s.clone()).into()));
                    }
                    _ => expressions.push(parse_expression(tokens, 0)?),
                }
            }

            Expression::StringFormat(span, expressions)
        }

        TokenKind::Identifier(identifier) => Expression::Variable(span, identifier),
        TokenKind::Keyword(Keyword::PSelf) => Expression::Variable(span, format!("self")),

        TokenKind::LeftParen => {
            let expr = parse_expression(tokens, 0)?;
            tokens.next(); // Consume right parenthesis
            expr
        }

        // Array literal
        TokenKind::LeftBracket => {
            let mut elements = Vec::new();

            while let Some(token) = tokens.peek() {
                if matches!(token.kind, TokenKind::RightBracket) {
                    tokens.next();
                    break;
                }

                elements.push(parse_expression(tokens, 0)?);

                if let Some(token) = tokens.peek()
                    && matches!(token.kind, TokenKind::Comma) {
                    tokens.next();
                }
            }

            Expression::Array(span, elements, None)
        }

        // Set literal
        TokenKind::LeftBrace => {
            match &tokens.peek().unwrap().kind {
                TokenKind::RightBrace => {
                    tokens.next();
                    Expression::Set(span, Vec::new(), None)
                }
                TokenKind::Operator(Operator::Colon) => {
                    tokens.next();
                    let next = next(tokens, format!("Empty Dict literal"))?;
                    match next.kind {
                        TokenKind::RightBrace => Expression::Dict(span, Vec::new(), None, None),
                        other => return Err(ParseError::IncorrectToken { span, token:other, expected:format!("}}"), loc: format!("Empty Dict literal") })
                    }
                }
                _ => {
                    let first = parse_expression(tokens, 0)?;

                    let token = next(tokens, format!("Set/Dict literal"))?;
                    match token.kind {
                        TokenKind::RightBrace => Expression::Set(span, [first].to_vec(), None),
                        TokenKind::Comma => {
                            // Set
                            let mut elements = [first].to_vec();

                            while let Some(token) = tokens.peek() {
                                if matches!(token.kind, TokenKind::RightBrace) {
                                    tokens.next();
                                    break;
                                }

                                elements.push(parse_expression(tokens, 0)?);

                                if let Some(token) = tokens.peek()
                                    && matches!(token.kind, TokenKind::Comma) {
                                    tokens.next();
                                }
                            }

                            Expression::Set(span, elements, None)
                        }
                        TokenKind::Operator(Operator::Colon) => {
                            // Dict
                            let first_val = parse_expression(tokens, 0)?;

                            let token = next(tokens, format!("Dict literal"))?;
                            match token.kind {
                                TokenKind::RightBrace => Expression::Dict(span, [(first, first_val)].to_vec(), None, None),
                                TokenKind::Comma => {
                                    let mut elements = [(first, first_val)].to_vec(); 

                                    while let Some(token) = tokens.peek() {
                                        if matches!(token.kind, TokenKind::RightBrace) {
                                            tokens.next();
                                            break;
                                        }

                                        let key = parse_expression(tokens, 0)?;

                                        let token = next(tokens, format!("Dict literal"))?;
                                        match token.kind {
                                            TokenKind::Operator(Operator::Colon) => {}
                                            other => return Err(ParseError::IncorrectToken { span, token:other, expected:format!("}}"), loc: format!("Empty Dict literal") })
                                        }

                                        elements.push((key, parse_expression(tokens, 0)?));

                                        if let Some(token) = tokens.peek()
                                            && matches!(token.kind, TokenKind::Comma) {
                                            tokens.next();
                                        }
                                    }

                                    Expression::Dict(span, elements, None, None)
                                }
                                other => return Err(ParseError::IncorrectToken { span, token:other, expected:format!(", or }}"), loc: format!("Dict literal") })
                            }
                        }
                        other => return Err(ParseError::IncorrectToken { span, token:other, expected:format!(", or :"), loc: format!("Set/Dict literal") })
                    }
                }
            }

        }

        token => return Err(ParseError::UnexpectedToken { span, token, loc:format!("value expression") }),
    };

    let span = token.span.clone();
    let mut caller = Expression::Value(span, Value::None);
    let mut qualifier: Option<NamespaceId> = None;

    // Check for member access / function call / array index
    let mut chained = false;
    while let Some(token) = tokens.peek() {
        let span = token.span.clone();

        match token.kind {
            TokenKind::Operator(Operator::DColon) | TokenKind::Operator(Operator::QDColon) => {
                tokens.next(); // Consume dcolon

                // Expect namespace identifier
                let token = tokens.next().unwrap();
                let identifier = match token.kind {
                    TokenKind::Identifier(identifier) => identifier,
                    other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected: format!("namespace identifier"), loc: format!("member access qualifier") })
                };

                // Add to qualifier
                match qualifier {
                    Some(namespace) => qualifier = Some(format!("{}::{}", namespace, identifier)),
                    None => qualifier = Some(identifier),
                }

                // Expect dot or colon next
                match &tokens.peek().unwrap().kind {
                    TokenKind::Operator(Operator::Dot) | TokenKind::Operator(Operator::QDot) |
                    TokenKind::Operator(Operator::DColon) | TokenKind::Operator(Operator::QDColon) | 
                    TokenKind::Operator(Operator::GT) => {} // Continue
                    other => return Err(ParseError::IncorrectToken { span:token.span, token:other.clone(), expected: format!("Access or next namespace"), loc: format!("member access") })
                }
            }
            TokenKind::Operator(Operator::Dot) | TokenKind::Operator(Operator::QDot) => {
                let optional = matches!(token.kind, TokenKind::Operator(Operator::QDot));
                if optional { chained = true }

                tokens.next(); // Consume dot

                if let TokenKind::Identifier(k) = tokens.next().unwrap().kind {
                    // Access Member
                    caller = expr.clone();
                    expr = Expression::MemberAccess{span, target: Box::new(expr), qualifier:qualifier.clone(), member: k, optional, chained };
                    qualifier = None;
                }
            }
            TokenKind::LeftParen | TokenKind::Operator(Operator::QCall) => {
                let optional = matches!(token.kind, TokenKind::Operator(Operator::QCall));
                if optional { chained = true }

                let token = tokens.next().unwrap();
                let span = token.span.clone();

                let mut args = Vec::new();

                while let Some(token) = tokens.peek() {
                    match token.kind {
                        TokenKind::RightParen => {
                            tokens.next();
                            break
                        }
                        TokenKind::Comma => {
                            tokens.next();
                        }
                        _ => {
                            args.push(parse_expression(tokens, 0)?);
                        }
                    }
                }

                expr = Expression::FunctionCall{span, caller:Box::new(caller.clone()), target:Box::new(expr), args, optional, chained};
            }
            
            TokenKind::LeftBracket => {
                tokens.next(); // Consume bracket
                let index = parse_expression(tokens, 0)?;
                tokens.next(); // Consume bracket
                expr = Expression::ArrayAccess{span, target:Box::new(expr), index:Box::new(index), optional:false, chained};
            }

            _ => break,
        }
    }

    // Check for operators
    loop {
        let Some(token) = tokens.peek() else { break };
        let span = token.span.clone();

        let op = match &token.kind {
            TokenKind::Operator(op) if lexer::BINARY_OPERATORS.contains(op) => op.clone(),
            _ => break,
        };

        let (left_bp, right_bp) = infix_bp(&op);
        if left_bp <= min_bp {
            break; // This op belongs to our caller, not us
        }

        tokens.next(); // consume the operator
        let right = parse_expression(tokens, right_bp)?;
        expr = Expression::BinaryOp { span, left: Box::new(expr), operator: op, right: Box::new(right) };
    }

    // Check for ternary
    if let Some(token) = tokens.peek() {
        let span = token.span.clone();
        if matches!(&token.kind, TokenKind::Operator(Operator::Question))
        {
            tokens.next(); // consume question mark
            
            let true_expr = parse_expression(tokens, 0)?;

            // Expect colon
            let token = tokens.peek().unwrap();
            if !matches!(token.kind, TokenKind::Operator(Operator::Colon)) {
                let span = token.span.clone();
                return Err(ParseError::IncorrectToken { span, token:token.kind.clone(), expected: format!(":"), loc: format!("Ternary operation") });
            }
            tokens.next(); // Consume colon
            
            let else_expr = parse_expression(tokens, 0)?;

            expr = Expression::Ternary{ span, condition:Box::new(expr), true_expr:Box::new(true_expr), else_expr:Box::new(else_expr)}
        }
    }

    Ok(expr)
}

fn parse_shape_expression(tokens: &mut PeekableTokens) -> Result<ShapeExpression, ParseError> {
    // Collection Literals
    match tokens.peek().unwrap().kind {
        // Array
        TokenKind::LeftBracket => {
            tokens.next(); // consume '['

            let inner = parse_shape_expression(tokens)?;

            let token = tokens.next().unwrap();
            match token.kind {
                TokenKind::RightBracket => return Ok(ShapeExpression::Array(token.span.clone(), Box::new(inner))),
                other => return Err(ParseError::IncorrectToken { span:token.span.clone(), token: other.clone(), expected: format!("}}"), loc: format!("shape set definition")}) 
            }
        }
        // Set & Dict
        TokenKind::LeftBrace => {
            tokens.next(); // consume '{'

            let inner = parse_shape_expression(tokens)?;

            let token = tokens.next().unwrap();
            match token.kind {
                TokenKind::RightBrace => return Ok(ShapeExpression::Set(token.span.clone(), Box::new(inner))),
                TokenKind::Operator(Operator::Colon) => {
                    let value = parse_shape_expression(tokens)?;
                    
                    let token = tokens.next().unwrap();
                    match token.kind {
                        TokenKind::RightBrace => {}
                        other => return Err(ParseError::IncorrectToken { span:token.span.clone(), token: other.clone(), expected: format!("}}"), loc: format!("shape dict definition")}) 
                    }

                    return Ok(ShapeExpression::Dict(token.span.clone(), Box::new(inner), Box::new(value)))
                }
                other => return Err(ParseError::IncorrectToken { span:token.span.clone(), token: other.clone(), expected: format!("}}"), loc: format!("shape set definition")}) 
            }
        }
        _ => {}
    }

    let token = tokens.next().unwrap();
    let span = token.span.clone();

    let mut base = match token.kind {
        TokenKind::Identifier(identifier) => ShapeExpression::Shape(span.clone(), identifier),
        TokenKind::Type(kind) => ShapeExpression::Primitive(span.clone(), kind),
        TokenKind::Operator(Operator::BWOr) => parse_function_signature(span.clone(), tokens)?,
        other => return Err(ParseError::UnexpectedToken { span, token:other, loc:format!("shape expression") }),
    };

    if let TokenKind::Operator(Operator::LT) = tokens.peek().unwrap().kind {
        tokens.next(); // consume '<'
        let mut args = Vec::new();

        loop {
            let arg = parse_shape_expression(tokens)?;
            args.push(arg);

            let token = tokens.next().unwrap();
            match token.kind {
                TokenKind::Comma => continue,
                TokenKind::Operator(Operator::GT) => break,
                other => return Err(ParseError::UnexpectedToken { span:token.span, token:other, loc:format!("shape generics") }),
            }
        }

        let base_identifier = match base {
            ShapeExpression::Shape(_, identifier) => identifier,
            other => todo!()
        };

        base = ShapeExpression::Applied { span:span.clone(), 
            base: base_identifier,
            args,
        }
    }

    while let Some(token) = tokens.peek() {
        match token.kind {
            //Option
            TokenKind::Operator(Operator::Question) => {
                tokens.next(); // consume '?'
                base = ShapeExpression::Optional(span.clone(), Box::new(base));
            }
            _ => break,
        }
    }

    Ok(base)
}

fn parse_function(shape: Option<ShapeExpression>, intrinsic_name:Option<String>, flags: AzimuthFlags, tokens: &mut PeekableTokens) -> Result<RawFunction, ParseError> {
    tokens.next(); // Consume LParen

    let mut input_types = Vec::new();

    let has_self = match shape {
        Some(shape) => {
            // Add self param
            let self_param = RawFunctionParam{value_type: shape, identifier:Some(format!("self"))};
            input_types.push(self_param);

            true
        }
        None => false,
    };
    // else if matches!(tokens.peek().unwrap().kind, TokenKind::Keyword(Keyword::PSelf)){
    //    tokens.next(); // Consume self keyword
//
        // Add self param
    //    let self_param = RawFunctionParam{value_type: shape, identifier:format!("self")};
    //    input_types.push(self_param);
//
    //    true
    //} else { false };

    while let Some(token) = tokens.peek() {
        let span = token.span.clone();
        match token.kind {
            TokenKind::Comma => {
                tokens.next();
            },
            TokenKind::RightParen => {
                tokens.next();
                break
            }
            _ => { 
                let value_type = parse_shape_expression(tokens)?;
                let identifier = match tokens.next().unwrap().kind {
                    TokenKind::Identifier(k) => Some(k),
                    TokenKind::Keyword(Keyword::Underscore) => None,
                    token => return Err(ParseError::UnexpectedToken { span, token, loc:format!("function parameter definition") }),
                };
                input_types.push(RawFunctionParam{value_type, identifier});
            }
        }
    }

    // Expect arrow
    let token = tokens.peek().unwrap();
    let span = token.span.clone();
    let output_type = if matches!(token.kind, TokenKind::Operator(Operator::Arrow)) {
        tokens.next(); // Consume arrow
        // Return type
        parse_shape_expression(tokens)?
    } else { ShapeExpression::Primitive(span.clone(), ValueKind::None) };

    // Statement
    let func = if flags.is_abstract {
        None
    } else if let Some(name) = intrinsic_name {
        Some(FunctionBody::Intrinsic(intrinsic::lookup(span, name)?))
    } else {
        Some(FunctionBody::Script(parse_statement(tokens)?))
    };

    Ok(RawFunction{ has_self, input_types, output_type, func })
}

fn parse_function_signature(span:Span, tokens: &mut PeekableTokens) -> Result<ShapeExpression, ParseError> {
    // Build parameters
    let mut params = Vec::new();
    match tokens.peek().unwrap().kind {
        TokenKind::LeftParen => {
            tokens.next(); // consume paren

            while let Some(token) = tokens.peek() {
                let span = token.span.clone();
                match token.kind {
                    TokenKind::RightParen => {
                        tokens.next();
                        break
                    }
                    TokenKind::Comma => {
                        tokens.next();
                    }
                    _ => params.push(parse_shape_expression(tokens)?),
                }
            }
        }
        _ => params.push(parse_shape_expression(tokens)?),
    }

    let mut return_type = ShapeExpression::Primitive(span.clone(), ValueKind::None);

    // Handle arrow
    if matches!(tokens.peek().unwrap().kind, TokenKind::Operator(Operator::Arrow)) {
        tokens.next();
        return_type = parse_shape_expression(tokens)?;
    }

    // Expect closing pipe
    let token = tokens.next().unwrap();
    if !matches!(token.kind, TokenKind::Operator(Operator::BWOr)) {
        return Err(ParseError::IncorrectToken { span:token.span, token:token.kind, expected:format!("|"), loc:format!("function signature") })
    }

    let signature = RawFunctionSignature{ input_types: params, output_type: return_type, has_self: false };
    Ok(ShapeExpression::FunctionSignature(span, Box::new(signature)))
}

fn parse_lambda(span:Span, tokens: &mut PeekableTokens) -> Result<Expression, ParseError> {
    // Build parameters
    let mut params = Vec::new();
    match tokens.peek().unwrap().kind {
        TokenKind::LeftParen => {
            tokens.next(); // consume paren

            while let Some(token) = tokens.peek() {
                let span = token.span.clone();
                match token.kind {
                    TokenKind::RightParen => {
                        tokens.next();
                        break
                    }
                    TokenKind::Comma => { tokens.next(); }
                    _ => {
                        let value_type = parse_shape_expression(tokens)?;
                        let token = tokens.next().unwrap();
                        let identifier = match token.kind {
                            TokenKind::Identifier(l) => Some(l),
                            TokenKind::Keyword(Keyword::Underscore) => None,
                            other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("parameter name"), loc:format!("lymphnoid parameters") })
                        };
                        params.push(RawFunctionParam { value_type, identifier });
                    }
                }
            }
        }
        _ => {
            let value_type = parse_shape_expression(tokens)?;
            let token = tokens.next().unwrap();
            let identifier = match token.kind {
                TokenKind::Identifier(l) => Some(l),
                TokenKind::Keyword(Keyword::Underscore) => None,
                other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("parameter name"), loc:format!("lymphnoid parameters") })
            };
            params.push(RawFunctionParam { value_type, identifier });
        }
    }

    let mut output_type = ShapeExpression::Primitive(span.clone(), ValueKind::None);
    let mut func = Some(FunctionBody::Script(Statement::Return { span:span.clone(), value:Expression::Value(span.clone(), Value::None) }));

    let captures = Vec::new();

    // Handle arrow
    if matches!(tokens.peek().unwrap().kind, TokenKind::Operator(Operator::Arrow)) {
        tokens.next();
        output_type = parse_shape_expression(tokens)?;
        let statement = parse_statement(tokens)?;
        func = Some(FunctionBody::Script(statement));
    }

    // Expect closing pipe
    let token = tokens.next().unwrap();
    if !matches!(token.kind, TokenKind::Operator(Operator::BWOr)) {
        return Err(ParseError::IncorrectToken { span:token.span, token:token.kind, expected:format!("|"), loc:format!("lymphnoid") })
    }

    Ok(Expression::Function{ span, has_self:false, input_types:params, output_type, func:Box::new(func), captures})
}

fn parse_object_statement(span:Span, tokens: &mut PeekableTokens) -> Result<Statement, ParseError> {
    // Build member call
    let object = parse_expression(tokens, 0)?;
    
    let token = tokens.peek().unwrap();
    match &token.kind {
        // Attach
        TokenKind::Operator(Operator::Attach) => {
            tokens.next(); // consume operator
            
            let shape = parse_shape_expression(tokens)?;
            let shape_name = shape.get_identifier();

            // Check for mappings
            if matches!(tokens.peek().unwrap().kind, TokenKind::LeftParen) {
                tokens.next(); // consume '{'
                let mut mappings = Vec::new();
                let mut values = HashMap::new();

                while let Some(token) = tokens.next() {
                    match token.kind {
                        // From slot mapping
                        TokenKind::Identifier(from_slot) => {

                            // Expect '->' operator
                            let token = tokens.next().unwrap();
                            match token.kind {
                                TokenKind::Operator(Operator::Arrow) => {

                                    // Check for before/after keywords
                                    let mut mapping_kind = MappingKind::Strict;
                                    match tokens.peek().unwrap().kind {
                                        TokenKind::Keyword(Keyword::Before) => {
                                            tokens.next();
                                            mapping_kind = MappingKind::Before
                                        }
                                        TokenKind::Keyword(Keyword::After) => {
                                            tokens.next();
                                            mapping_kind = MappingKind::After
                                        }
                                        _ => {}
                                    }

                                    // Expect to slot identifier
                                    match tokens.next().unwrap().kind {
                                        TokenKind::Identifier(to_slot) => mappings.push(RawMapping { from_slot, to_slot, kind:mapping_kind, shape:shape.clone() }),
                                        other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("Shape"), loc:format!("shape attachment remap value") }),
                                    }

                                },
                                TokenKind::Operator(Operator::Assign) => {
                                    let value = parse_expression(tokens, 0)?;
                                    values.insert(from_slot, value);
                                },
                                other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("->"), loc:format!("shape attachment remap value") }),
                            }
                        },
                        TokenKind::Comma => continue,
                        TokenKind::RightParen => break,
                        token => return Err(ParseError::UnexpectedToken { span, token, loc:format!("shape attachment remap") }),
                    }
                }

                Ok(Statement::Attach {span, object, shape, mappings })
            } else {
                Ok(Statement::Attach {span, object, shape, mappings:Vec::new() })
            }
        }

        //Detach
        TokenKind::Operator(Operator::Detach) => {
            tokens.next(); // consume operator
            
            let shape = parse_shape_expression(tokens)?;

            Ok(Statement::Detach {span, object, shape })
        }

        // Assign
        TokenKind::Operator(Operator::Assign) => {
            tokens.next(); // consume operator
            let value = parse_expression(tokens, 0)?;
            Ok(Statement::Assign{ span, target: object, value })
        }
        TokenKind::Operator(Operator::AddAssign) => {
            tokens.next(); // consume operator
            let value = parse_expression(tokens, 0)?;
            Ok(Statement::AssignAugmented{ span, target: object, value, operator:Operator::Add })
        }
        TokenKind::Operator(Operator::SubAssign) => {
            tokens.next(); // consume operator
            let value = parse_expression(tokens, 0)?;
            Ok(Statement::AssignAugmented{ span, target: object, value, operator:Operator::Sub })
        }
        TokenKind::Operator(Operator::MulAssign) => {
            tokens.next(); // consume operator
            let value = parse_expression(tokens, 0)?;
            Ok(Statement::AssignAugmented{ span, target: object, value, operator:Operator::Mul })
        }
        TokenKind::Operator(Operator::DivAssign) => {
            tokens.next(); // consume operator
            let value = parse_expression(tokens, 0)?;
            Ok(Statement::AssignAugmented{ span, target: object, value, operator:Operator::Div })
        }
        TokenKind::Operator(Operator::ModAssign) => {
            tokens.next(); // consume operator
            let value = parse_expression(tokens, 0)?;
            Ok(Statement::AssignAugmented{ span, target: object, value, operator:Operator::Mod })
        }
        TokenKind::Operator(Operator::AndAssign) => {
            tokens.next(); // consume operator
            let value = parse_expression(tokens, 0)?;
            Ok(Statement::AssignAugmented{ span, target: object, value, operator:Operator::BWAnd })
        }
        TokenKind::Operator(Operator::OrAssign) => {
            tokens.next(); // consume operator
            let value = parse_expression(tokens, 0)?;
            Ok(Statement::AssignAugmented{ span, target: object, value, operator:Operator::BWOr })
        }
        TokenKind::Operator(Operator::XorAssign) => {
            tokens.next(); // consume operator
            let value = parse_expression(tokens, 0)?;
            Ok(Statement::AssignAugmented{ span, target: object, value, operator:Operator::BWXor })
        }
        TokenKind::Operator(Operator::ShiftLAssign) => {
            tokens.next(); // consume operator
            let value = parse_expression(tokens, 0)?;
            Ok(Statement::AssignAugmented{ span, target: object, value, operator:Operator::BWShiftL })
        }
        TokenKind::Operator(Operator::ShiftRAssign) => {
            tokens.next(); // consume operator
            let value = parse_expression(tokens, 0)?;
            Ok(Statement::AssignAugmented{ span, target: object, value, operator:Operator::BWShiftR })
        }
        //TokenKind::Operator(Operator::Inc) => {
        //    tokens.next(); // consume operator
        //    let one = Expression::Value(span.clone(), 1.into());
        //    Ok(Statement::AssignAugmented{ span, target: object, value:one, operator:Operator::Add })
        //}
        //TokenKind::Operator(Operator::Dec) => {
        //    tokens.next(); // consume operator
        //    let one = Expression::Value(span.clone(), (-1).into());
        //    Ok(Statement::AssignAugmented{ span, target: object, value:one, operator:Operator::Add })
        //}

        _ => Ok(Statement::Expression{span, expr:object })

        //token => return Err(ParseError::UnexpectedToken { span, token:token.clone(), loc:format!("object operation") }),
    }
}

fn parse_azimuth(shape_identifier: Option<Identifier>, tokens: &mut PeekableTokens) -> Result<(RawAzimuth, Span), ParseError> {
    // Static
    let is_static = match tokens.peek().unwrap().kind {
        TokenKind::Keyword(Keyword::Static) => {
            tokens.next();
            true
        }
        _ => false
    };
    
    // Intrinsic
    let is_intrinsic = match tokens.peek().unwrap().kind {
        TokenKind::Keyword(Keyword::Intrinsic) => {
            tokens.next();
            true
        }
        _ => false
    };

    // Locked
    let is_locked = match tokens.peek().unwrap().kind {
        TokenKind::Keyword(Keyword::Locked) => {
            tokens.next();
            true
        }
        _ => false
    };

    // Const
    let is_const = match tokens.peek().unwrap().kind {
        TokenKind::Keyword(Keyword::Const) => {
            tokens.next();
            true
        }
        _ => false
    };

    // Abstract
    let is_abstract = match tokens.peek().unwrap().kind {
        TokenKind::Keyword(Keyword::Abstract) => {
            tokens.next();
            true
        }
        _ => false
    };

    // Slot name
    let token = tokens.next().unwrap();
    let span = token.span.clone();
    let slot_name = match token.kind {
        TokenKind::Identifier(k) => k,
        other => return Err(ParseError::UnexpectedToken { span:token.span, token:other, loc: format!("azimuth declaration") }),
    };

    // Intrinsic name
    let intrinsic_name = if is_intrinsic {
        match shape_identifier.clone() {
            Some(name) => Some(format!("{}::{}", name, slot_name.clone())),
            _ => Some(format!("{}", slot_name.clone())),
        }
    } else { None };

    // Function check
    if matches!(tokens.peek().unwrap().kind, TokenKind::LeftParen) {
        
        let flags = crate::AzimuthFlags { is_static:true, is_abstract, is_const, is_locked };

        // Function
        let shape_expr = match shape_identifier {
            Some(name) if !is_static => Some(ShapeExpression::Shape(span.clone(), name.clone())),
            _ => None,
        };
        let func = parse_function(shape_expr, intrinsic_name, flags.clone(), tokens)?;
        
        // Signature
        let sig_input_types = func.input_types.iter().map(|i| i.value_type.clone()).collect();
        let signature = RawFunctionSignature{ has_self:func.has_self, input_types:sig_input_types, output_type:func.output_type.clone() };
        let value_type = ShapeExpression::FunctionSignature(span.clone(), Box::new(signature));
        
        let func_expr = Expression::Function{ 
            span:span.clone(), 
            has_self:func.has_self,
            input_types: func.input_types, 
            output_type: func.output_type, 
            func:Box::new(func.func),
            captures:Vec::new(),
        };

        return Ok((RawAzimuth { 
            name: slot_name, 
            value_type,
            flags, 
            set_value: Some(func_expr), 
        }, span.clone()));
    }
    
    // Flags
    let flags = crate::AzimuthFlags { is_static, is_abstract, is_const, is_locked };

    // Type
    let value_type = parse_shape_expression(tokens)?;

    // Default
    let set_value = match tokens.peek().unwrap().kind {
        TokenKind::Operator(Operator::Assign) => {
            tokens.next();
            Some(parse_expression(tokens, 0)?)
        }
        _ => None
    };

    Ok((RawAzimuth { name: slot_name, value_type, flags, set_value }, span.clone()))
}

fn parse_statement(tokens: &mut PeekableTokens) -> Result<Statement, ParseError> {
    let token = tokens.peek().unwrap();
    let span = token.span.clone();

    match &token.kind {

        // Package import
        TokenKind::Keyword(Keyword::Using) => {
            tokens.next(); // consume 'using' keyword

            // First namespace
            let token = next(tokens, format!("package import"))?;
            let mut namespace_id = match token.kind {
                TokenKind::Identifier(name) => name,
                other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("namespace"), loc: format!("package import") })
            };

            // Subspaces
            while let Some(token) = tokens.peek() {
                match &token.kind {
                    TokenKind::Operator(Operator::DColon) => {
                        tokens.next();

                        let token = next(tokens, format!("package import"))?;
                        match token.kind {
                            TokenKind::Identifier(name) => {
                                namespace_id = format!("{}::{}", namespace_id, name);
                            }
                            other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("namespace"), loc: format!("package import") })
                        }
                    }
                    _ => break,
                }
            }

            Ok(Statement::Using {span, package:namespace_id })
        },

        // Shape declaration
        TokenKind::Keyword(Keyword::Shape) | TokenKind::Keyword(Keyword::Extension) => {
             // consume keyword and determine extension
            let extension = matches!(tokens.next().unwrap().kind, TokenKind::Keyword(Keyword::Extension));

            let shape_identifier = match tokens.next().unwrap().kind {
                TokenKind::Identifier(name) => name.clone(),
                token => return Err(ParseError::UnexpectedToken { span, token, loc:format!("shape declaration") }),
            };

            let mut generics = Vec::new();
            //let mut generic_names = Vec::new();

            // Establish generics
            if let TokenKind::Operator(Operator::LT) = tokens.peek().unwrap().kind {
                tokens.next(); // consume '<'

                while let Some(token) = tokens.peek() {
                    match &token.kind {
                        TokenKind::Identifier(_) => {
                            let shape_expr = parse_shape_expression(tokens)?;

                            // Dependencies
                            let mut dependencies = Vec::new();
                            while let Some(token) = tokens.peek() {
                                match &token.kind {
                                    TokenKind::Operator(Operator::Colon) => {
                                        tokens.next();
                                        let dependency = parse_shape_expression(tokens)?;
                                        dependencies.push(dependency);
                                    }
                                    TokenKind::Comma => {
                                        tokens.next();
                                    }
                                    _ => break
                                }
                            }
                            
                            generics.push((shape_expr, dependencies));
                        }
                        TokenKind::Comma => { tokens.next(); }
                        TokenKind::Operator(Operator::GT) => { 
                            tokens.next();
                            break
                        }
                        other => return Err(ParseError::UnexpectedToken { span:token.span.clone(), token:other.clone(), loc:format!("shape generic declaration") }),
                    }
                }
            }
            
            // Establish inheritance
            let mut mappings = Vec::new();
            let mut parents = Vec::new();
            
            let token = tokens.peek().unwrap();
            if let TokenKind::Operator(Operator::Colon) = token.kind {
                tokens.next(); // Consume colon

                while let Some(token) = tokens.peek() {
                    let span = token.span.clone();

                    match token.kind.clone() {
                        TokenKind::Comma => { tokens.next(); }
                        TokenKind::Identifier(_) => {
                            let parent = parse_shape_expression(tokens)?;
                            parents.push(parent.clone());

                            // Check for mappings
                            if let TokenKind::LeftParen = tokens.peek().unwrap().kind {
                                tokens.next(); // consume '{'

                                while let Some(token) = tokens.next() {
                                    match token.kind {
                                        // From slot mapping
                                        TokenKind::Identifier(from_slot) => {

                                            // Expect '->' operator
                                            let token = tokens.next().unwrap();
                                            if !matches!(token.kind, TokenKind::Operator(Operator::Arrow)) {
                                                return Err(ParseError::IncorrectToken { span, token:token.kind, expected:format!("->"), loc:format!("shape inheritance remap value") })
                                            }

                                            // Check for before/after keywords
                                            let mut mapping_kind = MappingKind::Strict;
                                            match tokens.peek().unwrap().kind {
                                                TokenKind::Keyword(Keyword::Before) => {
                                                    tokens.next();
                                                    mapping_kind = MappingKind::Before
                                                }
                                                TokenKind::Keyword(Keyword::After) => {
                                                    tokens.next();
                                                    mapping_kind = MappingKind::After
                                                }
                                                _ => {}
                                            }
                                            
                                            // Expect to slot identifier
                                            match tokens.next().unwrap().kind {
                                                TokenKind::Identifier(to_slot) => mappings.push(RawMapping { from_slot, to_slot, kind:mapping_kind, shape:parent.clone() }),
                                                token => return Err(ParseError::IncorrectToken { span, token, expected:format!("Slot"), loc:format!("shape inheritance remap value") }),
                                            }
                                        },
                                        TokenKind::Comma => {},
                                        TokenKind::RightParen => break,
                                        token => return Err(ParseError::UnexpectedToken { span, token, loc:format!("shape inheritance remap") }),
                                    }
                                }
                            }
                        },
                        _ => break,
                    }
                }

            }

            // Define slots
            let mut slot_ids = Vec::new();

            let token = tokens.next().unwrap();
            if let TokenKind::LeftBrace = token.kind {

                // Define slots
                while let Some(token) = tokens.peek() {
                    if matches!(token.kind, TokenKind::RightBrace) {
                        tokens.next();
                        break;
                    }

                    let (azimuth, _) = parse_azimuth(Some(shape_identifier.clone()), tokens)?;
                    slot_ids.push(azimuth);
                }
            } else {
                return Err(ParseError::IncorrectToken{span, token:token.kind, expected:format!("{{"), loc:format!("shape declaration")});
            }

            Ok(Statement::DeclareShape { span, name: shape_identifier, slot_ids, parents, mappings, generics, extension })
        },

        // Azimuth declaration within namespace
        TokenKind::Keyword(Keyword::Static) => {
            let (azimuth, span) = parse_azimuth(None, tokens)?;
            Ok(Statement::DeclareAzimuth{ span, azimuth })
        }

        // Namespace
        TokenKind::Keyword(Keyword::Namespace) => {
            tokens.next(); // Consume keyword

            let name = match next(tokens, format!("namespace declaration"))?.kind {
                TokenKind::Identifier(name) => name.clone(),
                token => return Err(ParseError::IncorrectToken { span, token, expected: format!("namespace identifier"), loc:format!("namespace declaration") }),
            };

            let brace = next(tokens, format!("namespace declaration"))?;
            match brace.kind {
                TokenKind::LeftBrace => {},
                other => return Err(ParseError::IncorrectToken { span:brace.span, token:other, expected: format!("{{"), loc:format!("namespace declaration") }),
            }
            let mut statements = Vec::new();
            while let Some(token) = tokens.peek() {
                match token.kind {
                    TokenKind::RightBrace => {
                        tokens.next(); // Consume brace
                        break
                    }
                    TokenKind::EOF => { return Err(ParseError::IncorrectToken { span:token.span.clone(), token:token.kind.clone(), expected:format!("}}"), loc:format!("namespace content") }); }
                    _ => statements.push(match parse_statement(tokens) {
                        Ok(statement) => statement,
                        Err(err) => return Err(ParseError::WithStatements{statements, error:Box::new(err)}),
                    })
                }
            }

            Ok(Statement::Namespace{span, name, content:statements})
        },

        // Object declaration
        TokenKind::Keyword(Keyword::Let) => {
            tokens.next(); // consume 'let' keyword
            let object_identifier = match tokens.next().unwrap().kind {
                TokenKind::Identifier(name) => name.clone(),
                token => return Err(ParseError::UnexpectedToken { span, token, loc:format!("object declaration") }),
            };

            let token = tokens.peek().unwrap();
            match token.kind {
                TokenKind::Operator(Operator::Attach) => {
                    // Object with attachment
                    tokens.next();
                    let shape = parse_shape_expression(tokens)?;
                    Ok(Statement::DeclareObject {span, name: object_identifier, shape })
                }
                TokenKind::Operator(Operator::Assign) => {
                    // Local with assignment
                    tokens.next();
                    let value = parse_expression(tokens, 0)?;
                    Ok(Statement::DeclareLocal {span, name: object_identifier, value })
                }
                _ => {
                    // Object without assignment
                    Ok(Statement::DeclareObject {span, name: object_identifier, shape:ShapeExpression::Primitive(token.span.clone(), ValueKind::Shape(OBJECT_INSTANCE)) })
                }
            }
        },
        
        // seal
        TokenKind::Keyword(Keyword::Seal) => {
            tokens.next(); // consume 'seal' keyword
            Ok(Statement::Seal{span, target: parse_expression(tokens, 0)?})
        },

        // print
        TokenKind::Keyword(Keyword::Print) => {
            tokens.next(); // consume 'print' keyword
            Ok(Statement::Print{span, expr: parse_expression(tokens, 0)?})
        },

        // Object Identifier
        TokenKind::Keyword(Keyword::PSelf) => parse_object_statement(span, tokens),
        TokenKind::Identifier(_) => parse_object_statement(span, tokens),

        // Blocking
        TokenKind::LeftBrace => {
            tokens.next(); // Consume brace

            // Add current statements to stack
            let mut statements = Vec::new();
            while let Some(token) = tokens.peek() {
                match token.kind {
                    TokenKind::RightBrace => {
                        tokens.next(); // Consume brace
                        break
                    }
                    TokenKind::EOF => { return Err(ParseError::IncorrectToken { span, token:token.kind.clone(), expected:format!("}}"), loc:format!("block") }); }
                    _ => statements.push(match parse_statement(tokens) {
                        Ok(statement) => statement,
                        Err(err) => return Err(ParseError::WithStatements{statements, error:Box::new(err)}),
                    })
                }
            }

            Ok(Statement::Block(statements))
        },

        // If
        TokenKind::Keyword(Keyword::If) => {
            tokens.next(); // Consume if

            let condition = parse_expression(tokens, 0)?;
            let true_statement = parse_statement(tokens)?;

            let else_token = tokens.peek().unwrap();
            let else_statement = match else_token.kind {
                TokenKind::Keyword(Keyword::Else) => {
                    tokens.next(); // Consume else
                    Some(Box::new(parse_statement(tokens)?))
                }
                _ => None
            };

            Ok(Statement::If{ span, condition, true_statement: Box::new(true_statement), else_statement: else_statement })
        },

        // Switch
        TokenKind::Keyword(Keyword::Switch) => {
            tokens.next(); // Consume keyword

            let target = parse_expression(tokens, 0)?;

            // Expect brace
            let token = tokens.next().unwrap();
            match token.kind {
                TokenKind::LeftBrace => {},
                other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("{{"), loc: format!("Switch statement") })
            }

            let mut branches = Vec::new();
            let mut else_statement = None;

            while let Some(token) = tokens.peek() {
                match token.kind {
                    TokenKind::RightBrace => { 
                        tokens.next(); 
                        break 
                    }
                    TokenKind::Comma => { tokens.next(); }
                    TokenKind::Keyword(Keyword::Else) => {
                        tokens.next();

                        // Expect Arrow
                        let token = tokens.next().unwrap();
                        match token.kind {
                            TokenKind::Operator(Operator::Arrow) => {},
                            other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("->"), loc: format!("Switch statement") })
                        }

                        else_statement = Some(Box::new(parse_statement(tokens)?));

                        // Expect closing brace
                        let token = tokens.next().unwrap();
                        match token.kind {
                            TokenKind::RightBrace => {},
                            other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("}}"), loc: format!("Switch statement") })
                        }

                        break
                    }
                    _ => {
                        let branch_expr = parse_expression(tokens, 0)?;

                        // Expect Arrow
                        let token = tokens.next().unwrap();
                        match token.kind {
                            TokenKind::Operator(Operator::Arrow) => {},
                            other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("->"), loc: format!("Switch statement") })
                        }

                        let branch_statement = parse_statement(tokens)?;

                        branches.push((branch_expr, branch_statement));
                    }
                }
            }

            Ok(Statement::Switch{ span, target, branch_statements:branches, else_statement })
        },

        // While
        TokenKind::Keyword(Keyword::While) => {
            tokens.next(); // Consume while

            let condition = parse_expression(tokens, 0)?;
            let statement = parse_statement(tokens)?;

            Ok(Statement::While{span, condition, statement: Box::new(statement) })
        },

        // Loop (while true)
        TokenKind::Keyword(Keyword::Loop) => {
            tokens.next(); // Consume loop

            let condition = Expression::Value(span.clone(), true.into());
            let statement = parse_statement(tokens)?;

            Ok(Statement::While{span, condition, statement: Box::new(statement) })
        },
        
        // For
        TokenKind::Keyword(Keyword::For) => {
            tokens.next(); // Consume keyword

            // Expect local identifier
            let local = match tokens.next().unwrap().kind {
                TokenKind::Identifier(k) => k,
                token => return Err(ParseError::IncorrectToken { span, token, expected: format!("identifier"), loc: format!("For loop header") }),
            };
            
            // Expect in keyword or assign pattern
            match tokens.next().unwrap().kind {

                TokenKind::Keyword(Keyword::In) => {
                    let target = parse_expression(tokens, 0)?;
                    let statement = parse_statement(tokens)?;
                    Ok(Statement::For{span, local, target, statement:Box::new(statement) })
                }

                TokenKind::Operator(Operator::Assign) => {
                    let start = parse_expression(tokens, 0)?; 
                    let cond = parse_expression(tokens, 0)?;
                    let statement = parse_statement(tokens)?;
                    let inc = parse_statement(tokens)?;
                    Ok(Statement::ForInc{span, local, start, cond, inc:Box::new(inc), statement:Box::new(statement) })
                }

                token => Err(ParseError::IncorrectToken { span, token, expected: format!("in or assign"), loc: format!("For loop header") }),
            }
        },

        // Try
        TokenKind::Keyword(Keyword::Try) => {
            tokens.next(); // Consume try

            let try_statement = parse_statement(tokens)?;

            let catch_token = tokens.peek().unwrap();
            let catch_statement = match catch_token.kind {
                TokenKind::Keyword(Keyword::Catch) => {
                    tokens.next(); // Consume catch
                    Some(parse_statement(tokens)?)
                }
                _ => None
            };

            Ok(Statement::Try{ span, try_statement: Box::new(try_statement), catch_statement: Box::new(catch_statement) })
        },

        // Returns
        TokenKind::Keyword(Keyword::Break) => {
            tokens.next();
            Ok(Statement::Break{span})
        }
        TokenKind::Keyword(Keyword::Continue) => {
            tokens.next();
            Ok(Statement::Continue{span})
        }
        TokenKind::Keyword(Keyword::Return) => {
            tokens.next();
            Ok(Statement::Return{span, value:parse_expression(tokens, 0)?})
        }
        TokenKind::Keyword(Keyword::Throw) => {
            tokens.next();
            Ok(Statement::Throw{span, message:parse_expression(tokens, 0)?})
        }

        token => Err(ParseError::UnexpectedToken { span, token:token.clone(), loc:format!("statements") }),
    }
}

pub struct ParsedAtlas{
    pub name: Identifier,
    pub dependencies: Option<Vec<Identifier>>,
    pub mappings: Vec<AtlasMapping>,
}

pub fn parse_atlas(tokens: &mut PeekableTokens) -> Result<ParsedAtlas, ParseError> {
    let mut name = format!("");

    // Dependencies
    let dependencies = match tokens.peek().unwrap().kind {
        TokenKind::Keyword(Keyword::Dependencies) => {
            tokens.next();
            let mut dependencies = Vec::new();

            // Expect brace
            match tokens.next() {
                Some(token) if matches!(token.kind, TokenKind::LeftBrace) => {},
                Some(other) => return Err(ParseError::IncorrectToken { span:other.span, token:other.kind, expected:format!("{{"), loc:format!("atlas dependencies") }),
                None => return Err(ParseError::EOF(format!("atlas dependencies")))
            }

            while let Some(token) = tokens.next() {
                let span = token.span.clone();
                match token.kind {
                    TokenKind::RightBrace => break,
                    TokenKind::Identifier(name) => {
                        dependencies.push(name);
                        todo!()
                    }
                    other => return Err(ParseError::UnexpectedToken { span, token:other, loc:format!("atlas dependencies") }),
                }
            }

            Some(dependencies)
        }
        _ => None
    };
    
    // Chart
    let token = next(tokens, format!("atlas chart"))?;
    match token.kind {
        TokenKind::Keyword(Keyword::Chart) => {},
        other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("chart"), loc:format!("atlas chart") }),
    }

    let token = next(tokens, format!("atlas chart"))?;
    match token.kind {
        TokenKind::Identifier(ident) => {
            name = ident;
        },
        other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("module name"), loc:format!("atlas chart") }),
    }
    
    let token = next(tokens, format!("atlas chart"))?;
    match token.kind {
        TokenKind::LeftBrace => {}
        other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("{{"), loc:format!("atlas chart") }),
    }
    
    let mut mappings = Vec::new();
    loop {
        let token = tokens.peek().unwrap();
        match &token.kind {
            TokenKind::RightBrace => {
                tokens.next(); 
                break   
            }
            TokenKind::Comma => {
                tokens.next();
            }
            _ => {
                let hidden = match tokens.peek().unwrap().kind {
                        TokenKind::Keyword(Keyword::Hidden) => {
                            tokens.next();
                            true
                        }
                        _ => false
                    };

                let lazy = match tokens.peek().unwrap().kind {
                        TokenKind::Keyword(Keyword::Lazy) => {
                            tokens.next();
                            true
                        }
                        _ => false
                    };

                let trailhead = match tokens.peek().unwrap().kind {
                        TokenKind::Keyword(Keyword::Trailhead) => {
                            tokens.next();
                            true
                        }
                        _ => false
                    };

                // Namespace
                let token = next(tokens, format!("atlas chart"))?;
                let from = match token.kind {
                    TokenKind::Identifier(from) => from,
                    other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("Namespace Identifier"), loc:format!("atlas chart") }),
                };

                // Arrow
                let token = next(tokens, format!("atlas chart"))?;
                match token.kind {
                    TokenKind::Operator(Operator::Arrow) => {}
                    other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("->"), loc:format!("atlas chart") }),
                }
                
                // Mapping
                let filename = parse_atlas_filename(tokens)?;

                mappings.push(AtlasMapping{from, to:filename, flags:AtlasMappingFlags{ hidden, lazy, trailhead }});
            }
        }
    }
    
    let token = next(tokens, format!("atlas"))?;
    match token.kind {
        TokenKind::EOF => {}
        other => return Err(ParseError::IncorrectToken { span:token.span, token:other, expected:format!("End Of File"), loc:format!("atlas") }),
    }

    Ok(ParsedAtlas{name, dependencies, mappings})
}

pub fn parse_atlas_dependency(tokens: &mut PeekableTokens) -> Result<AtlasLocation, ParseError> {
    todo!()
}

pub fn parse_atlas_filename(tokens: &mut PeekableTokens) -> Result<AtlasLocation, ParseError> {

    let token = next(tokens, format!("atlas namespace location"))?;
    let location = match token.kind {
        TokenKind::Identifier(name) => name,
        other => return Err(ParseError::UnexpectedToken { span:token.span, token:other, loc:format!("atlas namespace location") })
    };

    let token = tokens.peek();
    let subspace = match token {
        Some(token) if matches!(token.kind, TokenKind::Operator(Operator::DColon)) => {
            tokens.next(); // Consume ::
            let token = next(tokens, format!("atlas subspace identifier"))?;
            match token.kind {
                TokenKind::Identifier(name) => Some(name),
                other => return Err(ParseError::UnexpectedToken { span:token.span, token:other, loc:format!("atlas subspace identifier") })
            }
        }
        _ => None,
    };

    Ok(AtlasLocation::LocalFile{path:location})
}

pub fn next(tokens: &mut PeekableTokens, loc: String) -> Result<Token, ParseError> {
    match tokens.next() {
        Some(token) => Ok(token),
        None => Err(ParseError::EOF(loc)),
    }
}

pub fn parse(input: Vec<Token>) -> Result<Vec<Statement>, ParseError> {
    let mut statements = Vec::new();
    let mut tokens = input.into_iter().peekable();

    while let Some(token) = tokens.peek() {
        match token.kind {
            TokenKind::EOF => break,
            _ => statements.push(match parse_statement(&mut tokens) {
                Ok(statement) => statement,
                Err(err) => return Err(ParseError::WithStatements{statements, error:Box::new(err)}),
            })
        }
    }
    
    //println!("\n Parsed Ast: \n{:?}", statements);
    Ok(statements)
}

pub fn parse_atlas_file(input: Vec<Token>) -> Result<ParsedAtlas, ParseError> {
    let mut tokens = input.into_iter().peekable();
    Ok(parse_atlas(&mut tokens)?)
}