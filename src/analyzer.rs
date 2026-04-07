use std::{collections::{HashMap, HashSet}, fs};

use crate::{AzimuthFlags, AzimuthId, FunctionSignature, GenericId, NumKind, Number, ObjectId, ShapeId, Value, ValueKind, analyzer, executor::{self, OBJECT_INSTANCE, ShapeInstance}, intrinsic::IntrinsicOp, lexer::{self, Span}, loader::{Loader, Namespace, NamespaceId, NamespaceKind}, parser::{self, FunctionBody, MappingKind, RawMapping}}; 
use parser::{RawAzimuth, Expression, Statement, ShapeExpression};
use lexer::{Operator};

#[derive(Debug, Clone)]
pub enum CompileError {
    Error { span: Span, message: String },
    UndefinedSymbol { span: Span, name: String },
    UndefinedStatic { span: Span, name: String },
    DuplicateSymbol { span: Span, name: String },
    AmbiguousCall { span: Span, found: Vec<NamespaceId> },
    ExpectedBoolCondition { span: Span, found: ValueKind },
    TypeMismatch { span: Span, expected: ValueKind, found: ValueKind, loc: String },
    IncorrectReturn { span: Span, expected: ValueKind, found: ValueKind },
    InvalidUnaryOp { span: Span, operator: Operator, operand: Value },
    InvalidBinaryOp { span: Span, operator: Operator, left: Value, right: Value },
    InvalidThrow { span: Span, found: ValueKind },
    IoError { span: Span, message: String },
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::Error { span, message } =>
                write!(f, "{}: {}", span, message),

            CompileError::UndefinedSymbol { span, name } =>
                write!(f, "{}: Undefined symbol: {}", span, name),

            CompileError::DuplicateSymbol { span, name } =>
                write!(f, "{}: Duplicate symbol: {}", span, name),

            CompileError::ExpectedBoolCondition { span, found } =>
                write!(f, "{}: Expected bool condition, got {:?}", span, found),

            CompileError::AmbiguousCall { span, found } =>
                write!(f, "{}: Ambiguous call, multiple found- {:?}", span, found),

            CompileError::TypeMismatch { span, expected, found, loc } =>
                write!(f, "{}: Type mismatch: expected {:?} for {}, got {:?}", span, expected, loc, found),
                
            CompileError::IncorrectReturn { span, expected, found } =>
                write!(f, "{}: Return type mismatch: expected {:?}, got {:?}", span, expected, found),

            CompileError::InvalidUnaryOp { span, operator, operand } =>
                write!(f, "{}: Invalid unary operator {:?} on {:?}", span, operator, operand),

            CompileError::InvalidBinaryOp { span, operator, left, right } =>
                write!(f, "{}: Invalid binary operator {:?} on {:?} and {:?}", span, operator, left, right),

            CompileError::InvalidThrow { span, found } =>
                write!(f, "{}: Throw expects string, got {:?}", span, found),

            CompileError::UndefinedStatic { span, name } =>
                write!(f, "{}: Static not found for {}", span, name),

            CompileError::IoError { span, message } =>
                write!(f, "{}: IO error: {}", span, message),
        }
    }
}

pub type LocalId = u32;
type Identifier = String;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ShapeInfo {
    pub id: ShapeId,
    pub name: Identifier,
    pub static_id: Option<Box<ObjectInfo>>,
    pub azimuths: Vec<AzimuthId>,
    pub generics: Vec<ResolvedShapeExpression>,
    pub parent_ids: Vec<u32>,
    pub mappings: Vec<ResolvedMapping>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct AzimuthInfo {
    pub id: AzimuthId,
    pub name: Identifier,
    pub flags: AzimuthFlags,
    pub shape_id: ShapeId,
    pub default_value: Option<Box<ResolvedExpression>>,
    pub value_type: ValueKind,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ObjectInfo {
    pub id: ObjectId,
    pub name: Identifier,
    pub known_shapes: Vec<ValueKind>,
    pub static_origin: Option<NamespaceId>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LocalInfo {
    pub id: LocalId,
    pub name: Identifier,
    pub known_shapes: Vec<ValueKind>
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GenericInfo {
    pub id: GenericId,
    pub name: Identifier,
    pub known_shapes: Vec<ValueKind>
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct NamespaceIdentifier {
    pub id: NamespaceId,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Symbol {
    Object(ObjectInfo),
    Generic(GenericInfo),
    //Azimuth(AzimuthInfo),
    //Shape(ShapeInfo),
    Local(LocalInfo),
}

impl Symbol {
    fn get_name(&self) -> String {
        match self {
            //Symbol::Shape(info) => info.name.clone(),
            Symbol::Object(info) => info.name.clone(),
            //Symbol::Azimuth(info) => info.name.clone(),
            Symbol::Generic(info) => info.name.clone(),
            Symbol::Local(info) => info.name.clone(),
        }
    }
    
    fn kind(&self) -> ValueKind {
        match self {
            //Symbol::Shape(info) => ValueKind::Shape(ShapeInstance{id: info.id, generics: Vec::new()}),
            Symbol::Object(info) => ValueKind::Object(info.known_shapes.clone()),
            //Symbol::Azimuth(info) => info.value_type.clone(),
            Symbol::Generic(info) => ValueKind::Generic(info.id, info.known_shapes.clone()),
            Symbol::Local(info) => {
                if info.known_shapes.len() <= 1 {
                    info.known_shapes.first().unwrap().clone()
                } else {
                    ValueKind::Object(info.known_shapes.clone())
                }
            },
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum SymbolId {
    Identifier(String),
    Azimuth {
        name: String,
        clarifier: Option<String>,
    },
    Shape{
        name: String,
        namespace: String,
    },
}

type ScopeId = u32;

#[derive(Debug, Clone)]
pub struct Scope {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub symbols: HashMap<Identifier, Symbol>,
    pub generics: GenericId,
    pub locals: u32,
    pub using: Vec<NamespaceId>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ResolvedMapping {
    pub from: AzimuthInfo,
    pub to: AzimuthInfo,
    pub kind: MappingKind,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ResolvedFunctionBody {
    Script(ResolvedStatement),
    Intrinsic(IntrinsicOp),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ResolvedFunctionParameter {
    pub shape: ResolvedShapeExpression,
    pub local: LocalId
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ResolvedStatement {
    Using { span: Span, package: NamespaceId },
    Namespace { span: Span, name: Identifier, content: Vec<ResolvedStatement>},
    DeclareShape { span: Span, info: ShapeInfo, azimuths: Vec<AzimuthInfo> },
    DeclareObject { span: Span, local: LocalId, info: ObjectInfo, shape: ResolvedShapeExpression },
    DeclareLocal { span: Span, info: LocalInfo, value: ResolvedExpression },
    Detach { span: Span, object: ResolvedExpression, shape: ResolvedShapeExpression },
    AddMapping { span: Span, object: ResolvedExpression, mapping: ResolvedMapping },
    Attach { span: Span, object: ResolvedExpression, shape: ResolvedShapeExpression, mappings: Vec<ResolvedMapping>, },
    Print { span: Span, expr: ResolvedExpression },
    Expression { span: Span, expr: ResolvedExpression },
    If {
        span: Span, 
        condition: ResolvedExpression,
        true_statement: Box<ResolvedStatement>,
        else_statement: Option<Box<ResolvedStatement>>,
    },
    Switch {
        span: Span, 
        target: ResolvedExpression,
        branch_statements: Vec<(ResolvedExpression, ResolvedStatement)>,
        else_statement: Option<Box<ResolvedStatement>>,
    },
    While {
        span: Span, 
        condition: ResolvedExpression,
        statement: Box<ResolvedStatement>,
    },
    For {
        span: Span, 
        local: LocalId,
        target: ResolvedExpression,
        statement: Box<ResolvedStatement>,
    },
    ForInc {
        span: Span, 
        local: LocalId,
        start: ResolvedExpression,
        cond: ResolvedExpression,
        inc: Box<ResolvedStatement>,
        statement: Box<ResolvedStatement>,
    },
    Try {
        span: Span, 
        try_statement: Box<ResolvedStatement>,
        catch_statement: Box<Option<ResolvedStatement>>,
    },
    Assign { span: Span, target: ResolvedExpression, value: ResolvedExpression },
    Seal { span: Span, target: ResolvedExpression, },
    Block(Vec<ResolvedStatement>),
    Break{ span: Span, }, 
    Continue{ span: Span, }, 
    Return{span: Span, value: ResolvedExpression },
    Throw{span: Span, message: ResolvedExpression },
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ReturnStatus {
    Always,
    Conditionally,
    Never,
}

impl ResolvedStatement {

    pub fn walk_returns(&self, expected:ValueKind) -> Result<ReturnStatus, CompileError> {
        match self {
            ResolvedStatement::Return{span, value} => {
                if !value.kind().is_assignable_from(expected.clone()) { 
                    Err(CompileError::IncorrectReturn{span:span.clone(), expected, found: value.kind()})
                } else {
                    Ok(ReturnStatus::Always)
                }
            }
            ResolvedStatement::Block(statements) => {
                let mut conditionally_flag = false;

                for statement in statements {
                    match statement.walk_returns(expected.clone())? {
                        ReturnStatus::Always => return Ok(ReturnStatus::Always),
                        ReturnStatus::Conditionally => conditionally_flag = true,
                        ReturnStatus::Never => continue,
                    }
                }

                if conditionally_flag {
                    Ok(ReturnStatus::Conditionally)
                } else {
                    Ok(ReturnStatus::Never)
                }
            }
            ResolvedStatement::If{true_statement, else_statement, ..} => {
                let true_status = (*true_statement).walk_returns(expected.clone())?;
                
                let else_status = if let Some(statement) = else_statement.as_ref() {
                    statement.walk_returns(expected)?
                } else { ReturnStatus::Never };

                if true_status == ReturnStatus::Always && true_status == else_status {
                    Ok(ReturnStatus::Always)
                } else if true_status == ReturnStatus::Never && true_status == else_status {
                    Ok(ReturnStatus::Never)
                } else {
                    Ok(ReturnStatus::Conditionally)
                }
            }
            ResolvedStatement::Switch{branch_statements, else_statement, ..} => {
                let mut branches_status = ReturnStatus::Never;
                let mut always_count = 0;

                for (_, branch) in branch_statements {
                    match branch.walk_returns(expected.clone())? {
                        ReturnStatus::Always => {
                            branches_status = ReturnStatus::Conditionally;
                            always_count += 1;
                        }
                        ReturnStatus::Conditionally => {
                            branches_status = ReturnStatus::Conditionally;
                        }
                        ReturnStatus::Never => {}
                    }
                }

                if always_count == branch_statements.len() {
                    branches_status = ReturnStatus::Always
                }
                
                let else_status = match else_statement {
                    Some(statement) => statement.walk_returns(expected)?,
                    None => ReturnStatus::Never
                };

                if branches_status == ReturnStatus::Always && branches_status == else_status {
                    Ok(ReturnStatus::Always)
                } else if branches_status == ReturnStatus::Never && branches_status == else_status {
                    Ok(ReturnStatus::Never)
                } else {
                    Ok(ReturnStatus::Conditionally)
                }
            }
            ResolvedStatement::For {statement, ..} => statement.walk_returns(expected),
            ResolvedStatement::While {statement, ..} => statement.walk_returns(expected),
            _ => Ok(ReturnStatus::Never)
        }
    }

}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ResolvedShapeExpression {
    Simple(Span, ShapeInfo),
    Parameter(Span, Identifier), // T
    Primitive(Span, ValueKind),
    Array(Span, Box<ResolvedShapeExpression>),
    Set(Span, Box<ResolvedShapeExpression>),
    Dict(Span, Box<ResolvedShapeExpression>, Box<ResolvedShapeExpression>),
    Applied {
        span: Span,
        base: ShapeInfo,
        args: Vec<ResolvedShapeExpression>,
    },
    Optional(Span, Box<ResolvedShapeExpression>),
}

impl ResolvedShapeExpression {
    pub fn kind(&self) -> ValueKind {
        match self {
            ResolvedShapeExpression::Simple(_, info) => ValueKind::Shape(ShapeInstance{id: info.id, generics: Vec::new()}),
            ResolvedShapeExpression::Parameter(_, _) => ValueKind::Generic(0,Vec::new()),
            ResolvedShapeExpression::Applied{base: info, args, ..} => ValueKind::Shape(ShapeInstance{
                id: info.id,
                generics: args.iter().map(|expr| expr.kind()).collect()
            }),
            ResolvedShapeExpression::Primitive(_, kind) => kind.clone(),
            ResolvedShapeExpression::Array(_, expr) => ValueKind::Array(Box::new(expr.kind())),
            ResolvedShapeExpression::Set(_, expr) => ValueKind::Set(Box::new(expr.kind())),
            ResolvedShapeExpression::Dict(_, key, value) => ValueKind::Dict(Box::new(key.kind()), Box::new(value.kind())),
            ResolvedShapeExpression::Optional(_, expr) => ValueKind::Option(Box::new(expr.kind())),
            _ => unreachable!()
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ResolvedExpression {
    Value(Span, Value),
    Array(Span, Vec<ResolvedExpression>, ValueKind),
    Set(Span, Vec<ResolvedExpression>, ValueKind),
    Dict(Span, Vec<(ResolvedExpression, ResolvedExpression)>, ValueKind, ValueKind),
    StringFormat(Span, Vec<ResolvedExpression>),
    Variable(Span, Symbol),
    Option(Span, Box<Option<ResolvedExpression>>),
    Shape(Span, ResolvedShapeExpression),
    UnaryOp {
        span: Span,
        operator: Operator,
        operand: Box<ResolvedExpression>,
    },
    BinaryOp {
        span: Span,
        left: Box<ResolvedExpression>,
        operator: Operator,
        right: Box<ResolvedExpression>,
    },
    Ternary {
        span: Span,
        condition: Box<ResolvedExpression>,
        true_expr: Box<ResolvedExpression>,
        else_expr: Box<ResolvedExpression>,
    },
    MemberAccess  {
        span: Span,
        target: Box<ResolvedExpression>,
        member: AzimuthInfo,
        optional: bool,
        chained: bool,
    },
    ArrayAccess  {
        span: Span,
        target: Box<ResolvedExpression>,
        index: Box<ResolvedExpression>,
        optional: bool,
        chained: bool,
    },
    FunctionCall  {
        span: Span,
        target: Box<ResolvedExpression>,
        args: Vec<ResolvedExpression>,
        optional: bool,
        chained: bool,
    },
    Function {
        span: Span,
        has_self: bool,
        input_types: Vec<ResolvedFunctionParameter>,
        output_type: ResolvedShapeExpression,
        func: Box<Option<ResolvedFunctionBody>>,
        captures: Vec<LocalId>,
    },
    Reflection(Span, Box<ResolvedExpression>),
}

impl ResolvedExpression {
    pub fn get_name(&self) -> String {
        match self {
            ResolvedExpression::Variable(_, symbol) => symbol.get_name(),
            ResolvedExpression::MemberAccess { member, target, .. } => format!("{}.{}", target.get_name(), member.name.clone()),
            ResolvedExpression::ArrayAccess { target, index, .. } => format!("{}[{}]", target.get_name(), index.get_name()),
            other => format!("{:?}", other.kind()),
        }
    }

    pub fn is_assignable(&self) -> bool {
        match self {
            ResolvedExpression::Variable(_, symbol) => true,
            ResolvedExpression::MemberAccess { member, .. } => match member {
                info => !info.flags.is_const,
                _ => false
            },
            ResolvedExpression::ArrayAccess { target, .. } => true,
            _ => false
        }
    }

    pub fn kind(&self) -> ValueKind {
        match self {
            ResolvedExpression::Value(_, value) => value.kind(),
            ResolvedExpression::Array(_, _, value_type) => ValueKind::Array(Box::new(value_type.clone())),
            ResolvedExpression::Set(_, _, value_type) => ValueKind::Set(Box::new(value_type.clone())),
            ResolvedExpression::Dict(_, _, key_type, value_type) => ValueKind::Dict(Box::new(key_type.clone()), Box::new(value_type.clone())),
            ResolvedExpression::StringFormat(_, _) => ValueKind::String,
            ResolvedExpression::Variable(_, symbol) => symbol.kind(),
            ResolvedExpression::Option(_, option) => match *option.clone() {
                Some(expr) => expr.kind(),
                None => ValueKind::None,
            }
            ResolvedExpression::Shape(_, value) => value.kind(),
            ResolvedExpression::UnaryOp { operator, operand, .. } => operator.kind(operand.kind()),
            ResolvedExpression::BinaryOp { operator, left, .. } => operator.kind(left.kind()),
            ResolvedExpression::Ternary { true_expr, .. } => true_expr.kind(),
            ResolvedExpression::MemberAccess { member, .. } => member.value_type.clone(),
            ResolvedExpression::ArrayAccess { target, span, .. } => match target.kind() {
                ValueKind::Array(value_type) => *value_type.clone(),
                ValueKind::Range(num_kind) => ValueKind::Number(num_kind),
                ValueKind::String => ValueKind::String,
                other => panic!("{:?}: Iterable caled on {:?}", span, other)
            }
            ResolvedExpression::FunctionCall { target, .. } => match target.kind() {
                ValueKind::Function(info) => info.output_type.clone(),
                _ => unreachable!()
            }
            ResolvedExpression::Function { input_types, output_type, has_self, .. } => {
                let resolved_out = output_type.kind();
                let resolved_in = input_types.iter().map(|i| i.shape.kind()).collect();
                ValueKind::Function(Box::new(FunctionSignature{ input_types:resolved_in, output_type:resolved_out, has_self:*has_self }))
            }
            ResolvedExpression::Reflection(_, expr) => match *expr.clone() {
                ResolvedExpression::MemberAccess { member, .. } => ValueKind::Azimuth(Box::new(member.value_type)),
                ResolvedExpression::Variable(_, symbol) => todo!(),
                _ => todo!()
            }
        }
    }

    pub fn span(&self) -> &Span {
        match self {
            ResolvedExpression::Value(span, _) => span,
            ResolvedExpression::Array(span, _, _) => span,
            ResolvedExpression::Set(span, _, _) => span,
            ResolvedExpression::Dict(span, _, _, _) => span,
            ResolvedExpression::StringFormat(span, _) => span,
            ResolvedExpression::Variable(span, _) => span,
            ResolvedExpression::Option(span, _) => span,
            ResolvedExpression::Shape(span, _) => span,
            ResolvedExpression::UnaryOp { span, .. } => span,
            ResolvedExpression::BinaryOp { span, .. } => span,
            ResolvedExpression::Ternary { span, .. } => span,
            ResolvedExpression::MemberAccess { span, .. } => span,
            ResolvedExpression::ArrayAccess { span, .. } => span,
            ResolvedExpression::FunctionCall { span, .. } => span,
            ResolvedExpression::Function { span, .. } => span,
            ResolvedExpression::Reflection(span, _) => span,
        }
    }
}

pub fn type_binary_operands(operator: Operator) -> ValueKind {
    match operator {
        Operator::Mul | Operator::Div | Operator::Sub | Operator::Mod |
        Operator::BWAnd | Operator::BWOr | Operator::BWXor | Operator::BWNot | Operator::BWShiftL | Operator::BWShiftR |
        Operator::Inc | Operator::Dec | Operator::Range | Operator::RangeLT
            => ValueKind::Number(NumKind::Int32),
            
        Operator::And | Operator::Or | Operator::Not
            => ValueKind::Bool,
            
        _ => ValueKind::None
    }
}

pub fn compile_time_error(span: Span, message:String) {
    panic!("[{:?}:{:?}]: {}", span.line, span.column, message);
}

macro_rules! numeric_binop {
    ($span:expr, $left:expr, $right:expr, $operator:expr, $t:ty) => {{
        let left = $left as $t;
        let right = $right as $t;
        match $operator {
            Operator::Equal    => Ok(ResolvedExpression::Value($span, (left == right).into())),
            Operator::NEqual   => Ok(ResolvedExpression::Value($span, (left != right).into())),
            Operator::LT       => Ok(ResolvedExpression::Value($span, (left < right).into())),
            Operator::GT       => Ok(ResolvedExpression::Value($span, (left > right).into())),
            Operator::LTE      => Ok(ResolvedExpression::Value($span, (left <= right).into())),
            Operator::GTE      => Ok(ResolvedExpression::Value($span, (left >= right).into())),
            Operator::Add      => Ok(ResolvedExpression::Value($span, (left + right).into())),
            Operator::Sub      => Ok(ResolvedExpression::Value($span, (left - right).into())),
            Operator::Mul      => Ok(ResolvedExpression::Value($span, (left * right).into())),
            Operator::Div      => Ok(ResolvedExpression::Value($span, (left / right).into())),
            Operator::Mod      => Ok(ResolvedExpression::Value($span, (left % right).into())),
            Operator::BWAnd    => Ok(ResolvedExpression::Value($span, (left & right).into())),
            Operator::BWOr     => Ok(ResolvedExpression::Value($span, (left | right).into())),
            Operator::BWXor    => Ok(ResolvedExpression::Value($span, (left ^ right).into())),
            Operator::BWShiftL => Ok(ResolvedExpression::Value($span, (left << right).into())),
            Operator::BWShiftR => Ok(ResolvedExpression::Value($span, (left >> right).into())),
            Operator::Range    => Ok(ResolvedExpression::Value($span, executor::create_range(left as i32, right as i32, true))),
            Operator::RangeLT  => Ok(ResolvedExpression::Value($span, executor::create_range(left as i32, right as i32, false))),
            op => Err(CompileError::InvalidBinaryOp { span: $span, operator: op, left: left.into(), right: right.into() }),
        }
    }};
}

macro_rules! float_binop {
    ($span:expr, $left:expr, $right:expr, $operator:expr, $t:ty) => {{
        let left = $left as $t;
        let right = $right as $t;
        match $operator {
            Operator::Equal    => Ok(ResolvedExpression::Value($span, (left == right).into())),
            Operator::NEqual   => Ok(ResolvedExpression::Value($span, (left != right).into())),
            Operator::LT       => Ok(ResolvedExpression::Value($span, (left < right).into())),
            Operator::GT       => Ok(ResolvedExpression::Value($span, (left > right).into())),
            Operator::LTE      => Ok(ResolvedExpression::Value($span, (left <= right).into())),
            Operator::GTE      => Ok(ResolvedExpression::Value($span, (left >= right).into())),
            Operator::Add      => Ok(ResolvedExpression::Value($span, (left + right).into())),
            Operator::Sub      => Ok(ResolvedExpression::Value($span, (left - right).into())),
            Operator::Mul      => Ok(ResolvedExpression::Value($span, (left * right).into())),
            Operator::Div      => Ok(ResolvedExpression::Value($span, (left / right).into())),
            Operator::Mod      => Ok(ResolvedExpression::Value($span, (left % right).into())),
            op => Err(CompileError::InvalidBinaryOp { span: $span, operator: op, left: left.into(), right: right.into() }),
        }
    }};
}

pub struct Analyzer{
    pub scopes: Vec<Scope>,
    pub loader: Loader,
    pub namespace_scopes: HashMap<u32, ScopeId>,
    pub namespace_static_info: HashMap<u32, (ObjectInfo, Vec<AzimuthId>)>,

    pub azimuths: HashMap<u32, AzimuthInfo>,
    pub shapes: HashMap<u32, ShapeInfo>,

    pub next_scope_id: u32,
    pub next_object_id: u32,
}

impl Analyzer {
    pub fn new(loader:Loader) -> Self {
        let global = Scope{
            id: 0, 
            parent: None, 
            symbols: HashMap::new(), 
            locals:0, 
            generics:0, 
            using: Vec::new(),
        };
        Analyzer{
            scopes: [global].to_vec(), 
            loader,
            next_scope_id: 1,
            next_object_id: 1,
            namespace_scopes: HashMap::new(),
            namespace_static_info: HashMap::new(),
            azimuths: HashMap::new(),
            shapes: HashMap::new(),
        }
    }

    pub fn get_scope(&self, id:ScopeId) -> &Scope {
        self.scopes.get(id as usize).expect(format!("Scope not found: {:?}", id).as_str())
    }

    pub fn get_scope_mut(&mut self, id:ScopeId) -> &mut Scope {
        self.scopes.get_mut(id as usize).expect(format!("Scope not found: {:?}", id).as_str())
    }

    pub fn create_scope(&mut self, parent:ScopeId) -> ScopeId {
        let id = self.next_scope_id;
        let parent_scope = self.get_scope(parent);

        let scope = Scope{
            id: id.clone(), 
            parent: Some(parent), 
            symbols: HashMap::new(), 
            locals:parent_scope.locals, 
            generics:parent_scope.generics,
            using: Vec::new(),
        };

        self.next_scope_id += 1;
        self.scopes.push(scope);
        id
    }

    pub fn get_using(&self, id:ScopeId) -> Vec<NamespaceId> {
        let scope = self.get_scope(id);

        let mut using = scope.using.clone();
        match scope.parent {
            Some(parent) => {
                using.append(&mut self.get_using(parent));
            }
            _ => {}
        }
        using
    }

    pub fn get_symbol(&self, id:ScopeId, identifier:Identifier) -> Option<&Symbol> {
        let scope = self.get_scope(id);
        match scope.symbols.get(&identifier) {
            Some(symbol) => Some(symbol),
            _ => {
                match scope.parent {
                    Some(parent) => self.get_symbol(parent, identifier),
                    _ => None
                }
            }
        }
    }

    fn add_local_known_type(&mut self, span:&Span, scope:ScopeId, id:LocalId, shape:ResolvedShapeExpression) -> Result<(), CompileError> {
        let scope = self.get_scope_mut(scope);
        for symbol in scope.symbols.values_mut() {
            match symbol {
                Symbol::Local(info) if info.id == id => {
                    info.known_shapes.push(shape.kind());
                    return Ok(())
                },
                _ => continue
            }
        }

        println!("Beans");

        let parent: Option<u32> = scope.parent.clone();
        match parent {
            Some(parent) => self.add_local_known_type(span, parent, id, shape),
            _ => Err(CompileError::Error{span:span.clone(), message:format!("BAHFKEJHFKAB")})
        }
    }

    pub fn get_object(&self, id:ScopeId, identifier:Identifier) -> Option<&ObjectInfo> {
        let symbol = self.get_symbol(id, identifier.clone());
        match symbol {
            Some(Symbol::Object(info)) => Some(info),
            _ => None,
        }
    }

    pub fn get_local(&self, id:ScopeId, identifier:Identifier) -> Option<&LocalInfo> {
        let symbol = self.get_symbol(id, identifier.clone());
        match symbol {
            Some(Symbol::Local(info)) => Some(info),
            _ => None,
        }
    }

    pub fn get_shape(&self, id:ScopeId, identifier:Identifier) -> Option<&ShapeInfo> {
        println!("Getting {}: {:?}, using:{:?}", identifier, self.get_scope(id), self.get_using(id));

        let using = self.get_using(id);
        let namespaces = self.loader.get_namespaces_matching(identifier, using.clone());

        if namespaces.len() <= 0 { 
            println!("Couldn't find any namespaces: {:?}", using);
            return None 
        }
        if namespaces.len() > 1 {
           // return Err(CompileError::AmbiguousCall{span, found:namespaces.map(|(id, _)| id).collect()})
        }
        let namespace = &namespaces[0];

        return self.shapes.get(&namespace.id);
    }

    pub fn get_azimuth(&self, span:&Span, id:ScopeId, identifier:Identifier, qualifier: Option<NamespaceId>, signature: Option<FunctionSignature>) -> Result<Option<&AzimuthInfo>, CompileError> {
        let mut using = self.get_using(id).clone();

        let found = match qualifier {
            // No qualifier
            None => self.loader.get_azimuths(identifier.clone(), using.clone()),

            // Qualifier
            Some(name) => {
                let found = self.loader.get_azimuths(identifier.clone(), [name.clone()].to_vec());
                if found.len() >= 1 {
                    found
                } else {
                    // Attach qualifier to using
                    for namespace in &mut using {
                        *namespace = format!("{}::{}", namespace.clone(), name);
                    }

                    self.loader.get_azimuths(identifier.clone(), using.clone())
                }
            }
        };

        println!("Looking for {:?}: Found: {:?} using {:?}", identifier, found.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>(), using);

        if found.len() <= 0 { return Ok(None) }
        else if found.len() > 1 {
            match signature {
                None => return Err(CompileError::AmbiguousCall { span:span.clone(), found: found.iter().map(|(f, _)| f.clone()).collect() }),
                Some(signature) => {
                    let mut matches = Vec::new(); 

                    // Iter through possible functions
                    for (path, azimuth) in &found {
                        let test = self.azimuths.get(&azimuth.id);

                        // Get info
                        let info = match test {
                            None => continue,
                            Some(az) => az
                        };

                        // If function signature matches, mark it
                        match &info.default_value {
                            None => continue,
                            Some(val) => {
                                match val.kind() {
                                    ValueKind::Function(sig) if *sig == signature => matches.push((info, path)),
                                    _ => continue
                                }
                            }
                        };
                    }

                    // Return only match
                    if matches.len() == 1 { 
                        return Ok(Some(matches[0].0)) 
                    } else { return Err(CompileError::AmbiguousCall { span:span.clone(), found: matches.iter().map(|(_, f)| f.clone().clone()).collect() }) }
                }
            }
        }
        
        let azimuth = found[0].1.id;
        return Ok(self.azimuths.get(&azimuth));
    }

    fn declare_generic(&mut self, shape: &ShapeExpression, dependencies: &Vec<ShapeExpression>, scope: ScopeId) -> Result<(), CompileError> {
        let identifier = match shape {
            ShapeExpression::Shape(_, identifier) => identifier,
            _ => todo!()
        };
        
        let scope = self.get_scope_mut(scope);
        let id = scope.generics;
        scope.generics += 1;

        let symbol = Symbol::Generic(GenericInfo{id, name:identifier.clone(), known_shapes:Vec::new()});
        scope.symbols.insert(identifier.clone(), symbol);

        // Resolve dependencies
        let scope_id = scope.id;
        let mut known_shapes = Vec::new();
        for dependency in dependencies {
            let resolved = self.resolve_shape_expression(dependency.clone(), scope_id)?;
            known_shapes.push(resolved.kind());
        }

        // Add dependencies
        let scope = self.get_scope_mut(scope_id);
        let symbol = scope.symbols.get_mut(identifier).unwrap();
        match symbol {
            Symbol::Generic(info) => info.known_shapes = known_shapes,
            _ => panic!("ERRORCODE 511")
        }

        Ok(())
    }

    fn declare_object(&mut self, scope: ScopeId, name: Identifier, shape: ResolvedShapeExpression, static_origin:Option<NamespaceId>) -> (ObjectInfo, LocalId) {
        let id = self.next_object_id.clone();
        self.next_object_id += 1;

        let mut known_shapes = Vec::new();
        known_shapes.push(shape.kind());

        match &shape.kind() {
            // Add inherited shapes
            ValueKind::Shape(inst) => {
                match self.shapes.get(&inst.id) {
                    Some(known) => {
                        for parent in &known.parent_ids {
                            let inst = ShapeInstance{id:*parent, generics:inst.generics.clone()};
                            known_shapes.push(ValueKind::Shape(inst))
                        }
                    }
                    None => {}
                }
            }
            _ => {}
        }

        let local = self.declare_local(scope, name.clone(), known_shapes.clone());

        (ObjectInfo{id: id, name: name, known_shapes:known_shapes.clone(), static_origin}, local)
    }

    fn declare_local(&mut self, scope: ScopeId, name: Identifier, known_shapes: Vec<ValueKind>) -> LocalId {
        let scope = self.get_scope_mut(scope);

        let id = scope.locals;
        scope.locals += 1;
        let symbol = Symbol::Local(LocalInfo{id: id, name: name.clone(), known_shapes});

        scope.symbols.insert(name.clone(), symbol);
        id
    }

    fn get_member_without_qualifier(&self, target_kind: &ValueKind, member:Identifier, span:Span, scope:ScopeId) -> Result<&AzimuthInfo, CompileError> {
        let member = match target_kind {
            ValueKind::Object(known_types) => {
                let mut found = Vec::new();
                
                // Check all known shapes for azimuth
                for known in known_types {
                    let name = match known {
                        ValueKind::Shape(inst) => {
                            let shape = match self.shapes.get(&inst.id) {
                                Some(shape) => shape,
                                None => todo!()
                            };
                            shape.name.clone()
                        }
                        other => {
                            let shape = self.get_primitive_shape(other)?.unwrap();
                            shape.name.clone()
                        }
                    };

                    println!("Trying to find azimuth: {}", name);
                    match self.get_azimuth(&span, scope, member.clone(), Some(name), None)? {
                        Some(info) => found.push(info),
                        None => {}
                    }
                }

                if found.len() <= 0 {
                    println!("Heres your problem");
                    return Err(CompileError::UndefinedSymbol { span, name: member })
                } else if found.len() > 1 {
                    return Err(CompileError::AmbiguousCall { span, found:found.iter().map(|az| az.name.clone()).collect() })
                }

                found[0]
            }
            ValueKind::Shape(inst) => {
                let shape = match self.shapes.get(&inst.id) {
                    Some(shape) => shape,
                    None => todo!()
                };
                match self.get_azimuth(&span, scope, member.clone(), Some(shape.name.clone()), None)?{
                    Some(info) => info,
                    None => match self.get_azimuth(&span, scope, member.clone(), None, None)? {
                        Some(info) => info,
                        None => {
                            println!("ERRORCODE");
                            return Err(CompileError::UndefinedSymbol { span, name: member })
                        }
                    }
                }
            }
            ValueKind::String | ValueKind::Bool | ValueKind::Array(_) 
            | ValueKind::Set(_)  | ValueKind::Dict(_,_) 
            | ValueKind::Number(_) | ValueKind::Range(_) => {
                let name = match self.get_primitive_shape(target_kind)? {
                    Some(info) => info.name.clone(),
                    None => return Err(CompileError::UndefinedSymbol { span, name: member })
                };
                match self.get_azimuth(&span, scope, member.clone(), Some(name), None)? {
                    Some(info) => info,
                    None => return Err(CompileError::UndefinedSymbol { span, name: member })
                }
            }
            ValueKind::Option(value) => self.get_member_without_qualifier(value, member, span, scope)?,
            ValueKind::Pointer(value) => self.get_member_without_qualifier(value, member, span, scope)?,
            other => match self.get_azimuth(&span, scope, member.clone(), None, None)? {
                Some(info) => info,
                None => {
                    println!("Called azimuth on a {:?}", other);
                    return Err(CompileError::UndefinedSymbol { span, name: member })
                }
            },
        };
        Ok(member)
    }

    pub fn resolve_expression(&mut self, expression:Expression, scope:ScopeId) -> Result<ResolvedExpression, CompileError> {
        match expression {
            Expression::Value(span, value) => Ok(ResolvedExpression::Value(span, value)),
            Expression::Array(span, expressions, kind) => {
                let mut values = Vec::new();
                for item in expressions {
                    values.push(self.resolve_expression(item, scope.clone())?);
                }

                let kind = match kind {
                    Some(k) => k,
                    None => match values.first() {
                        Some(val) => val.kind(),
                        None => ValueKind::None,
                    }
                };

                Ok(ResolvedExpression::Array(span, values, kind))
            },
            Expression::Set(span, expressions, kind) => {
                let mut values = HashSet::new();
                for item in expressions {
                    values.insert(self.resolve_expression(item, scope.clone())?);
                }

                let kind = match kind {
                    Some(k) => k,
                    None => match values.iter().find(|_|true) {
                        Some(val) => val.kind(),
                        None => ValueKind::None,
                    }
                };

                Ok(ResolvedExpression::Set(span, values.iter().cloned().collect(), kind))
            },
            Expression::Dict(span, expressions, key_kind, value_kind) => {
                let mut values = HashMap::new();
                for (key, val) in expressions {
                    values.insert(self.resolve_expression(key, scope.clone())?, self.resolve_expression(val, scope.clone())?);
                }

                let key_kind = match key_kind {
                    Some(k) => k,
                    None => match values.keys().find(|_|true) {
                        Some(val) => val.kind(),
                        None => ValueKind::None,
                    }
                };

                let value_kind = match value_kind {
                    Some(k) => k,
                    None => match values.values().find(|_|true) {
                        Some(val) => val.kind(),
                        None => ValueKind::None,
                    }
                };

                Ok(ResolvedExpression::Dict(span, values.iter().map(|(k,v)| (k.clone(),v.clone())).collect(), key_kind, value_kind))
            },
            Expression::Range(span, from, to) => {
                todo!()
            }

            Expression::StringFormat(span, expressions) => {
                let mut resolved = Vec::new();
                for expr in expressions {
                    resolved.push(self.resolve_expression(expr, scope)?);
                }

                Ok(ResolvedExpression::StringFormat(span, resolved))
            }

            Expression::Option(span, option) => todo!(),

            Expression::Shape(span, shape) => Ok(ResolvedExpression::Shape(span, self.resolve_shape_expression(shape, scope)?)),
            
            Expression::Variable(span, k) => {
                match self.get_symbol(scope, k.clone()) {
                    Some(Symbol::Object(info)) => Ok(ResolvedExpression::Variable(span, Symbol::Object(info.clone()))),
                    Some(Symbol::Local(info)) => Ok(ResolvedExpression::Variable(span, Symbol::Local(info.clone()))),
                    _ => {
                        println!("Not object or local, must be shape");
                        let found = self.loader.get_namespaces_matching(k.clone(), self.get_using(scope));

                        if found.len() == 1 {

                            match &self.namespace_static_info.get(&found[0].id) {
                                Some((info, _)) => Ok(ResolvedExpression::Variable(span, Symbol::Object(info.clone()))),
                                None => Err(CompileError::Error { span, message: format!("Shape {} does not have static instance", k) })
                            }

                        } else if found.len() > 1 {
                            
                            return Err(CompileError::AmbiguousCall{span, found:found.into_iter().map(|namespace| namespace.name.clone()).collect()})

                        } else {
                            println!("Not object, local, or shape, must be namespace azimuth");
                            match self.get_azimuth(&span, scope, k.clone(), None, None)? {
                                Some(info) => { 

                                    let (static_info, _) = match self.namespace_static_info.get(&info.shape_id) {
                                        Some(info) => info,
                                        None => return Err(CompileError::UndefinedSymbol { span, name: k })
                                    };

                                    Ok(ResolvedExpression::MemberAccess{
                                        span:span.clone(), 
                                        target: Box::new(ResolvedExpression::Variable(span, Symbol::Object(static_info.clone()))), 
                                        member:info.clone(), 
                                        optional:false, chained:false
                                    })
                                }
                                _ => Err(CompileError::UndefinedSymbol { span, name: k })
                            }
                        }
                    }
                }
            }

            Expression::UnaryOp { span, operator, operand } => {
                match self.resolve_expression(*operand, scope)? {
                    // Bool Optimization
                    ResolvedExpression::Value(span, Value::Bool(val)) => match operator {
                        Operator::Not => Ok(ResolvedExpression::Value(span, (!val).into())),
                        operator => Err(CompileError::InvalidUnaryOp { span, operator, operand:val.into()}),
                    },

                    // Int Optimization
                    ResolvedExpression::Value(span, Value::Number(val)) => {
                        let val = val.to_i32();
                        match operator {
                            Operator::Inc => Ok(ResolvedExpression::Value(span, (val + 1).into())),
                            Operator::Dec => Ok(ResolvedExpression::Value(span, (val - 1).into())),
                            Operator::BWNot => Ok(ResolvedExpression::Value(span, (!val).into())),
                            Operator::Sub => Ok(ResolvedExpression::Value(span, (-val).into())),
                            operator => Err(CompileError::InvalidUnaryOp { span, operator, operand:val.into()}),
                        }
                    },

                    // String Optimization
                    ResolvedExpression::Value(span, Value::String(val)) => match operator {
                        Operator::Len => Ok(ResolvedExpression::Value(span, (val.len() as i32).into())),
                        operator => Err(CompileError::InvalidUnaryOp { span, operator, operand:val.into()}),
                    },

                    // Array Optimization
                    ResolvedExpression::Value(span, Value::Array(vec, kind)) => match operator {
                        Operator::Len => Ok(ResolvedExpression::Value(span, (vec.len() as i32).into())),
                        operator => Err(CompileError::InvalidUnaryOp { span, operator, operand:Value::Array(vec, kind)}),
                    },

                    // Set Optimization
                    ResolvedExpression::Value(span, Value::Set(vec, kind)) => match operator {
                        Operator::Len => Ok(ResolvedExpression::Value(span, (vec.len() as i32).into())),
                        operator => Err(CompileError::InvalidUnaryOp { span, operator, operand:Value::Set(vec, kind)}),
                    },

                    // Dict Optimization
                    ResolvedExpression::Value(span, Value::Dict(vec, key_kind, val_kind)) => match operator {
                        Operator::Len => Ok(ResolvedExpression::Value(span, (vec.len() as i32).into())),
                        operator => Err(CompileError::InvalidUnaryOp { span, operator, operand:Value::Dict(vec, key_kind, val_kind)}),
                    },

                    // Default
                    operand => Ok(ResolvedExpression::UnaryOp{span, operator, operand: Box::new(operand)})
                }
            }

            Expression::BinaryOp { span, left, operator, right } => {
                match (self.resolve_expression(*left, scope)?, self.resolve_expression(*right, scope)?) {
                    // Bool Optimization
                    (ResolvedExpression::Value(span, Value::Bool(left)), 
                        ResolvedExpression::Value(_, Value::Bool(right))) => match operator {
                        Operator::Equal => Ok(ResolvedExpression::Value(span, (left == right).into())),
                        Operator::NEqual => Ok(ResolvedExpression::Value(span, (left != right).into())),
                        Operator::And => Ok(ResolvedExpression::Value(span, (left && right).into())),
                        Operator::Or => Ok(ResolvedExpression::Value(span, (left || right).into())),
                        other => Err(CompileError::InvalidBinaryOp { span, operator:other, left:left.into(), right:right.into() }),
                    },
                    
                    // Int Optimization
                    (ResolvedExpression::Value(span, Value::Number(left)),
                    ResolvedExpression::Value(_, Value::Number(right))) => {
                        let kind = Number::promote_kind(left.num_kind(), right.num_kind());
                        match kind {
                            NumKind::Float64 => float_binop!(span, left.to_f64(), right.to_f64(), operator, f64),
                            NumKind::Float32 => float_binop!(span, left.to_f32(), right.to_f32(), operator, f32),
                            NumKind::UInt64  => numeric_binop!(span, left.to_u64(), right.to_u64(), operator, u64),
                            NumKind::Int64   => numeric_binop!(span, left.to_i64(), right.to_i64(), operator, i64),
                            NumKind::UInt32  => numeric_binop!(span, left.to_u32(), right.to_u32(), operator, u32),
                            NumKind::Int32   => numeric_binop!(span, left.to_i32(), right.to_i32(), operator, i32),
                            NumKind::UInt16  => numeric_binop!(span, left.to_u16(), right.to_u16(), operator, u16),
                            NumKind::Int16   => numeric_binop!(span, left.to_i16(), right.to_i16(), operator, i16),
                            NumKind::UInt8  => numeric_binop!(span, left.to_u8(), right.to_u8(), operator, u8),
                            NumKind::Int8   => numeric_binop!(span, left.to_i8(), right.to_i8(), operator, i8),
                            _ => panic!("Couldnt do number conversion")
                        }
                    }

                    // DQuestion optimization
                    (ResolvedExpression::Value(span, Value::None), right) => match operator {
                        Operator::DQuestion => Ok(right),
                        _ => Ok(ResolvedExpression::BinaryOp{span:span.clone(), left: Box::new(ResolvedExpression::Value(span, Value::None)), operator, right: Box::new(right)})
                    },

                    // Default
                    (left, right) => {
                        Ok(ResolvedExpression::BinaryOp{span, left: Box::new(left), operator, right: Box::new(right)})
                    }
                }
            }

            Expression::Ternary { span, condition, true_expr, else_expr  } => {
                let condition = self.resolve_expression(*condition, scope)?;
                if condition.kind() != ValueKind::Bool {
                    return Err(CompileError::ExpectedBoolCondition { span, found: condition.kind() })
                }

                let true_expr = self.resolve_expression(*true_expr, scope)?;
                let else_expr = self.resolve_expression(*else_expr, scope)?;

                if !true_expr.kind().is_assignable_from(else_expr.kind()) {
                    return Err(CompileError::TypeMismatch { span, expected: true_expr.kind(), found: else_expr.kind(), loc: format!("ternary operation") } )
                }

                Ok(ResolvedExpression::Ternary{span, condition:Box::new(condition), true_expr:Box::new(true_expr), else_expr:Box::new(else_expr)})
            }

            Expression::MemberAccess{ span, target, qualifier, member, optional, chained} => {
                let target = self.resolve_expression(*target, scope)?;

                let member = match qualifier {
                    // Provided qualifier
                    Some(qualifier) => match self.get_azimuth(&span, scope, member.clone(), Some(qualifier), None)? {
                        Some(info) => info,
                        None => return Err(CompileError::UndefinedSymbol { span, name: member }),
                    },

                    // No provided qualifier
                    None => match &target {
                        ResolvedExpression::Variable(_, Symbol::Object(info)) if info.static_origin.is_some() => 
                            match self.get_azimuth(&span, scope, member.clone(), info.static_origin.clone(), None)? {
                                Some(info) => info,
                                None => return Err(CompileError::UndefinedSymbol { span, name: member }),
                            },
                        _ => self.get_member_without_qualifier(&target.kind(), member, span.clone(), scope)?
                    }
                };

                Ok(ResolvedExpression::MemberAccess{span, 
                    target:Box::new(target),
                    member:member.clone(),
                    optional, chained
                })
            }

            Expression::ArrayAccess{ span, target, index, optional, chained} => {
                let target = self.resolve_expression(*target, scope)?;
                let index = self.resolve_expression(*index, scope)?;

                if !index.kind().is_assignable_from(ValueKind::Number(NumKind::Int32)) {
                    return Err(CompileError::TypeMismatch { span, expected: ValueKind::Number(NumKind::Int32), found: index.kind(), loc:format!("array index") });
                }

                Ok(ResolvedExpression::ArrayAccess{span, target:Box::new(target), index:Box::new(index), optional, chained})
            }

            Expression::FunctionCall { span, caller, target, args, optional, chained } => {
                let target = self.resolve_expression(*target, scope)?;
                let caller = self.resolve_expression(*caller, scope)?;

                let func = match target.kind() {
                    ValueKind::Function(func) => *func,
                    other => return Err(CompileError::Error { span, message:format!("Expected function, got {:?}", other) })
                };

                let mut resolved_args = Vec::new();

                // Add self
                if func.has_self {
                    resolved_args.push(caller);
                }

                for index in 0..args.len() {
                    let arg = self.resolve_expression(args.get(index).unwrap().clone(), scope)?;

                    match func.input_types.get(if func.has_self { index + 1 } else { index }) {
                        Some(param) => {
                            // Type check
                            if !arg.kind().is_assignable_from(param.clone()) {
                                return Err(CompileError::TypeMismatch { span, expected: param.clone(), found: arg.kind(), loc:format!("function param")})
                            }

                            // Accept arg
                            resolved_args.push(arg);
                        }
                        None => return Err(CompileError::Error { span, message:format!("Too few args: {}, needed: {}", args.len(), func.input_types.len()) }),
                    }
                }
                if resolved_args.len() < func.input_types.len() {
                    return Err(CompileError::Error { span, message:format!("Too few args: {}, needed: {}", args.len(), func.input_types.len()) })
                }

                Ok(ResolvedExpression::FunctionCall{span, target:Box::new(target), args:resolved_args, optional, chained})
            },

            Expression::Function {span, has_self, input_types, output_type, func, captures } => {

                let new_scope = self.create_scope(scope);

                let mut resolved_inputs = Vec::new();
                for input in input_types {
                    let value_type = self.resolve_shape_expression(input.value_type, scope)?;
                    let id = self.declare_local(new_scope, input.identifier, [value_type.kind()].to_vec());

                    let resolved_input = ResolvedFunctionParameter{ shape:value_type, local:id };

                    resolved_inputs.push(resolved_input);
                }

                let mut resolved_captures = Vec::new();
                for capture in captures {
                    let symbol = self.get_symbol(scope, capture);

                    let id = match symbol {
                        Some(Symbol::Local(info)) => info.id,
                        _ => todo!()
                    };
                    resolved_captures.push(id);
                }

                let output_type = self.resolve_shape_expression(output_type, new_scope)?;

                let body = match func.as_ref() {
                    None => None,
                    Some(FunctionBody::Script(func)) => {
                        let statement = self.resolve_statement(func.clone(), new_scope)?;
                        match statement.walk_returns(output_type.kind())? {
                            ReturnStatus::Always => {},
                            _ => if !output_type.kind().is_assignable_from(ValueKind::None) {
                                return Err(CompileError::IncorrectReturn { span, expected: output_type.kind(), found: ValueKind::None })
                            }
                        }
                        Some(ResolvedFunctionBody::Script(statement))
                    },
                    Some(FunctionBody::Intrinsic(func)) => Some(ResolvedFunctionBody::Intrinsic(*func)),
                };

                Ok(ResolvedExpression::Function{span, has_self, 
                    input_types:resolved_inputs, 
                    output_type, 
                    func:Box::new(body),
                    captures:resolved_captures
                })
            }

            Expression::Reflection(span, expr) => {
                let resolved = self.resolve_expression(*expr, scope)?;
                match resolved {
                    ResolvedExpression::MemberAccess { span, target, member, optional, chained } => {
                        
                    }
                    _ => todo!()
                }
                
                todo!()
            }

        }
    }

    pub fn resolve_shape_expression(&mut self, expression:ShapeExpression, scope:ScopeId) -> Result<ResolvedShapeExpression, CompileError> {
        match expression{
            ShapeExpression::Shape(span, k) => match self.get_shape(scope, k.clone()) {
                Some(info) => Ok(ResolvedShapeExpression::Simple(span, info.clone())),
                _ => { 
                    match self.get_symbol(scope, k.clone()) {
                        Some(Symbol::Generic(info)) => Ok(ResolvedShapeExpression::Parameter(span, info.name.clone())),
                        _ => Err(CompileError::UndefinedSymbol { span, name: format!("{} A", k) })
                    }
                }
            }
            ShapeExpression::Primitive(span, kind) => Ok(ResolvedShapeExpression::Primitive(span, kind)),
            ShapeExpression::Array(span, expr) => Ok(ResolvedShapeExpression::Array(span, Box::new(self.resolve_shape_expression(*expr, scope)?))),
            ShapeExpression::Set(span, expr) => Ok(ResolvedShapeExpression::Set(span, Box::new(self.resolve_shape_expression(*expr, scope)?))),
            ShapeExpression::Dict(span, key, val) => Ok(ResolvedShapeExpression::Dict(span, Box::new(self.resolve_shape_expression(*key, scope)?), Box::new(self.resolve_shape_expression(*val, scope)?))),
            ShapeExpression::Optional(span, expr) => Ok(ResolvedShapeExpression::Optional(span, Box::new(self.resolve_shape_expression(*expr, scope)?))),
            ShapeExpression::Applied { span, base, args } => {
                let base = self.get_shape(scope, base.clone())
                    .expect(format!("Shape not found in scope: {:?}", base).as_str()).clone();

                let mut resolved = Vec::new();
                for arg in args{
                    resolved.push(self.resolve_shape_expression(arg, scope)?);
                }
                Ok(ResolvedShapeExpression::Applied{ span, base, args: resolved})
            },
            ShapeExpression::FunctionSignature(span, signature) => {
                let output_type = self.resolve_shape_expression(signature.output_type, scope)?.kind();

                let mut input_types = Vec::new();
                for input in signature.input_types {
                    input_types.push(self.resolve_shape_expression(input, scope)?.kind())
                }

                let info = FunctionSignature{input_types, output_type, has_self: signature.has_self };

                Ok(ResolvedShapeExpression::Primitive(span, ValueKind::Function(Box::new(info))))
            }
        }
    }

    pub fn resolve_mapping(&mut self, span:Span, mapping:RawMapping, scope:ScopeId) -> Result<ResolvedMapping, CompileError> {
        let from = if let Some(symbol) = self.get_azimuth(&span, scope, mapping.from_slot.clone(), None, None)?{
            symbol.clone()
        } else if let Some(symbol) = self.get_azimuth(&span, scope, mapping.from_slot.clone(), Some(mapping.shape.get_identifier().clone()), None)?{
            symbol.clone()
        } else { return Err(CompileError::UndefinedSymbol { span, name: mapping.from_slot }); };
        
        let to = if let Some(symbol) = self.get_azimuth(&span, scope, mapping.to_slot.clone(), None, None)?{
            symbol.clone()
        } else if let Some(symbol) = self.get_azimuth(&span, scope, mapping.from_slot.clone(), Some(mapping.shape.get_identifier()), None)?{
            symbol.clone()
        } else { return Err(CompileError::UndefinedSymbol { span, name: mapping.to_slot }); };

        Ok(ResolvedMapping{from, to, kind:mapping.kind})
    }

    pub fn get_azimuth_with_id(&self, scope:ScopeId, id:AzimuthId) -> Option<&AzimuthInfo> {
        self.azimuths.get(&id)
    }

    pub fn resolve_statement(&mut self, statement:Statement, scope:ScopeId) -> Result<ResolvedStatement,CompileError> {
        match statement {
            Statement::Expression {span, expr} => Ok(ResolvedStatement::Expression{span, expr:self.resolve_expression(expr, scope)?}),
            
            Statement::Using { .. } => {
                Ok(ResolvedStatement::Block(Vec::new()))
            }
            Statement::Namespace{ .. } => {
                Ok(ResolvedStatement::Block(Vec::new()))
            },
            Statement::DeclareAzimuth{ .. } => {
                Ok(ResolvedStatement::Block(Vec::new()))
            },
            Statement::DeclareShape { .. } => {
                Ok(ResolvedStatement::Block(Vec::new()))
            }

            Statement::DeclareObject { span, name, shape } => {
                let my_scope = self.get_scope_mut(scope);
                if my_scope.symbols.contains_key(&name) { return Err(CompileError::DuplicateSymbol{span, name}); }

                let shape = self.resolve_shape_expression(shape, scope)?;
    
                let (info, local) = self.declare_object(scope, name, shape.clone(), None);
                Ok(ResolvedStatement::DeclareObject{ span, local, info, shape })
            }
            Statement::DeclareLocal { span, name, value } => {
                let my_scope = self.get_scope_mut(scope);
                if my_scope.symbols.contains_key(&name) { return Err(CompileError::DuplicateSymbol{span, name}); }

                let value = self.resolve_expression(value, scope)?;
                let kind = ResolvedShapeExpression::Primitive(span.clone(), value.kind());
    
                let id = self.declare_local(scope, name.clone(), [kind.kind()].to_vec());
                let mut kinds = Vec::new();
                kinds.push(value.kind());
                let info = LocalInfo{ id, name, known_shapes: kinds };
                Ok(ResolvedStatement::DeclareLocal{ span, info, value })
            }
            Statement::Detach { span, object, shape } => {
                let shape = self.resolve_shape_expression(shape, scope)?;

                Ok(ResolvedStatement::Detach{ span, 
                    object: self.resolve_expression(object, scope)?, 
                    shape,
                })
            }
            Statement::AddMapping { span, object, mapping } => 
                Ok(ResolvedStatement::AddMapping{ span:span.clone(), 
                    object: self.resolve_expression(object, scope)?, 
                    mapping: self.resolve_mapping(span, mapping, scope)?,
                }),
            Statement::Attach { span, object, shape, mappings } => {
                let object = self.resolve_expression(object, scope)?;

                let mut resolved_mappings = Vec::new();
                for mapping in mappings {
                    resolved_mappings.push(self.resolve_mapping(span.clone(), mapping, scope)?);
                }

                // Validate abstract
                let shape = self.resolve_shape_expression(shape, scope)?;
                let (azimuths, defaults) = match &shape {
                    ResolvedShapeExpression::Simple(span, info) => (info.azimuths.clone(), info.mappings.clone()),
                    _ => (Vec::new(), Vec::new()),
                };
                for id in azimuths {
                    // Find az with id
                    if let Some(found) = self.get_azimuth_with_id(scope, id) {
                        if found.flags.is_abstract {
                            match defaults.iter().find(|mapping| mapping.from.id == id) {
                                Some(_) => continue,
                                None => {}
                            }
                            match resolved_mappings.iter().find(|mapping| mapping.from.id == id) {
                                Some(_) => continue,
                                None => return Err(CompileError::Error { span, message:format!("Abstract attached without mapping") })
                            }
                        }
                    }
                }

                // Add known types
                match &object {
                    ResolvedExpression::Variable(_, Symbol::Local(info)) => {
                        self.add_local_known_type(&span, scope, info.id, shape.clone())?;

                        match &shape {
                            ResolvedShapeExpression::Simple(_, info) => {
                                for parent_id in &info.parent_ids {
                                    let parent = self.shapes.get(&parent_id).unwrap();
                                    self.add_local_known_type(&span, scope, info.id, ResolvedShapeExpression::Simple(span.clone(), parent.clone()))?;
                                }
                            }
                            ResolvedShapeExpression::Applied{span, base, args} => {
                                for parent_id in &base.parent_ids {
                                    let parent = self.shapes.get(&parent_id).unwrap();
                                    self.add_local_known_type(&span, scope, info.id, ResolvedShapeExpression::Applied{span:span.clone(), base:parent.clone(), args:args.clone()})?;
                                }
                            }
                            other => panic!("Got {:?}", other)
                        };
                        
                    }
                    other => panic!("Cant attach shape to {:?}", other)
                }

                Ok(ResolvedStatement::Attach{ span, object, shape, mappings: resolved_mappings})
            },
            Statement::Print { span, expr } => Ok(ResolvedStatement::Print{span, expr: self.resolve_expression(expr, scope)?}),
            Statement::Seal { span, target } => {
                let target = self.resolve_expression(target, scope)?;
                match target.kind() {
                    ValueKind::Shape(_) => {},
                    _ => return Err(CompileError::Error { span, message: format!("{} ({:?}) is not sealable", target.get_name(), target.kind()) })
                }

                Ok(ResolvedStatement::Seal{span, target})
            }

            Statement::If { span, condition, true_statement, else_statement } => {
                let resolved_condition = self.resolve_expression(condition, scope)?;
                if resolved_condition.kind() != ValueKind::Bool {
                    return Err(CompileError::ExpectedBoolCondition { span, found: resolved_condition.kind() })
                }

                let resolved_true = self.resolve_statement(*true_statement, scope)?;
                let resolved_else = if let Some(statement) = else_statement {
                    Some(Box::new(self.resolve_statement(*statement, scope)?))
                } else { None };

                Ok(ResolvedStatement::If{span, condition: resolved_condition, true_statement: Box::new(resolved_true), else_statement: resolved_else})
            }

            Statement::Switch { span, target, branch_statements, else_statement } => {
                let target = self.resolve_expression(target, scope)?;
                
                let mut resolved_branches = Vec::new();
                for (expr, statement) in branch_statements {
                    let resolved_expr = self.resolve_expression(expr, scope)?;
                    let resolved_statement = self.resolve_statement(statement, scope)?;
                    resolved_branches.push((resolved_expr, resolved_statement));
                }

                let else_statement = match else_statement {
                    None => None,
                    Some(statement) => Some(Box::new(self.resolve_statement(*statement, scope)?))
                };

                Ok(ResolvedStatement::Switch{span, target, branch_statements:resolved_branches, else_statement })
            }

            Statement::While { span, condition, statement } => {
                let resolved_condition = self.resolve_expression(condition, scope)?;
                if resolved_condition.kind() != ValueKind::Bool {
                    return Err(CompileError::ExpectedBoolCondition { span, found: resolved_condition.kind() })
                }

                let resolved_statement = self.resolve_statement(*statement, scope)?;
                Ok(ResolvedStatement::While{span, condition: resolved_condition, statement: Box::new(resolved_statement)})
            }
            Statement::Try { span, try_statement, catch_statement } => {
                let resolved_try = self.resolve_statement(*try_statement, scope)?;

                let resolved_catch = if let Some(statement) = *catch_statement {
                    Some(self.resolve_statement(statement, scope)?)
                } else { None };

                Ok(ResolvedStatement::Try{span, try_statement:Box::new(resolved_try), catch_statement:Box::new(resolved_catch)})
            }
            Statement::For { span, local, target, statement } => {
                let target = self.resolve_expression(target, scope)?;

                let iterable_type = match target.kind() {
                    ValueKind::Array(kind) => ShapeExpression::Primitive(span.clone(), *kind),
                    ValueKind::Set(kind) => ShapeExpression::Primitive(span.clone(), *kind),
                    ValueKind::Range(kind) => ShapeExpression::Primitive(span.clone(), ValueKind::Number(kind)),
                    ValueKind::String => ShapeExpression::Primitive(span.clone(), ValueKind::String),
                    other => return Err(CompileError::Error{span, message:format!("Expected array or range in for loop declaration, got {:?}",other)}),
                };

                let new_scope = self.create_scope(scope);
                
                let resolved_type = self.resolve_shape_expression(iterable_type, scope)?;
                let id = self.declare_local(new_scope, local, [resolved_type.kind()].to_vec()).clone();

                let statement = self.resolve_statement(*statement, new_scope)?;

                Ok(ResolvedStatement::For{span, local:id, target, statement:Box::new(statement)})
            }
            
            Statement::ForInc { span, local, start, cond, inc, statement } => {
                let resolved_start = self.resolve_expression(start, scope)?;
                if !resolved_start.kind().is_assignable_from(ValueKind::Number(NumKind::Any)) {
                    return Err(CompileError::TypeMismatch { span, expected: ValueKind::Number(NumKind::Any), found: resolved_start.kind(), loc: format!("for loop start value") } )
                }
                let resolved_condition = self.resolve_expression(cond, scope)?;
                if !resolved_condition.kind().is_assignable_from(ValueKind::Bool) {
                    return Err(CompileError::ExpectedBoolCondition { span, found: resolved_condition.kind() })
                }

                let new_scope = self.create_scope(scope);
                
                let resolved_type = ResolvedShapeExpression::Primitive(span.clone(), ValueKind::Number(NumKind::Int32));
                let id = self.declare_local(new_scope, local, [resolved_type.kind()].to_vec()).clone();

                let inc = self.resolve_statement(*inc, new_scope)?;
                let statement = self.resolve_statement(*statement, new_scope)?;

                Ok(ResolvedStatement::ForInc{span, local:id, start:resolved_start, cond:resolved_condition, inc:Box::new(inc), statement:Box::new(statement)})
            }

            Statement::Assign {span, target, value } => {
                let target = self.resolve_expression(target, scope)?;

                if !target.is_assignable() {
                    return Err(CompileError::Error { span, message:format!("{:?} is not assignable", target.get_name()) })
                }

                let value = self.resolve_expression(value, scope)?;

                if !target.kind().is_assignable_from(value.kind()) {
                    return Err(CompileError::TypeMismatch { span, expected: target.kind(), found: value.kind(), loc:format!("value assignment") })
                }

                Ok(ResolvedStatement::Assign{ span, target, value })
            }
            
            Statement::AssignAugmented {span, target, value, operator } => {
                let target = self.resolve_expression(target, scope)?;

                if !target.is_assignable() {
                    return Err(CompileError::Error { span, message:format!("{:?} is not assignable", target) })
                }

                let value = self.resolve_expression(value, scope)?;

                let expr = ResolvedExpression::BinaryOp {span:span.clone(), left: Box::new(target.clone()), operator, right: Box::new(value) };
                Ok(ResolvedStatement::Assign { span, target, value: expr })
            }
                
            Statement::Block(statements) => {
                let new_scope = self.create_scope(scope);

                let mut resolved_statements = Vec::new();
                for statement in statements {
                    resolved_statements.push(self.resolve_statement(statement, new_scope)?);
                }
                Ok(ResolvedStatement::Block(resolved_statements))
            }

            Statement::Break{span} => Ok(ResolvedStatement::Break{span}),
            Statement::Continue{span} => Ok(ResolvedStatement::Continue{span}),
            Statement::Return{span, value} => Ok(ResolvedStatement::Return{span, value:self.resolve_expression(value, scope)?}),
            Statement::Throw{span, message} => {
                let message = self.resolve_expression(message, scope)?;
                if message.kind() != ValueKind::String {
                    return Err(CompileError::InvalidThrow { span, found: message.kind() })
                }
                Ok(ResolvedStatement::Throw{span, message})
            }
        }
    }

    fn get_primitive_shape(&self, kind: &ValueKind) -> Result<Option<&ShapeInfo>, CompileError> {
        for shape in self.shapes.values() {
            if shape.name.ends_with(match kind {
                ValueKind::Number(NumKind::Int32) => "::Int32",
                ValueKind::Bool => "::Bool",
                ValueKind::String => "::String",
                ValueKind::Array(_) => "::Array",
                ValueKind::Set(_) => "::Set",
                ValueKind::Dict(_,_) => "::Dict",
                ValueKind::Range(_) => "::Range",
                _ => todo!()
            }) {
                return Ok(Some(shape))
            }
        }
        Ok(None)
    }

    fn resolve_namespace_headers(&mut self, namespace: Namespace, scope: ScopeId) -> Result<(), CompileError> {
        let scope_id = self.create_scope(scope);
        self.namespace_scopes.insert(namespace.id, scope_id);
        
        let scope_mut = self.get_scope_mut(scope_id);
        scope_mut.using.push(namespace.name.clone());

        for dependency in &namespace.dependencies { 
            scope_mut.using.push(dependency.clone());
        }

        for azimuth in &namespace.azimuths {
            self.azimuths.insert(azimuth.id, AzimuthInfo { 
                id: azimuth.id, 
                name: azimuth.name.clone(), 
                flags: azimuth.flags.clone(), 
                shape_id: namespace.id, 
                default_value: None, 
                value_type: ValueKind::None, 
            });
        }
        match namespace.kind {
            NamespaceKind::Atlas => {
                // Using all children
                //let scope = self.get_scope_mut(scope);
                //for child in &namespace.children {
                //    //scope.using.push([child.name.clone()].to_vec());
                //}
            }
            NamespaceKind::Shape{ .. } => {
                self.shapes.insert(namespace.id, ShapeInfo { 
                    id: namespace.id,
                    name: namespace.name.clone(),
                    static_id: None,
                    azimuths: namespace.azimuths.iter().map(|az| az.id).collect(),
                    generics: Vec::new(),
                    parent_ids: Vec::new(),
                    mappings: Vec::new(), 
                });
            }
            _ => {}
        }
        for child_name in &namespace.children {
            let child = self.loader.get_single_namespace(namespace.span.clone(), child_name)?.clone();
            self.resolve_namespace_headers(child, scope_id)?;
        }
        Ok(())
    }

    fn resolve_namespace_shapes(&mut self, namespace: Namespace, scope: ScopeId) -> Result<(), CompileError> {
        let span = namespace.span.clone();

        // Static singleton
        if namespace.has_static() {
            let info = self.declare_object(
                scope, 
                format!("{}::Static", namespace.name), 
                ResolvedShapeExpression::Primitive(span.clone(), ValueKind::Shape(OBJECT_INSTANCE)),
                Some(namespace.name),
            ).0;
            let azimuths = namespace.azimuths.iter().map(|az| az.id).collect();
            self.namespace_static_info.insert(namespace.id, (info.clone(), azimuths));
        }

        match &namespace.kind {
            NamespaceKind::Shape{ generics, mappings, parents } => {
                let shape_scope = *self.namespace_scopes.get(&namespace.id).unwrap_or(&scope);

                // Generics
                let mut resolved_generics = Vec::new();
                for (generic, dependencies) in generics {
                    self.declare_generic(generic, dependencies, shape_scope)?;
                    let resolved_generic = self.resolve_shape_expression(generic.clone(), shape_scope)?;
                    resolved_generics.push(resolved_generic);
                }

                // Inheritance
                let mut parent_ids = Vec::new();
                for parent in parents {
                    match self.get_shape(shape_scope, parent.get_identifier().clone()) {
                        Some(info) => parent_ids.push(info.id),
                        _ => return Err(CompileError::UndefinedSymbol { span, name:parent.get_identifier().clone() })
                    }
                }

                // Mappings
                let mut resolved_mappings = Vec::new();
                for mapping in mappings {
                    println!("Resolving mapping {:?}", mapping);
                    resolved_mappings.push(self.resolve_mapping(span.clone(), mapping.clone(), shape_scope)?);
                }

                // Set info
                match self.shapes.get_mut(&namespace.id) {
                    None => return Err(CompileError::Error{span, message: format!("Dead azimuth: {}", namespace.id)}),
                    Some(info) => {
                        info.generics = resolved_generics;
                        info.mappings = resolved_mappings;
                        info.static_id = match self.namespace_static_info.get(&namespace.id) {
                            None => None,
                            Some((info, _)) => Some(Box::new(info.clone()))
                        };
                        info.parent_ids = parent_ids;
                    }
                }
            }
            _ => {}
        }

        let scope = *self.namespace_scopes.get(&namespace.id).unwrap_or(&scope);
        for child_name in &namespace.children {
            let child = self.loader.get_single_namespace(namespace.span.clone(), child_name)?.clone();
            self.resolve_namespace_shapes(child, scope)?;
        }
        Ok(())
    }

    fn resolve_namespace_kinds(&mut self, namespace: Namespace, scope: ScopeId) -> Result<(), CompileError> {
        let span = namespace.span.clone();

        let scope = *self.namespace_scopes.get(&namespace.id).unwrap_or(&scope);

        for azimuth in &namespace.azimuths {
            let resolved = self.resolve_shape_expression(azimuth.kind.clone(), scope)?;
            match self.azimuths.get_mut(&azimuth.id) {
                None => return Err(CompileError::Error{span, message: format!("Dead azimuth: {}", namespace.id)}),
                Some(info) => {
                    info.value_type = resolved.kind();
                }
            }
        }
        for child_name in &namespace.children {
            let child = self.loader.get_single_namespace(namespace.span.clone(), child_name)?.clone();
            self.resolve_namespace_kinds(child, scope)?;
        }
        Ok(())
    }

    fn resolve_namespace_defaults(&mut self, namespace: Namespace, scope: ScopeId) -> Result<(), CompileError> {
        let scope = *self.namespace_scopes.get(&namespace.id).unwrap_or(&scope);

        for azimuth in &namespace.azimuths {
            let resolved = match &azimuth.default_value {
                None => continue,
                Some(value) => self.resolve_expression(value.clone(), scope)?
            };

            match self.azimuths.get_mut(&azimuth.id) {
                None => return Err(CompileError::Error{span:namespace.span.clone(), message: format!("Dead azimuth: {}", namespace.id)}),
                Some(info) => {
                    info.default_value = Some(Box::new(resolved));
                }
            }
        }
        for child_name in &namespace.children {
            let child = self.loader.get_single_namespace(namespace.span.clone(), child_name)?.clone();
            self.resolve_namespace_defaults(child, scope)?;
        }
        Ok(())
    }

    fn analyze(&mut self) -> Result<Vec<ResolvedStatement>, CompileError> {
        let files = std::mem::take(&mut self.loader.files);
        let load_order = std::mem::take(&mut self.loader.load_order);
        let root = self.loader.namespaces.get(0).unwrap().clone();

        // Resolve namespaces
        //println!("Headers...");
        self.resolve_namespace_headers(root.clone(), 0)?;
        //println!("Done.\n{:?}\n{:?}\nShapes...", self.azimuths, self.shapes);
        self.resolve_namespace_shapes(root.clone(), 0)?;
        //println!("Done.\n{:?}\n{:?}\nKinds...", self.azimuths, self.shapes);
        self.resolve_namespace_kinds(root.clone(), 0)?;
        //println!("Done.\n{:?}\n{:?}\nDefaults...", self.azimuths, self.shapes);
        self.resolve_namespace_defaults(root.clone(), 0)?;
        //println!("Done.\n{:?}\n{:?}\nStatements...", self.azimuths, self.shapes);

        // Resolve statements
        let mut resolved = Vec::new();

        for (loc, namespace, namespace_id) in &load_order {
            let file = match files.get(&loc) {
                Some(file) => file,
                None => panic!("Something bad happened: {:?} wasn't in {:?}", loc, files)
            };

            let new_scope = *self.namespace_scopes.get(&namespace_id).unwrap_or(&0);

            for statement in file {
                resolved.push(self.resolve_statement(statement.clone(), new_scope)?);
                //println!("Scope: {:?}, Root: {:?}", self.get_scope(new_scope), self.get_scope(0));
            }
        }
        
        println!("Done.\n");
    
        //println!("\n Resolved Ast: \n{:?}", resolved);
        Ok(resolved)
    }
}

pub fn analyze(loader: Loader) -> Result<(Vec<ResolvedStatement>, Analyzer), CompileError> {
    let mut analyzer = Analyzer::new(loader);
    Ok((analyzer.analyze()?, analyzer))
}
